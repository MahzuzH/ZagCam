//! RGB to NV12 conversion for the virtual camera channel.
//!
//! The provider advertises BT.709 limited range (`MF_MT_YUV_MATRIX`,
//! `MF_MT_VIDEO_NOMINAL_RANGE` in `crates/zagcam-vcam/src/source.rs`), so the engine
//! converts with the matching coefficients.

use zagcam_vcam_mgr::nv12_len;

/// Converts packed RGB to NV12 (BT.709 limited range, 2x2 chroma).
pub fn rgb_to_nv12(rgb: &[u8], width: u32, height: u32, out: &mut [u8]) -> Result<(), String> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return Err(format!(
            "NV12 needs even non-zero dimensions, got {width}x{height}"
        ));
    }
    let pixels = width as usize * height as usize;
    if rgb.len() < pixels * 3 {
        return Err(format!("RGB buffer too small for {width}x{height}"));
    }
    let len = nv12_len(width, height);
    if out.len() < len {
        return Err(format!("NV12 buffer too small for {width}x{height}"));
    }

    for (i, pixel) in rgb[..pixels * 3].as_chunks::<3>().0.iter().enumerate() {
        out[i] = luma(pixel[0], pixel[1], pixel[2]);
    }

    let luma_plane = pixels;
    let chroma_width = width as usize / 2;
    for cy in 0..height as usize / 2 {
        for cx in 0..chroma_width {
            let mut u = 0i32;
            let mut v = 0i32;
            for dy in 0..2 {
                let row = (cy * 2 + dy) * width as usize;
                for dx in 0..2 {
                    let i = (row + cx * 2 + dx) * 3;
                    let (r, g, b) = (rgb[i] as i32, rgb[i + 1] as i32, rgb[i + 2] as i32);
                    u += -37_797 * r - 74_203 * g + 112_000 * b;
                    v += 112_000 * r - 93_786 * g - 18_214 * b;
                }
            }
            let i = luma_plane + cy * width as usize + cx * 2;
            out[i] = clamp(128 + u / (4 * 255_000));
            out[i + 1] = clamp(128 + v / (4 * 255_000));
        }
    }
    Ok(())
}

fn luma(r: u8, g: u8, b: u8) -> u8 {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    clamp(16 + (65_481 * r + 128_553 * g + 24_966 * b) / 255_000)
}

fn clamp(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::rgb_to_nv12;
    use zagcam_vcam_mgr::nv12_len;

    fn convert(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut out = vec![0u8; nv12_len(width, height)];
        rgb_to_nv12(pixels, width, height, &mut out).expect("convert");
        out
    }

    #[test]
    fn black_and_white_use_limited_range_luma() {
        assert_eq!(convert(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 2, 2)[0], 16);
        assert_eq!(convert(&[255; 12], 2, 2)[0], 235);
    }

    #[test]
    fn greys_have_neutral_chroma() {
        let out = convert(&[128; 12], 2, 2);
        let luma = 4;
        assert!(
            out[luma..].iter().all(|&value| value == 128),
            "chroma {out:?}"
        );
    }

    #[test]
    fn conversion_keeps_the_expected_size_and_red_chroma() {
        let out = convert(&[255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0], 2, 2);
        assert_eq!(out.len(), nv12_len(2, 2));
        assert!(out[4] < 128, "red U should be below neutral: {out:?}");
        assert!(out[5] > 128, "red V should be above neutral: {out:?}");
    }

    #[test]
    fn odd_dimensions_are_rejected() {
        let mut out = vec![0u8; nv12_len(2, 2)];
        assert!(rgb_to_nv12(&[0; 12], 3, 2, &mut out).is_err());
    }

    #[test]
    fn short_buffers_are_rejected() {
        let mut out = vec![0u8; nv12_len(2, 2)];
        assert!(rgb_to_nv12(&[0; 11], 2, 2, &mut out).is_err());
        assert!(rgb_to_nv12(&[0; 12], 2, 2, &mut out[..5]).is_err());
    }
}
