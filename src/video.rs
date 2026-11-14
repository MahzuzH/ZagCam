//! Looping video backgrounds through the Windows Media Foundation source reader.
//!
//! Decoding runs on a worker thread; the pipeline only copies the latest prepared frame.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(windows)]
mod platform {
    use super::*;
    use fast_image_resize::images::{Image, ImageRef};
    use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
    use windows::core::{GUID, HSTRING};
    use windows::Win32::Media::MediaFoundation::{
        IMFMediaBuffer, IMFMediaType, IMFSample, IMFSourceReader, MFCreateAttributes,
        MFCreateMediaType, MFCreateSourceReaderFromURL, MFMediaType_Video, MFStartup, MFShutdown,
        MFVideoFormat_RGB32, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
        MF_MT_SUBTYPE, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM,
        MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MFSTARTUP_LITE,
        MF_VERSION,
    };
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

    const REQUEST_POLL: Duration = Duration::from_millis(5);
    const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(3);

    struct Guards {
        com: bool,
    }

    impl Guards {
        fn init() -> Self {
            let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let _ = unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) };
            Self { com }
        }
    }

    impl Drop for Guards {
        fn drop(&mut self) {
            let _ = unsafe { MFShutdown() };
            if self.com {
                unsafe { CoUninitialize() };
            }
        }
    }

    struct Decoder {
        _guards: Guards,
        reader: IMFSourceReader,
        src_width: usize,
        src_height: usize,
        stride: i32,
        rgb: Vec<u8>,
        scaled: Image<'static>,
        resizer: Resizer,
        opts: ResizeOptions,
    }

    impl Decoder {
        fn open(path: &Path, width: usize, height: usize) -> Result<Self, String> {
            let guards = Guards::init();
            unsafe {
                let mut attributes = None;
                MFCreateAttributes(&mut attributes, 1)
                    .map_err(|why| format!("video attributes: {why}"))?;
                let attributes = attributes.ok_or("video attributes are missing")?;
                attributes
                    .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                    .map_err(|why| format!("video processing: {why}"))?;
                let reader = MFCreateSourceReaderFromURL(
                    &HSTRING::from(path.to_string_lossy().as_ref()),
                    &attributes,
                )
                .map_err(|why| format!("cannot open {}: {why}", path.display()))?;
                let media_type = MFCreateMediaType().map_err(|why| format!("video type: {why}"))?;
                media_type
                    .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                    .map_err(|why| format!("video major type: {why}"))?;
                media_type
                    .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                    .map_err(|why| format!("video RGB type: {why}"))?;
                reader
                    .SetCurrentMediaType(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        None,
                        &media_type,
                    )
                    .map_err(|why| format!("video decoder setup: {why}"))?;
                let (src_width, src_height, stride) = format_of(&reader)?;
                Ok(Self {
                    _guards: guards,
                    reader,
                    src_width,
                    src_height,
                    stride,
                    rgb: vec![0; src_width * src_height * 3],
                    scaled: Image::new(width as u32, height as u32, PixelType::U8x3),
                    resizer: Resizer::new(),
                    opts: ResizeOptions::new()
                        .resize_alg(ResizeAlg::Interpolation(FilterType::Bilinear)),
                })
            }
        }

        fn read_and_scale(&mut self) -> Result<&[u8], String> {
            unsafe {
                for _ in 0..4 {
                    let mut flags = 0u32;
                    let mut sample: Option<IMFSample> = None;
                    self.reader
                        .ReadSample(
                            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                            0,
                            None,
                            Some(&mut flags),
                            None,
                            Some(&mut sample),
                        )
                        .map_err(|why| format!("video read: {why}"))?;
                    if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                        (self.src_width, self.src_height, self.stride) = format_of(&self.reader)?;
                        self.rgb.resize(self.src_width * self.src_height * 3, 0);
                    }
                    if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                        self.reader
                            .SetCurrentPosition(&GUID::zeroed(), &Default::default())
                            .map_err(|why| format!("video loop: {why}"))?;
                        continue;
                    }
                    let Some(sample) = sample else { continue };
                    let buffer = sample
                        .ConvertToContiguousBuffer()
                        .map_err(|why| format!("video buffer: {why}"))?;
                    copy_rgb32(
                        &buffer,
                        self.src_width,
                        self.src_height,
                        self.stride,
                        &mut self.rgb,
                    )?;
                    let source = ImageRef::new(
                        self.src_width as u32,
                        self.src_height as u32,
                        &self.rgb,
                        PixelType::U8x3,
                    )
                    .map_err(|why| format!("video image: {why}"))?;
                    self.resizer
                        .resize(&source, &mut self.scaled, &self.opts)
                        .map_err(|why| format!("video resize: {why}"))?;
                    return Ok(self.scaled.buffer());
                }
            }
            Err("video has no decodable frames".to_string())
        }
    }

    fn format_of(reader: &IMFSourceReader) -> Result<(usize, usize, i32), String> {
        unsafe {
            let media_type: IMFMediaType = reader
                .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
                .map_err(|why| format!("video format: {why}"))?;
            let packed = media_type
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|why| format!("video frame size: {why}"))?;
            let width = (packed >> 32) as usize;
            let height = (packed & 0xffff_ffff) as usize;
            if width == 0
                || height == 0
                || width > 16_384
                || height > 16_384
                || width * height > 100_000_000
            {
                return Err(format!("video frame size {width}x{height} is unsupported"));
            }
            let stride = media_type
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map_or(width as i32 * 4, |stride| stride as i32);
            Ok((width, height, stride))
        }
    }

    fn copy_rgb32(
        buffer: &IMFMediaBuffer,
        width: usize,
        height: usize,
        stride: i32,
        rgb: &mut [u8],
    ) -> Result<(), String> {
        if width == 0 || height == 0 || rgb.len() < width * height * 3 {
            return Err("video frame buffer is too small".to_string());
        }
        let stride = if stride == 0 { width as i32 * 4 } else { stride };
        let row_bytes = stride.unsigned_abs() as usize;
        unsafe {
            let mut raw: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buffer
                .Lock(&mut raw, None, Some(&mut len))
                .map_err(|why| format!("video lock: {why}"))?;
            let result = (|| {
                if raw.is_null() || (len as usize) < row_bytes * height {
                    return Err("video frame buffer is short".to_string());
                }
                let bytes = std::slice::from_raw_parts(raw, len as usize);
                let bottom_up = stride < 0;
                for y in 0..height {
                    let source_y = if bottom_up { height - 1 - y } else { y };
                    let row = &bytes[source_y * row_bytes..];
                    let out = &mut rgb[y * width * 3..(y + 1) * width * 3];
                    for x in 0..width {
                        let px = x * 4;
                        out[x * 3] = row[px + 2];
                        out[x * 3 + 1] = row[px + 1];
                        out[x * 3 + 2] = row[px];
                    }
                }
                Ok(())
            })();
            let _ = buffer.Unlock();
            result
        }
    }

    struct Shared {
        frame: Option<Vec<u8>>,
        error: Option<String>,
        requests: u64,
        decoded: u64,
    }

    pub struct VideoBackground {
        shared: Arc<Mutex<Shared>>,
        stop: Arc<AtomicBool>,
        join: Option<JoinHandle<()>>,
        frame: Vec<u8>,
    }

    impl VideoBackground {
        pub fn open(path: &Path, width: usize, height: usize) -> Result<Self, String> {
            let shared = Arc::new(Mutex::new(Shared {
                frame: None,
                error: None,
                requests: 0,
                decoded: 0,
            }));
            let stop = Arc::new(AtomicBool::new(false));
            let path = path.to_path_buf();
            let worker_shared = Arc::clone(&shared);
            let worker_stop = Arc::clone(&stop);
            let join = std::thread::Builder::new()
                .name("zagcam-video".into())
                .spawn(move || decode_loop(&path, width, height, worker_shared, worker_stop))
                .map_err(|why| format!("cannot start the video decoder: {why}"))?;

            let deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
            loop {
                let ready = {
                    let state = shared.lock().map_err(|_| "video decoder lock poisoned")?;
                    if let Some(error) = &state.error {
                        return Err(error.clone());
                    }
                    state.frame.clone()
                };
                if let Some(frame) = ready {
                    return Ok(Self {
                        shared,
                        stop,
                        join: Some(join),
                        frame,
                    });
                }
                if Instant::now() >= deadline {
                    return Err("video decoder did not produce a first frame".to_string());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        pub fn next_frame(&mut self) -> Result<&[u8], String> {
            let mut state = self.shared.lock().map_err(|_| "video decoder lock poisoned")?;
            if let Some(error) = &state.error {
                return Err(error.clone());
            }
            let Some(frame) = &state.frame else {
                return Err("video has no frame".to_string());
            };
            self.frame.clear();
            self.frame.extend_from_slice(frame);
            if state.requests == state.decoded {
                state.requests += 1;
            }
            drop(state);
            Ok(&self.frame)
        }
    }

    impl Drop for VideoBackground {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    fn decode_loop(
        path: &Path,
        width: usize,
        height: usize,
        shared: Arc<Mutex<Shared>>,
        stop: Arc<AtomicBool>,
    ) {
        let mut decoder = match Decoder::open(path, width, height) {
            Ok(decoder) => decoder,
            Err(why) => {
                if let Ok(mut state) = shared.lock() {
                    state.error = Some(why);
                }
                return;
            }
        };
        match decoder.read_and_scale() {
            Ok(frame) => {
                if let Ok(mut state) = shared.lock() {
                    state.frame = Some(frame.to_vec());
                    state.requests = 1;
                    state.decoded = 1;
                }
            }
            Err(why) => {
                if let Ok(mut state) = shared.lock() {
                    state.error = Some(why);
                }
                return;
            }
        }
        while !stop.load(Ordering::Relaxed) {
            let need = shared
                .lock()
                .map(|state| state.requests > state.decoded)
                .unwrap_or(false);
            if !need {
                std::thread::sleep(REQUEST_POLL);
                continue;
            }
            match decoder.read_and_scale() {
                Ok(frame) => {
                    if let Ok(mut state) = shared.lock() {
                        match &mut state.frame {
                            Some(buffer) if buffer.len() == frame.len() => {
                                buffer.copy_from_slice(frame);
                            }
                            _ => state.frame = Some(frame.to_vec()),
                        }
                        state.decoded += 1;
                    }
                }
                Err(why) => {
                    if let Ok(mut state) = shared.lock() {
                        state.error = Some(why);
                    }
                    return;
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    use fast_image_resize::images::Image;
    use fast_image_resize::PixelType;

    pub struct VideoBackground {
        _scaled: Image<'static>,
    }

    impl VideoBackground {
        pub fn open(_path: &Path, width: usize, height: usize) -> Result<Self, String> {
            Ok(Self {
                _scaled: Image::new(width as u32, height as u32, PixelType::U8x3),
            })
        }

        pub fn next_frame(&mut self) -> Result<&[u8], String> {
            Err("video backgrounds require Windows".to_string())
        }
    }
}

pub use platform::VideoBackground;
