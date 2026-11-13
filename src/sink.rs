use minifb::{Key, Window, WindowOptions};

use crate::capture::{MAX_H, MAX_W};

pub trait Sink {
    fn present(&mut self, rgb: &[u8], width: usize, height: usize) -> Result<(), String>;
    fn alive(&self) -> bool {
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
        let window =
            Window::new("ZagCam", dw, dh, WindowOptions { resize: true, ..Default::default() })
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
        self.camera.send(rgb).map_err(|why| format!("virtual camera send failed: {why}"))
    }
}

fn fit(sw: usize, sh: usize, max_w: usize, max_h: usize) -> (usize, usize) {
    if sw <= max_w && sh <= max_h {
        return (sw, sh);
    }
    let scale = f64::min(max_w as f64 / sw as f64, max_h as f64 / sh as f64);
    (((sw as f64 * scale) as usize).max(1), ((sh as f64 * scale) as usize).max(1))
}

fn scale_rgb_to_0rgb(src: &[u8], sw: usize, xs: &[usize], ys: &[usize], dst: &mut [u32], dw: usize) {
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
