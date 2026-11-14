use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::segmenter::{Segmenter, IN_H, IN_W};
use crate::settings::{Mode, Quality, validate_background};

const EMA_ALPHA: u32 = 102; // 0.4 in 0..255

/// What the background is replaced with. Both paths use the same mask and `blend_rgb`.
pub enum BackgroundSource<'a> {
    Blur(u8),
    Image(&'a Path),
}

enum Background {
    Blur,
    Image(Image<'static>),
}

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
    background: Background,
    out: Vec<u8>,
    frame: u64,
    mask_w: usize,
    mask_div: usize,
    quality: Quality,
    pub timings: Timings,
    pub samples: u64,
}

impl Pipeline {
    pub fn new(
        segmenter: Segmenter,
        width: usize,
        height: usize,
        blur: u8,
        quality: Quality,
    ) -> Result<Self, String> {
        let div = blur_div(blur);
        let (bw, bh) = ((width / div).max(1) as u32, (height / div).max(1) as u32);
        let pixels = width * height;
        let mask_div = quality.mask_div();
        let (mask_w, mask_h) = (
            width.div_ceil(mask_div).max(1),
            height.div_ceil(mask_div).max(1),
        );
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
            background: Background::Blur,
            out: vec![0u8; pixels * 3],
            frame: 0,
            mask_w,
            mask_div,
            quality,
            timings: Timings::default(),
            samples: 0,
        })
    }

    /// Builds the pipeline for a mode. `None` for `Mode::Off`.
    pub fn build(
        mode: Mode,
        model: &Path,
        width: usize,
        height: usize,
        blur: u8,
        background: Option<&Path>,
        quality: Quality,
    ) -> Result<Option<Self>, String> {
        match mode {
            Mode::Off => Ok(None),
            Mode::Blur => Ok(Some(Self::new(
                Segmenter::load(model)?,
                width,
                height,
                blur,
                quality,
            )?)),
            Mode::Image => {
                let path = background.ok_or("image mode needs a background image (--background PATH)")?;
                let mut pipeline =
                    Self::new(Segmenter::load(model)?, width, height, blur, quality)?;
                pipeline.configure(BackgroundSource::Image(path))?;
                Ok(Some(pipeline))
            }
        }
    }

    /// Switches the background without touching the mask. On error the previous background stays.
    pub fn configure(&mut self, source: BackgroundSource<'_>) -> Result<(), String> {
        match source {
            BackgroundSource::Blur(intensity) => {
                self.set_blur(intensity);
                self.background = Background::Blur;
                Ok(())
            }
            BackgroundSource::Image(path) => {
                let image = load_background(path, self.width, self.height)?;
                self.background = Background::Image(image);
                Ok(())
            }
        }
    }

    /// Changes inference cadence and mask resolution. The next frame uses the new quality.
    pub fn set_quality(&mut self, quality: Quality) {
        if quality == self.quality {
            return;
        }
        self.quality = quality;
        let div = quality.mask_div();
        if div != self.mask_div {
            self.mask_div = div;
            self.mask_w = self.width.div_ceil(div).max(1);
            let mask_h = self.height.div_ceil(div).max(1);
            self.mask_half = Image::new(self.mask_w as u32, mask_h as u32, PixelType::U8);
            self.ema = vec![255u8; self.mask_w * mask_h];
        }
    }

    fn set_blur(&mut self, intensity: u8) {
        let div = blur_div(intensity);
        let (width, height) = ((self.width / div).max(1) as u32, (self.height / div).max(1) as u32);
        if (self.blur_small.width(), self.blur_small.height()) != (width, height) {
            self.blur_small = Image::new(width, height, PixelType::U8x3);
        }
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

        if self.frame.is_multiple_of(self.quality.infer_every()) {
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

        if matches!(self.background, Background::Blur) {
            t = Instant::now();
            let src = if self.quality == Quality::High {
                ImageRef::new(w, h, rgb, PixelType::U8x3).map_err(resize_err)?
            } else {
                ImageRef::new(
                    IN_W as u32,
                    IN_H as u32,
                    self.small_rgb.buffer(),
                    PixelType::U8x3,
                )
                .map_err(resize_err)?
            };
            self.resizer
                .resize(&src, &mut self.blur_small, &self.opts_down)
                .map_err(resize_err)?;
            self.resizer
                .resize(&self.blur_small, &mut self.blur_full, &self.opts_up)
                .map_err(resize_err)?;
            self.timings.blur += t.elapsed();
        }

        t = Instant::now();
        let background: &[u8] = match &self.background {
            Background::Blur => self.blur_full.buffer(),
            Background::Image(image) => image.buffer(),
        };
        blend_rgb(
            rgb,
            background,
            &self.ema,
            self.mask_w,
            self.mask_div,
            self.width,
            &mut self.out,
        );
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

/// Decodes PNG/JPEG once and stretches it to the capture size.
fn load_background(path: &Path, width: usize, height: usize) -> Result<Image<'static>, String> {
    validate_background(path)?;
    let decoded = image::ImageReader::open(path)
        .map_err(|why| format!("cannot open {}: {why}", path.display()))?
        .with_guessed_format()
        .map_err(|why| format!("cannot read {}: {why}", path.display()))?
        .decode()
        .map_err(|why| format!("cannot decode {}: {why}", path.display()))?;
    let rgb = decoded.to_rgb8();
    let source = ImageRef::new(rgb.width(), rgb.height(), rgb.as_raw(), PixelType::U8x3)
        .map_err(resize_err)?;
    let mut resized = Image::new(width as u32, height as u32, PixelType::U8x3);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Interpolation(FilterType::Bilinear));
    Resizer::new()
        .resize(&source, &mut resized, &options)
        .map_err(resize_err)?;
    Ok(resized)
}

/// Exponential moving average of the mask, alpha scaled to 0..=255.
pub fn ema_u8(prev: &mut [u8], cur: &[u8], alpha: u32) {
    let keep = 255 - alpha;
    for (prev, cur) in prev.iter_mut().zip(cur) {
        let v = u32::from(*prev) * keep + u32::from(*cur) * alpha;
        *prev = ((v + 1 + (v >> 8)) >> 8) as u8;
    }
}

/// `out = fg*m + bg*(1-m)`. `mask` is in 0..=255 with width `mask_w`; each mask pixel
/// covers a `mask_div` x `mask_div` block of the `width`-pixel-wide frame.
pub fn blend_rgb(
    fg: &[u8],
    bg: &[u8],
    mask: &[u8],
    mask_w: usize,
    mask_div: usize,
    width: usize,
    out: &mut [u8],
) {
    use rayon::prelude::*;

    let row = width * 3;
    out.par_chunks_mut(row).enumerate().for_each(|(y, out_row)| {
        let mask_row = &mask[(y / mask_div) * mask_w..(y / mask_div + 1) * mask_w];
        let fg_row = &fg[y * row..(y + 1) * row];
        let bg_row = &bg[y * row..(y + 1) * row];
        for (block, &mask_value) in mask_row.iter().enumerate() {
            let m = u32::from(mask_value);
            let keep = 255 - m;
            let start = block * mask_div;
            let end = (start + mask_div).min(width);
            for x in start..end {
                let idx = x * 3;
                for c in 0..3 {
                    let v = u32::from(fg_row[idx + c]) * m + u32::from(bg_row[idx + c]) * keep;
                    out_row[idx + c] = ((v + 1 + (v >> 8)) >> 8) as u8;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{blend_rgb, blur_div, ema_u8, load_background};
    use fast_image_resize::PixelType;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_png(width: u32, height: u32) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "zagcam-test-{}-{n}.png",
            std::process::id()
        ));
        let image = image::RgbImage::from_pixel(width, height, image::Rgb([10, 20, 30]));
        image
            .save_with_format(&path, image::ImageFormat::Png)
            .expect("write test png");
        path
    }

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

        blend_rgb(&fg, &bg, &[255], 1, 2, 2, &mut out);
        assert_eq!(out, fg);

        blend_rgb(&fg, &bg, &[0], 1, 2, 2, &mut out);
        assert_eq!(out, bg);
    }

    #[test]
    fn blend_handles_quarter_resolution_masks() {
        let fg = [10u8; 6 * 4 * 3];
        let bg = [200u8; 6 * 4 * 3];
        let mut out = [0u8; 6 * 4 * 3];

        blend_rgb(&fg, &bg, &[255, 0], 2, 4, 6, &mut out);
        for row in out.chunks(18) {
            assert!(row[..12].iter().all(|&value| value == 10));
            assert!(row[12..].iter().all(|&value| value == 200));
        }
    }

    #[test]
    fn background_is_resized_to_the_capture_size() {
        let path = temp_png(4, 2);
        let resized = load_background(&path, 8, 4).expect("load background");
        let _ = std::fs::remove_file(&path);

        assert_eq!(resized.pixel_type(), PixelType::U8x3);
        assert_eq!(resized.buffer().len(), 8 * 4 * 3);
        assert!(resized.buffer().as_chunks::<3>().0.iter().all(|p| p == &[10, 20, 30]));
    }

    #[test]
    fn missing_background_is_an_error() {
        let path = std::env::temp_dir().join("zagcam-definitely-missing.png");
        let _ = std::fs::remove_file(&path);
        assert!(load_background(&path, 8, 4).is_err());
    }

    #[test]
    fn corrupt_background_is_an_error() {
        let path = std::env::temp_dir().join(format!("zagcam-corrupt-{}.png", std::process::id()));
        std::fs::write(&path, b"not an image").expect("write corrupt file");
        assert!(load_background(&path, 8, 4).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
