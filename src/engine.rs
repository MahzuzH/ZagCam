//! The capture/inference worker. It owns the physical camera and the frame channel; the UI thread
//! only sends it new settings.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nokhwa::Camera;
use nokhwa::pixel_format::RgbFormat;

use crate::capture;
use crate::pipeline::{BackgroundSource, Pipeline};
use crate::settings::{Mode, Settings};
use crate::sink::{MfvcamSink, Sink};

const IDLE_RELEASE: Duration = Duration::from_secs(3);
const CAPTURE_RETRY: Duration = Duration::from_secs(1);
const POLL_IDLE: Duration = Duration::from_millis(50);

pub enum Control {
    Settings(Settings),
    Quit,
}

#[derive(Clone)]
pub struct PreviewFrame {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// Handle to the worker thread.
pub struct Engine {
    control: Sender<Control>,
    camera_in_use: Arc<AtomicBool>,
    error: Arc<Mutex<String>>,
    preview: Arc<Mutex<Option<PreviewFrame>>>,
    preview_wanted: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn spawn(initial: Settings, model: PathBuf) -> Result<Self, String> {
        let (control, commands) = channel();
        let camera_in_use = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(String::new()));
        let preview = Arc::new(Mutex::new(None));
        let preview_wanted = Arc::new(AtomicBool::new(false));
        let join = {
            let camera_in_use = Arc::clone(&camera_in_use);
            let error = Arc::clone(&error);
            let preview = Arc::clone(&preview);
            let preview_wanted = Arc::clone(&preview_wanted);
            std::thread::Builder::new()
                .name("zagcam-engine".into())
                .spawn(move || {
                    worker(
                        initial,
                        model,
                        commands,
                        camera_in_use,
                        error,
                        preview,
                        preview_wanted,
                    );
                })
                .map_err(|why| format!("cannot start the engine thread: {why}"))?
        };
        Ok(Self {
            control,
            camera_in_use,
            error,
            preview,
            preview_wanted,
            join: Some(join),
        })
    }

    pub fn camera_in_use(&self) -> bool {
        self.camera_in_use.load(Ordering::Relaxed)
    }

    pub fn last_error(&self) -> Option<String> {
        let error = self.error.lock().map(|e| e.clone()).unwrap_or_default();
        (!error.is_empty()).then_some(error)
    }

    pub fn set_preview_wanted(&self, wanted: bool) {
        self.preview_wanted.store(wanted, Ordering::Relaxed);
        if !wanted && let Ok(mut slot) = self.preview.lock() {
            *slot = None;
        }
    }

    pub fn preview(&self) -> Option<PreviewFrame> {
        self.preview.lock().ok().and_then(|frame| frame.clone())
    }

    pub fn send(&self, settings: Settings) {
        let _ = self.control.send(Control::Settings(settings));
    }

    pub fn shutdown(&mut self) {
        let _ = self.control.send(Control::Quit);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker(
    initial: Settings,
    model: PathBuf,
    commands: Receiver<Control>,
    camera_in_use: Arc<AtomicBool>,
    error: Arc<Mutex<String>>,
    preview: Arc<Mutex<Option<PreviewFrame>>>,
    preview_wanted: Arc<AtomicBool>,
) {
    let mut desired = initial;
    let mut applied: Option<Settings> = None;
    let mut session: Option<Session> = None;
    let mut sink = MfvcamSink::new();
    let mut last_capture_error: Option<Instant> = None;
    let mut last_apply_error: Option<Instant> = None;

    loop {
        let mut quit = false;
        loop {
            match commands.try_recv() {
                Ok(Control::Settings(settings)) => {
                    desired = settings;
                    applied = None;
                    last_apply_error = None;
                }
                Ok(Control::Quit) | Err(TryRecvError::Disconnected) => {
                    quit = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if quit {
            return;
        }

        let wants = sink.wants_frame();
        camera_in_use.store(wants, Ordering::Relaxed);

        if let Some(current) = session.as_ref()
            && release_due(wants, current.last_active.elapsed())
        {
            println!("no client reading; releasing camera");
            session = None;
            applied = None;
        }
        if !wants {
            std::thread::sleep(POLL_IDLE);
            continue;
        }

        if let Some(current) = session.as_ref()
            && source_changed(&current.key, &desired)
        {
            session = None;
            applied = None;
            last_apply_error = None;
        }

        if session.is_none() {
            match open_session(&desired, &model, &error) {
                Ok(mut opened) => {
                    opened.preview = Arc::clone(&preview);
                    opened.preview_wanted = Arc::clone(&preview_wanted);
                    session = Some(opened);
                    applied = None;
                    last_capture_error = None;
                    last_apply_error = None;
                }
                Err(why) => {
                    if last_capture_error.is_none_or(|at| at.elapsed() >= CAPTURE_RETRY) {
                        report(&error, format!("capture unavailable: {why}; retrying"));
                        last_capture_error = Some(Instant::now());
                    }
                    std::thread::sleep(CAPTURE_RETRY);
                    continue;
                }
            }
        }

        if let Some(current) = session.as_mut() {
            if applied.as_ref() != Some(&desired)
                && last_apply_error.is_none_or(|at| at.elapsed() >= CAPTURE_RETRY)
            {
                match current.apply(&desired) {
                    Ok(()) => {
                        applied = Some(desired.clone());
                        last_apply_error = None;
                        clear(&error);
                    }
                    Err(why) => {
                        report(&error, format!("cannot apply settings: {why}"));
                        last_apply_error = Some(Instant::now());
                    }
                }
            }
            if let Err(why) = current.publish(&mut sink) {
                report(&error, format!("capture stopped: {why}; reopening"));
                session = None;
                applied = None;
            }
        }
    }
}

fn report(error: &Arc<Mutex<String>>, message: String) {
    eprintln!("zagcam: {message}");
    if let Ok(mut slot) = error.lock() {
        *slot = message;
    }
}

fn clear(error: &Arc<Mutex<String>>) {
    if let Ok(mut slot) = error.lock() {
        slot.clear();
    }
}

/// Opens the camera; a bad image background degrades to no effect instead of blocking capture.
fn open_session(
    settings: &Settings,
    model: &Path,
    error: &Arc<Mutex<String>>,
) -> Result<Session, String> {
    match Session::open(settings, model) {
        Err(why) => match fallback_settings(settings) {
            Some(fallback) => {
                report(error, format!("cannot use background: {why}; effect off"));
                Session::open(&fallback, model)
            }
            None => Err(why),
        },
        result => result,
    }
}

fn fallback_settings(settings: &Settings) -> Option<Settings> {
    (settings.mode == Mode::Image).then(|| Settings {
        mode: Mode::Off,
        ..settings.clone()
    })
}

fn camera_key(settings: &Settings) -> (Option<String>, usize, usize, usize) {
    let (width, height) = settings.quality.capture_size();
    (
        settings.camera_name.clone(),
        settings.camera_index,
        width,
        height,
    )
}

fn source_changed(current: &(Option<String>, usize, usize, usize), desired: &Settings) -> bool {
    *current != camera_key(desired)
}

fn release_due(wants: bool, idle: Duration) -> bool {
    !wants && idle >= IDLE_RELEASE
}

struct Session {
    camera: Camera,
    rgb: Vec<u8>,
    pipeline: Option<Pipeline>,
    model: PathBuf,
    key: (Option<String>, usize, usize, usize),
    width: usize,
    height: usize,
    last_active: Instant,
    preview: Arc<Mutex<Option<PreviewFrame>>>,
    preview_wanted: Arc<AtomicBool>,
    preview_counter: u64,
}

impl Session {
    fn open(settings: &Settings, model: &Path) -> Result<Self, String> {
        let cameras = capture::list()?;
        let position = settings
            .camera_index(&cameras)
            .ok_or_else(|| format!("camera {} not found", settings.camera_index))?;
        let source = cameras[position].index().clone();
        let (max_w, max_h) = settings.quality.capture_size();
        let camera = capture::open(&source, max_w, max_h)?;
        let format = camera.camera_format();
        let (width, height) = (format.width() as usize, format.height() as usize);
        let mut pipeline = Pipeline::build(
            settings.mode,
            model,
            width,
            height,
            settings.blur,
            settings.background.as_deref(),
            settings.quality,
        )?;
        if let Some(pipeline) = pipeline.as_mut() {
            pipeline.set_adjustments(
                settings.mirror,
                settings.flip,
                settings.brightness,
                settings.contrast,
            );
        }
        Ok(Self {
            camera,
            rgb: vec![0u8; width * height * 3],
            pipeline,
            model: model.to_path_buf(),
            key: camera_key(settings),
            width,
            height,
            last_active: Instant::now(),
            preview: Arc::new(Mutex::new(None)),
            preview_wanted: Arc::new(AtomicBool::new(false)),
            preview_counter: 0,
        })
    }

    fn apply(&mut self, settings: &Settings) -> Result<(), String> {
        match settings.mode {
            Mode::Off => self.pipeline = None,
            Mode::Blur => match self.pipeline.as_mut() {
                Some(pipeline) => pipeline.configure(BackgroundSource::Blur(settings.blur))?,
                None => {
                    self.pipeline = Pipeline::build(
                        Mode::Blur,
                        &self.model,
                        self.width,
                        self.height,
                        settings.blur,
                        None,
                        settings.quality,
                    )?;
                }
            },
            Mode::Image => match self.pipeline.as_mut() {
                Some(pipeline) => {
                    let path = settings
                        .background
                        .as_deref()
                        .ok_or("image mode needs a background image")?;
                    pipeline.configure(BackgroundSource::Media(path))?;
                }
                None => {
                    self.pipeline = Pipeline::build(
                        Mode::Image,
                        &self.model,
                        self.width,
                        self.height,
                        settings.blur,
                        settings.background.as_deref(),
                        settings.quality,
                    )?;
                }
            },
        }
        if let Some(pipeline) = self.pipeline.as_mut() {
            pipeline.set_quality(settings.quality);
            pipeline.set_adjustments(
                settings.mirror,
                settings.flip,
                settings.brightness,
                settings.contrast,
            );
        }
        Ok(())
    }

    fn publish(&mut self, output: &mut dyn Sink) -> Result<(), String> {
        let frame = self
            .camera
            .frame()
            .map_err(|why| format!("capture failed: {why}"))?;
        frame
            .decode_image_to_buffer::<RgbFormat>(&mut self.rgb)
            .map_err(|why| format!("decode failed: {why}"))?;
        let present: &[u8] = match self.pipeline.as_mut() {
            Some(pipeline) => pipeline.process(&self.rgb)?,
            None => &self.rgb,
        };
        self.preview_counter += 1;
        let preview = (self.preview_wanted.load(Ordering::Relaxed)
            && self.preview_counter.is_multiple_of(5))
        .then(|| preview_frame(present, self.width, self.height));
        output.present(present, self.width, self.height)?;
        if let Some(frame) = preview
            && let Ok(mut slot) = self.preview.lock()
        {
            *slot = Some(frame);
        }
        self.last_active = Instant::now();
        Ok(())
    }
}

fn preview_frame(rgb: &[u8], width: usize, height: usize) -> PreviewFrame {
    const TARGET_WIDTH: usize = 320;
    let target_width = TARGET_WIDTH.min(width).max(1);
    let target_height = (height * target_width / width.max(1)).max(1);
    let mut out = vec![0u8; target_width * target_height * 3];
    for y in 0..target_height {
        let source_y = y * height / target_height;
        let source_row = source_y * width * 3;
        let target_row = y * target_width * 3;
        for x in 0..target_width {
            let source_x = x * width / target_width;
            let source = source_row + source_x * 3;
            let target = target_row + x * 3;
            out[target..target + 3].copy_from_slice(&rgb[source..source + 3]);
        }
    }
    PreviewFrame {
        width: target_width,
        height: target_height,
        rgb: out,
    }
}

#[cfg(test)]
mod tests {
    use super::{IDLE_RELEASE, fallback_settings, preview_frame, release_due, source_changed};
    use crate::settings::{Mode, Settings};
    use std::time::Duration;

    #[test]
    fn camera_is_released_only_without_a_consumer_and_after_the_grace_period() {
        assert!(!release_due(true, IDLE_RELEASE * 2));
        assert!(!release_due(false, IDLE_RELEASE - Duration::from_millis(1)));
        assert!(release_due(false, IDLE_RELEASE));
    }

    #[test]
    fn changing_the_source_camera_requests_a_reopen() {
        let same = Settings {
            camera_name: Some("Cam A".to_string()),
            ..Settings::default()
        };
        let key = super::camera_key(&same);
        let other = Settings {
            camera_name: Some("Cam B".to_string()),
            ..Settings::default()
        };
        let eco = Settings {
            quality: crate::settings::Quality::Eco,
            ..Settings::default()
        };
        assert!(!source_changed(&key, &same));
        assert!(source_changed(&key, &other));
        assert!(source_changed(&key, &eco));
    }

    #[test]
    fn only_image_mode_falls_back_to_off() {
        let image = Settings {
            mode: Mode::Image,
            ..Settings::default()
        };
        assert_eq!(fallback_settings(&image).map(|settings| settings.mode), Some(Mode::Off));
        assert!(fallback_settings(&Settings::default()).is_none());
        let blur = Settings {
            mode: Mode::Blur,
            ..Settings::default()
        };
        assert!(fallback_settings(&blur).is_none());
    }

    #[test]
    fn preview_downscales_to_a_small_rgb_frame() {
        let source = vec![10u8; 4 * 2 * 3];
        let preview = preview_frame(&source, 4, 2);
        assert_eq!((preview.width, preview.height), (4, 2));
        assert_eq!(preview.rgb.len(), 4 * 2 * 3);
        assert!(preview.rgb.iter().all(|value| *value == 10));
    }
}

