use std::time::{Duration, Instant};

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use minifb::{Key, Window, WindowOptions};
use zagcam_vcam_mgr::{FrameChannel, nv12_len};

use crate::capture::{MAX_H, MAX_W};
use crate::nv12::rgb_to_nv12;

const CHANNEL_RETRY: Duration = Duration::from_secs(1);

pub trait Sink {
    fn present(&mut self, rgb: &[u8], width: usize, height: usize) -> Result<(), String>;
    fn alive(&self) -> bool {
        true
    }
    fn wants_frame(&mut self) -> bool {
        true
    }
    fn set_title(&mut self, _title: &str) {}
}

pub struct WindowSink {
    window: Window,
    pixels: Vec<u32>,
    dw: usize,
    dh: usize,
    xs: Vec<usize>,
    ys: Vec<usize>,
}

impl WindowSink {
    pub fn new(sw: usize, sh: usize) -> Result<Self, String> {
        let (dw, dh) = fit(sw, sh, MAX_W, MAX_H);
        let window = Window::new(
            "ZagCam",
            dw,
            dh,
            WindowOptions {
                resize: true,
                ..Default::default()
            },
        )
        .map_err(|why| format!("window failed: {why}"))?;
        Ok(Self {
            window,
            pixels: vec![0u32; dw * dh],
            dw,
            dh,
            xs: (0..dw).map(|x| x * sw / dw).collect(),
            ys: (0..dh).map(|y| y * sh / dh).collect(),
        })
    }
}

impl Sink for WindowSink {
    fn present(&mut self, rgb: &[u8], width: usize, _height: usize) -> Result<(), String> {
        scale_rgb_to_0rgb(rgb, width, &self.xs, &self.ys, &mut self.pixels, self.dw);
        self.window
            .update_with_buffer(&self.pixels, self.dw, self.dh)
            .map_err(|why| format!("present failed: {why}"))
    }

    fn alive(&self) -> bool {
        self.window.is_open() && !self.window.is_key_down(Key::Escape)
    }

    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }
}

/// Publishes engine frames into the ZagCam virtual camera's shared channel.
pub struct MfvcamSink {
    channel: Option<FrameChannel>,
    channel_attempt: Option<Instant>,
    request: Option<(u32, u32)>,
    resizer: Resizer,
    opts: ResizeOptions,
    scaled: Option<Image<'static>>,
    nv12: Vec<u8>,
    size: (u32, u32),
    last_report: Option<(u32, u32)>,
}

impl MfvcamSink {
    pub fn new() -> Self {
        Self {
            channel: None,
            channel_attempt: None,
            request: None,
            resizer: Resizer::new(),
            opts: ResizeOptions::new().resize_alg(ResizeAlg::Interpolation(FilterType::Bilinear)),
            scaled: None,
            nv12: Vec::new(),
            size: (0, 0),
            last_report: None,
        }
    }

    fn poll_request(&mut self) -> Option<(u32, u32)> {
        if self.channel.is_none() {
            let now = Instant::now();
            if self
                .channel_attempt
                .is_none_or(|last| now.duration_since(last) >= CHANNEL_RETRY)
            {
                self.channel_attempt = Some(now);
                self.channel = FrameChannel::open().ok();
            }
        }
        let request = self.channel.as_ref().and_then(FrameChannel::active_request);
        if request.is_some() && request != self.last_report {
            if let Some((w, h)) = request {
                println!("virtual camera client reading {w}x{h}");
            }
            self.last_report = request;
        } else if request.is_none() && self.last_report.is_some() {
            println!("virtual camera client stopped");
            self.last_report = None;
            self.scaled = None;
            self.nv12 = Vec::new();
            self.size = (0, 0);
        }
        request
    }

    fn ensure_target(&mut self, width: u32, height: u32) {
        if self.size == (width, height) {
            return;
        }
        self.scaled = None;
        self.nv12.resize(nv12_len(width, height), 0);
        self.size = (width, height);
    }
}

impl Sink for MfvcamSink {
    fn present(&mut self, rgb: &[u8], width: usize, height: usize) -> Result<(), String> {
        let target = match self.request {
            Some(size) => Some(size),
            None => self.poll_request(),
        };
        let Some((tw, th)) = target else {
            return Ok(());
        };
        self.ensure_target(tw, th);

        let source = if (width as u32, height as u32) == (tw, th) {
            rgb
        } else {
            let src = ImageRef::new(width as u32, height as u32, rgb, PixelType::U8x3)
                .map_err(|why| format!("resize failed: {why}"))?;
            let scaled = self
                .scaled
                .get_or_insert_with(|| Image::new(tw, th, PixelType::U8x3));
            self.resizer
                .resize(&src, scaled, &self.opts)
                .map_err(|why| format!("resize failed: {why}"))?;
            scaled.buffer()
        };

        rgb_to_nv12(source, tw, th, &mut self.nv12)?;
        let Some(channel) = self.channel.as_ref() else {
            return Ok(());
        };
        channel.write_frame(tw, th, |out| out.copy_from_slice(&self.nv12));
        Ok(())
    }

    fn wants_frame(&mut self) -> bool {
        self.request = self.poll_request();
        self.request.is_some()
    }
}

#[cfg(feature = "sink-vcam")]
pub struct VcamSink {
    camera: virtualcam::Camera,
}

#[cfg(feature = "sink-vcam")]
impl VcamSink {
    pub fn new(width: u32, height: u32, fps: f64) -> Result<Self, String> {
        let camera = virtualcam::Camera::builder(width, height, fps)
            .format(virtualcam::PixelFormat::RGB)
            .build()
            .map_err(|why| {
                format!("virtual camera unavailable (is the OBS Virtual Camera installed?): {why}")
            })?;
        Ok(Self { camera })
    }
}

#[cfg(feature = "sink-vcam")]
impl Sink for VcamSink {
    fn present(&mut self, rgb: &[u8], _width: usize, _height: usize) -> Result<(), String> {
        self.camera
            .send(rgb)
            .map_err(|why| format!("virtual camera send failed: {why}"))
    }
}

fn fit(sw: usize, sh: usize, max_w: usize, max_h: usize) -> (usize, usize) {
    if sw <= max_w && sh <= max_h {
        return (sw, sh);
    }
    let scale = f64::min(max_w as f64 / sw as f64, max_h as f64 / sh as f64);
    (
        ((sw as f64 * scale) as usize).max(1),
        ((sh as f64 * scale) as usize).max(1),
    )
}

fn scale_rgb_to_0rgb(
    src: &[u8],
    sw: usize,
    xs: &[usize],
    ys: &[usize],
    dst: &mut [u32],
    dw: usize,
) {
    for (y, dst_row) in dst.chunks_mut(dw).enumerate() {
        let src_row = ys[y] * sw;
        for (x, pixel) in dst_row.iter_mut().enumerate() {
            let i = (src_row + xs[x]) * 3;
            *pixel =
                (u32::from(src[i]) << 16) | (u32::from(src[i + 1]) << 8) | u32::from(src[i + 2]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fit, scale_rgb_to_0rgb};

    #[test]
    fn fit_keeps_small_frames_and_caps_large_ones() {
        assert_eq!(fit(640, 480, 1280, 720), (640, 480));
        assert_eq!(fit(1920, 1080, 1280, 720), (1280, 720));
        assert_eq!(fit(1080, 1920, 1280, 720), (405, 720));
    }

    #[test]
    fn scale_maps_top_left_pixel() {
        let src = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
        let mut dst = [0u32; 1];
        scale_rgb_to_0rgb(&src, 2, &[0], &[0], &mut dst, 1);
        assert_eq!(dst[0], (10 << 16) | (20 << 8) | 30);
    }
}
