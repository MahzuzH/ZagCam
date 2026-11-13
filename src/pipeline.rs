use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use std::time::{Duration, Instant};

use crate::segmenter::{Segmenter, IN_H, IN_W};

const EMA_ALPHA: u32 = 102; // 0.4 in 0..255

#[derive(Clone, Copy, Default)]
pub struct Timings {
    pub preprocess: Duration,
    pub infer: Duration,
    pub mask: Duration,
    pub blur: Duration,
    pub composite: Duration,
}

pub struct Pipeline {
    width: usize,
    height: usize,
    segmenter: Segmenter,
    resizer: Resizer,
    opts_down: ResizeOptions,
    opts_up: ResizeOptions,
    small_rgb: Image<'static>,
    input: Vec<f32>,
    mask_small: Vec<u8>,
    mask_f32: Vec<f32>,
    mask_half: Image<'static>,
    ema: Vec<u8>,
    blur_small: Image<'static>,
    blur_full: Image<'static>,
    out: Vec<u8>,
    frame: u64,
    mask_w: usize,
    pub timings: Timings,
    pub samples: u64,
}

impl Pipeline {
    pub fn new(
        segmenter: Segmenter,
        width: usize,
        height: usize,
        blur: u8,
    ) -> Result<Self, String> {
        let div = blur_div(blur);
        let (bw, bh) = ((width / div).max(1) as u32, (height / div).max(1) as u32);
        let pixels = width * height;
        let (mask_w, mask_h) = ((width / 2).max(1), (height / 2).max(1));
        Ok(Self {
            width,
            height,
            segmenter,
            resizer: Resizer::new(),
            opts_down: ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Box)),
            opts_up: ResizeOptions::new().resize_alg(ResizeAlg::Interpolation(FilterType::Bilinear)),
            small_rgb: Image::new(IN_W as u32, IN_H as u32, PixelType::U8x3),
            input: vec![0.0; IN_W * IN_H * 3],
            mask_small: vec![0u8; IN_W * IN_H],
            mask_f32: vec![0.0; IN_W * IN_H],
            mask_half: Image::new(mask_w as u32, mask_h as u32, PixelType::U8),
            ema: vec![255u8; mask_w * mask_h],
            blur_small: Image::new(bw, bh, PixelType::U8x3),
            blur_full: Image::new(width as u32, height as u32, PixelType::U8x3),
            out: vec![0u8; pixels * 3],
            frame: 0,
            mask_w,
            timings: Timings::default(),
            samples: 0,
        })
    }

    /// Blurs the background of `rgb` in place into the internal output buffer,
    /// which is returned. `rgb` is 24-bit RGB at the pipeline resolution.
    pub fn process(&mut self, rgb: &[u8]) -> Result<&[u8], String> {
        let (w, h) = (self.width as u32, self.height as u32);
        let mut t = Instant::now();

        let src = ImageRef::new(w, h, rgb, PixelType::U8x3).map_err(resize_err)?;
        self.resizer
            .resize(&src, &mut self.small_rgb, &self.opts_down)
            .map_err(resize_err)?;
        let plane = IN_W * IN_H;
        for (i, pixel) in self.small_rgb.buffer().as_chunks::<3>().0.iter().enumerate() {
            self.input[i] = f32::from(pixel[0]) / 255.0;
            self.input[plane + i] = f32::from(pixel[1]) / 255.0;
            self.input[2 * plane + i] = f32::from(pixel[2]) / 255.0;
        }
        self.timings.preprocess += t.elapsed();

        if self.frame.is_multiple_of(2) {
            t = Instant::now();
            self.segmenter.infer(&self.input, &mut self.mask_f32)?;
            self.timings.infer += t.elapsed();
            for (dst, value) in self.mask_small.iter_mut().zip(&self.mask_f32) {
                *dst = (value.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
        self.frame += 1;

        t = Instant::now();
        let mask_src =
            ImageRef::new(IN_W as u32, IN_H as u32, &self.mask_small, PixelType::U8)
                .map_err(resize_err)?;
        self.resizer
            .resize(&mask_src, &mut self.mask_half, &self.opts_up)
            .map_err(resize_err)?;
        ema_u8(&mut self.ema, self.mask_half.buffer(), EMA_ALPHA);
        self.timings.mask += t.elapsed();

        t = Instant::now();
        let src = ImageRef::new(w, h, rgb, PixelType::U8x3).map_err(resize_err)?;
        self.resizer
            .resize(&src, &mut self.blur_small, &self.opts_down)
            .map_err(resize_err)?;
        self.resizer
            .resize(&self.blur_small, &mut self.blur_full, &self.opts_up)
            .map_err(resize_err)?;
        self.timings.blur += t.elapsed();

        t = Instant::now();
        blend_rgb(rgb, self.blur_full.buffer(), &self.ema, self.mask_w, &mut self.out);
        self.timings.composite += t.elapsed();
        self.samples += 1;
        Ok(&self.out)
    }
}

fn resize_err(why: impl std::fmt::Display) -> String {
    format!("resize failed: {why}")
}

/// Larger intensity means a smaller intermediate image, so a stronger blur.
pub fn blur_div(intensity: u8) -> usize {
    2 + usize::from(intensity.min(100)) / 12
}

/// Exponential moving average of the mask, alpha scaled to 0..=255.
pub fn ema_u8(prev: &mut [u8], cur: &[u8], alpha: u32) {
    let keep = 255 - alpha;
    for (prev, cur) in prev.iter_mut().zip(cur) {
        let v = u32::from(*prev) * keep + u32::from(*cur) * alpha;
        *prev = ((v + 1 + (v >> 8)) >> 8) as u8;
    }
}

/// `out = fg*m + bg*(1-m)`. `mask` is a half-resolution mask in 0..=255,
/// `mask_w` its width; each mask pixel covers a 2x2 block of the frame.
pub fn blend_rgb(fg: &[u8], bg: &[u8], mask: &[u8], mask_w: usize, out: &mut [u8]) {
    use rayon::prelude::*;

    let width = mask_w * 2;
    let row = width * 3;
    out.par_chunks_mut(row).enumerate().for_each(|(y, out_row)| {
        let mask_row = &mask[(y / 2) * mask_w..(y / 2 + 1) * mask_w];
        let fg_row = &fg[y * row..(y + 1) * row];
        let bg_row = &bg[y * row..(y + 1) * row];
        for x in 0..width {
            let m = u32::from(mask_row[x / 2]);
            let keep = 255 - m;
            let idx = x * 3;
            for c in 0..3 {
                let v = u32::from(fg_row[idx + c]) * m + u32::from(bg_row[idx + c]) * keep;
                out_row[idx + c] = ((v + 1 + (v >> 8)) >> 8) as u8;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{blend_rgb, blur_div, ema_u8};

    #[test]
    fn blur_div_grows_with_intensity() {
        assert_eq!(blur_div(0), 2);
        assert!(blur_div(100) > blur_div(50));
        assert_eq!(blur_div(250), blur_div(100));
    }

    #[test]
    fn ema_takes_the_first_sample_when_started_at_255() {
        let mut prev = [255u8; 4];
        ema_u8(&mut prev, &[0, 0, 0, 0], 255);
        assert_eq!(prev, [0, 0, 0, 0]);
    }

    #[test]
    fn blend_picks_foreground_or_background_at_the_extremes() {
        let fg = [10, 20, 30, 40, 50, 60];
        let bg = [200, 210, 220, 230, 240, 250];
        let mut out = [0u8; 6];

        blend_rgb(&fg, &bg, &[255], 1, &mut out);
        assert_eq!(out, fg);

        blend_rgb(&fg, &bg, &[0], 1, &mut out);
        assert_eq!(out, bg);
    }
}
