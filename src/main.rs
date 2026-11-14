mod app;
mod capture;
mod engine;
mod nv12;
mod pipeline;
mod segmenter;
mod settings;
mod sink;
mod stats;
mod vcam;

use nokhwa::Camera;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::CameraIndex;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use pipeline::Pipeline;
use settings::{Mode, Quality};
use sink::{Sink, WindowSink};

const USAGE: &str = "ZagCam - lightweight virtual camera

Usage:
  zagcam                     open the tray app (settings window from the tray menu)
  zagcam [options]

Options:
  --camera N        camera number to open (see --list, default 0)
  --list, -l        list available cameras
  --mode off|blur|image
                    effect (default off)
  --blur 0..100     blur intensity (default 50)
  --quality eco|balanced|quality
                    inference cadence and mask resolution (default balanced)
  --background PATH background image for --mode image
  --sink window|vcam|mfvcam
                    output: preview window, OBS virtual camera, or ZagCam camera (default window)
  --model PATH      ONNX segmentation model
  --probe S         run S seconds headless, print fps / RAM / CPU
  --version, -V     print the version
  --help, -h        show this help

Commands:
  vcam <cmd>        manage the virtual camera (start, stop, status, install, uninstall)

Keys: Esc quits the preview window.";

const DEFAULT_MODEL: &str = "models/selfie_segmentation_landscape.onnx";

fn default_model() -> PathBuf {
    let relative = PathBuf::from(DEFAULT_MODEL);
    if relative.exists() {
        return relative;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(DEFAULT_MODEL)))
        .filter(|path| path.exists())
        .unwrap_or(relative)
}

#[derive(Clone, Copy, PartialEq)]
enum SinkKind {
    Window,
    Vcam,
    Mfvcam,
}

struct Args {
    list: bool,
    camera: Option<usize>,
    mode: Mode,
    blur: u8,
    quality: Quality,
    background: Option<PathBuf>,
    sink: SinkKind,
    model: PathBuf,
    probe: Option<f64>,
}

fn main() -> ExitCode {
    let threads = std::env::var("ZAGCAM_THREADS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2);
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("zagcam: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("vcam") {
        argv.remove(0);
        return vcam::run(&argv);
    }
    if argv.is_empty() {
        return app::run(default_model());
    }
    let args = parse_args(argv)?;
    if args.list {
        let cameras = capture::list()?;
        for (i, cam) in cameras.iter().enumerate() {
            let name = cam.human_name();
            let kind = if name.to_ascii_lowercase().contains("virtual camera") {
                " (virtual)"
            } else {
                ""
            };
            println!(
                "{i}: {name} [{}]{kind}",
                cam.index().as_index().unwrap_or(0)
            );
        }
        return Ok(());
    }

    let cameras = capture::list()?;
    if cameras.is_empty() {
        if args.sink == SinkKind::Mfvcam && args.probe.is_none() {
            return run_mfvcam(&args, CameraIndex::Index(args.camera.unwrap_or(0) as u32));
        }
        return Err("no camera found".into());
    }
    let source = match args.camera {
        Some(i) => cameras
            .get(i)
            .ok_or_else(|| format!("camera {i} not found"))?,
        None => &cameras[0],
    };

    if args.sink == SinkKind::Mfvcam && args.probe.is_none() {
        return run_mfvcam(&args, source.index().clone());
    }

    let (capture_w, capture_h) = args.quality.capture_size();
    let mut camera = capture::open(source.index(), capture_w, capture_h)?;
    let format = camera.camera_format();
    let (sw, sh) = (format.width() as usize, format.height() as usize);
    println!(
        "source: {} | {}x{} {}",
        source.human_name(),
        sw,
        sh,
        format.format()
    );

    let mut rgb = vec![0u8; sw * sh * 3];

    let mut pipeline = Pipeline::build(
        args.mode,
        &args.model,
        sw,
        sh,
        args.blur,
        args.background.as_deref(),
        args.quality,
    )?;

    if let Some(secs) = args.probe {
        return probe(&mut camera, &mut rgb, secs, pipeline.as_mut());
    }

    let mut output = make_sink(args.sink, sw, sh)?;
    run_loop(&mut camera, &mut rgb, pipeline.as_mut(), output.as_mut())
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut args = Args {
        list: false,
        camera: None,
        mode: Mode::Off,
        blur: 50,
        quality: Quality::Balanced,
        background: None,
        sink: SinkKind::Window,
        model: default_model(),
        probe: None,
    };
    let mut raw = argv.into_iter();
    while let Some(arg) = raw.next() {
        let mut value = || raw.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--list" | "-l" => args.list = true,
            "--camera" | "-c" => {
                args.camera = Some(
                    value()?
                        .parse()
                        .map_err(|_| "bad camera index".to_string())?,
                );
            }
            "--mode" => {
                let raw = value()?;
                args.mode = Mode::parse(&raw)
                    .ok_or_else(|| format!("unknown mode {raw}"))?;
            }
            "--blur" => {
                args.blur = value()?
                    .parse()
                    .map_err(|_| "bad blur intensity".to_string())?;
            }
            "--quality" => {
                let raw = value()?;
                args.quality = Quality::parse(&raw)
                    .ok_or_else(|| format!("unknown quality {raw}"))?;
            }
            "--background" => args.background = Some(PathBuf::from(value()?)),
            "--sink" => {
                args.sink = match value()?.as_str() {
                    "window" => SinkKind::Window,
                    "vcam" => SinkKind::Vcam,
                    "mfvcam" => SinkKind::Mfvcam,
                    other => return Err(format!("unknown sink {other}")),
                };
            }
            "--model" => args.model = PathBuf::from(value()?),
            "--probe" => {
                args.probe = Some(
                    value()?
                        .parse()
                        .map_err(|_| "bad probe duration".to_string())?,
                );
            }
            "--version" | "-V" => {
                println!("ZagCam {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }
    Ok(args)
}

fn make_sink(kind: SinkKind, w: usize, h: usize) -> Result<Box<dyn Sink>, String> {
    match kind {
        SinkKind::Window => Ok(Box::new(WindowSink::new(w, h)?)),
        SinkKind::Vcam => {
            #[cfg(feature = "sink-vcam")]
            {
                Ok(Box::new(sink::VcamSink::new(w as u32, h as u32, 30.0)?))
            }
            #[cfg(not(feature = "sink-vcam"))]
            {
                let _ = (w, h);
                Err("this build has no OBS virtual-camera output; rebuild with --features sink-vcam".into())
            }
        }
        SinkKind::Mfvcam => {
            let _ = (w, h);
            Ok(Box::new(sink::MfvcamSink::new()))
        }
    }
}

const IDLE_RELEASE: Duration = Duration::from_secs(3);
const CAPTURE_RETRY: Duration = Duration::from_secs(1);

struct CaptureSession {
    camera: Camera,
    rgb: Vec<u8>,
    pipeline: Option<Pipeline>,
    width: usize,
    height: usize,
    last_active: Instant,
}

impl CaptureSession {
    fn open(index: &CameraIndex, args: &Args) -> Result<Self, String> {
        let (max_w, max_h) = args.quality.capture_size();
        let camera = capture::open(index, max_w, max_h)?;
        let format = camera.camera_format();
        let (width, height) = (format.width() as usize, format.height() as usize);
        let pipeline = Pipeline::build(
            args.mode,
            &args.model,
            width,
            height,
            args.blur,
            args.background.as_deref(),
            args.quality,
        )?;
        println!("camera opened: {width}x{height}");
        Ok(Self {
            camera,
            rgb: vec![0u8; width * height * 3],
            pipeline,
            width,
            height,
            last_active: Instant::now(),
        })
    }

    fn publish(&mut self, output: &mut dyn Sink) -> Result<(), String> {
        next_frame(&mut self.camera, &mut self.rgb)?;
        let present: &[u8] = match self.pipeline.as_mut() {
            Some(pipeline) => pipeline.process(&self.rgb)?,
            None => &self.rgb,
        };
        output.present(present, self.width, self.height)?;
        self.last_active = Instant::now();
        Ok(())
    }
}

fn run_mfvcam(args: &Args, index: CameraIndex) -> Result<(), String> {
    let mut output = sink::MfvcamSink::new();
    let mut session: Option<CaptureSession> = None;
    let mut last_error: Option<Instant> = None;
    println!("waiting for an app to read the ZagCam camera (run `zagcam vcam start` first)");

    loop {
        if output.wants_frame() {
            if session.is_none() {
                match CaptureSession::open(&index, args) {
                    Ok(opened) => {
                        session = Some(opened);
                        last_error = None;
                    }
                    Err(why) => {
                        if last_error.is_none_or(|at| at.elapsed() >= CAPTURE_RETRY) {
                            eprintln!("capture unavailable: {why}; retrying");
                            last_error = Some(Instant::now());
                        }
                    }
                }
            }
            if let Some(current) = session.as_mut() {
                if let Err(why) = current.publish(&mut output) {
                    eprintln!("capture stopped: {why}; reopening");
                    session = None;
                    std::thread::sleep(Duration::from_millis(250));
                }
            } else {
                std::thread::sleep(Duration::from_millis(250));
            }
        } else {
            if session
                .as_ref()
                .is_some_and(|current| current.last_active.elapsed() >= IDLE_RELEASE)
            {
                println!("no client reading; releasing camera");
                session = None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn run_loop(
    camera: &mut Camera,
    rgb: &mut [u8],
    mut pipeline: Option<&mut Pipeline>,
    output: &mut dyn Sink,
) -> Result<(), String> {
    let mut sampler = stats::Sampler::new();
    let start = Instant::now();
    let mut last_stats = Instant::now();
    let mut frames: u64 = 0;
    let size = camera.camera_format();
    let (w, h) = (size.width() as usize, size.height() as usize);

    while output.alive() {
        next_frame(camera, rgb)?;
        let present: &[u8] = match pipeline.as_deref_mut() {
            Some(pipeline) => pipeline.process(rgb)?,
            None => rgb,
        };
        output.present(present, w, h)?;
        frames += 1;

        if last_stats.elapsed() >= Duration::from_secs(1) {
            let snap = sampler.snapshot();
            output.set_title(&snap.title(frames, start.elapsed()));
            last_stats = Instant::now();
        }
    }

    let snap = sampler.snapshot();
    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "M1 session: {:.1} fps over {:.1}s | working set {:.1} MiB | CPU {:.1}% of one core",
        frames as f64 / elapsed,
        elapsed,
        snap.working_set as f64 / 1_048_576.0,
        snap.cpu_percent
    );
    Ok(())
}

fn probe(
    camera: &mut Camera,
    rgb: &mut [u8],
    secs: f64,
    mut pipeline: Option<&mut Pipeline>,
) -> Result<(), String> {
    let mut sampler = stats::Sampler::new();
    let start = Instant::now();
    let mut frames: u64 = 0;
    let mut first_frame = None;
    let mut latencies = Vec::new();
    while start.elapsed().as_secs_f64() < secs {
        let decode = next_frame(camera, rgb)?;
        let effect_start = Instant::now();
        if let Some(pipeline) = pipeline.as_deref_mut() {
            pipeline.process(rgb)?;
        }
        latencies.push(decode + effect_start.elapsed());
        frames += 1;
        if first_frame.is_none() {
            first_frame = Some(start.elapsed());
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    let snap = sampler.snapshot();
    println!(
        "M1 baseline: {:.1} fps over {:.1}s | working set {:.1} MiB | CPU {:.1}% of one core",
        frames as f64 / elapsed,
        elapsed,
        snap.working_set as f64 / 1_048_576.0,
        snap.cpu_percent
    );
    latencies.sort_unstable();
    println!(
        "  first frame {:.0} ms | average interval {:.2} ms | capture-to-output p50 {:.1} ms | p95 {:.1} ms",
        first_frame.unwrap_or_default().as_secs_f64() * 1000.0,
        elapsed * 1000.0 / frames.max(1) as f64,
        percentile(&latencies, 50),
        percentile(&latencies, 95)
    );
    if let Some(pipeline) = pipeline.as_deref()
        && pipeline.samples > 0
    {
        let n = pipeline.samples as f64;
        let t = pipeline.timings;
        println!(
            "  per frame: preprocess {:.2} ms | infer {:.2} ms | mask {:.2} ms | blur {:.2} ms | composite {:.2} ms",
            t.preprocess.as_secs_f64() * 1000.0 / n,
            t.infer.as_secs_f64() * 1000.0 / n,
            t.mask.as_secs_f64() * 1000.0 / n,
            t.blur.as_secs_f64() * 1000.0 / n,
            t.composite.as_secs_f64() * 1000.0 / n
        );
    }
    Ok(())
}

fn next_frame(camera: &mut Camera, rgb: &mut [u8]) -> Result<Duration, String> {
    let frame = camera
        .frame()
        .map_err(|why| format!("capture failed: {why}"))?;
    let decode_start = Instant::now();
    frame
        .decode_image_to_buffer::<RgbFormat>(rgb)
        .map_err(|why| format!("decode failed: {why}"))?;
    Ok(decode_start.elapsed())
}

fn percentile(sorted: &[Duration], percent: usize) -> f64 {
    let index = (sorted.len() * percent / 100).min(sorted.len().saturating_sub(1));
    sorted
        .get(index)
        .map_or(0.0, |value| value.as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::{Quality, parse_args, percentile};
    use std::time::Duration;

    #[test]
    fn quality_flag_parses() {
        let args = parse_args(vec!["--quality".to_string(), "eco".to_string()]).expect("parse");
        assert_eq!(args.quality, Quality::Eco);
    }

    #[test]
    fn unknown_quality_is_rejected() {
        assert!(parse_args(vec!["--quality".to_string(), "ultra".to_string()]).is_err());
    }

    #[test]
    fn latency_percentiles_handle_empty_and_tail() {
        assert_eq!(percentile(&[], 95), 0.0);
        let samples: Vec<Duration> = (1..=4).map(Duration::from_millis).collect();
        assert_eq!(percentile(&samples, 50), 3.0);
        assert_eq!(percentile(&samples, 95), 4.0);
    }
}
