#![cfg_attr(not(windows), allow(unused))]

mod stats;

use minifb::{Key, Window, WindowOptions};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraFormat, FrameFormat, RequestedFormat, RequestedFormatType};
use nokhwa::{query, Camera};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const MAX_W: usize = 1280;
const MAX_H: usize = 720;

const USAGE: &str = "ZagCam M0 capture prototype

Usage:
  zagcam              open the first camera
  zagcam --camera N   open camera number N (see --list)
  zagcam --list       list available cameras
  zagcam --probe S    capture for S seconds without a window, print baseline
  zagcam --help       show this help

Keys: Esc quits.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("zagcam: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut list = false;
    let mut pick = None;
    let mut probe = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--list" | "-l" => list = true,
            "--camera" | "-c" => {
                let raw = args.next().ok_or("--camera needs an index")?;
                pick = Some(raw.parse::<usize>().map_err(|_| format!("bad camera index {raw}"))?);
            }
            "--probe" => {
                let raw = args.next().ok_or("--probe needs a number of seconds")?;
                probe = Some(raw.parse::<f64>().map_err(|_| format!("bad probe duration {raw}"))?);
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }

    let cameras = query(ApiBackend::Auto).map_err(|why| format!("camera query failed: {why}"))?;
    if cameras.is_empty() {
        return Err("no camera found".into());
    }
    if list {
        for (i, cam) in cameras.iter().enumerate() {
            println!("{i}: {} [{}]", cam.human_name(), cam.index().as_index().unwrap_or(0));
        }
        return Ok(());
    }
    let source = match pick {
        Some(i) => cameras.get(i).ok_or_else(|| format!("camera {i} not found"))?,
        None => &cameras[0],
    };

    let mut camera = open(source.index())?;
    let format = camera.camera_format();
    let (sw, sh) = (format.width() as usize, format.height() as usize);
    let (dw, dh) = fit(sw, sh, MAX_W, MAX_H);
    println!(
        "source: {} | {}x{} {}",
        source.human_name(),
        sw,
        sh,
        format.format()
    );

    let mut rgb = vec![0u8; sw * sh * 3];

    if let Some(secs) = probe {
        let mut sampler = stats::Sampler::new();
        let start = Instant::now();
        let mut frames: u64 = 0;
        while start.elapsed().as_secs_f64() < secs {
            let frame = camera.frame().map_err(|why| format!("capture failed: {why}"))?;
            frame
                .decode_image_to_buffer::<RgbFormat>(&mut rgb)
                .map_err(|why| format!("decode failed: {why}"))?;
            frames += 1;
        }
        let elapsed = start.elapsed().as_secs_f64();
        let snap = sampler.snapshot();
        println!(
            "M0 baseline: {:.1} fps over {:.1}s | working set {:.1} MiB | CPU {:.1}% of one core",
            frames as f64 / elapsed,
            elapsed,
            snap.working_set as f64 / 1_048_576.0,
            snap.cpu_percent
        );
        return Ok(());
    }

    let mut window = Window::new("ZagCam", dw, dh, WindowOptions { resize: true, ..Default::default() })
        .map_err(|why| format!("window failed: {why}"))?;

    let mut pixels = vec![0u32; dw * dh];
    let xs: Vec<usize> = (0..dw).map(|x| x * sw / dw).collect();
    let ys: Vec<usize> = (0..dh).map(|y| y * sh / dh).collect();

    let mut sampler = stats::Sampler::new();
    let start = Instant::now();
    let mut last_stats = Instant::now();
    let mut frames: u64 = 0;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let frame = match camera.frame() {
            Ok(frame) => frame,
            Err(why) => {
                window.set_title(&format!("ZagCam - camera error: {why}"));
                break;
            }
        };
        frame
            .decode_image_to_buffer::<RgbFormat>(&mut rgb)
            .map_err(|why| format!("decode failed: {why}"))?;
        scale_rgb_to_0rgb(&rgb, sw, &xs, &ys, &mut pixels, dw);

        window
            .update_with_buffer(&pixels, dw, dh)
            .map_err(|why| format!("present failed: {why}"))?;
        frames += 1;

        if last_stats.elapsed() >= Duration::from_secs(1) {
            let snap = sampler.snapshot();
            window.set_title(&snap.title(frames, start.elapsed()));
            last_stats = Instant::now();
        }
    }

    let snap = sampler.snapshot();
    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "M0 baseline: {:.1} fps over {:.1}s | working set {:.1} MiB | CPU {:.1}% of one core",
        frames as f64 / elapsed,
        elapsed,
        snap.working_set as f64 / 1_048_576.0,
        snap.cpu_percent
    );
    Ok(())
}

fn open(index: &nokhwa::utils::CameraIndex) -> Result<Camera, String> {
    let w = MAX_W as u32;
    let h = MAX_H as u32;
    let attempts = [
        CameraFormat::new_from(w, h, FrameFormat::NV12, 30),
        CameraFormat::new_from(w, h, FrameFormat::MJPEG, 30),
        CameraFormat::new_from(w, h, FrameFormat::YUYV, 30),
    ];
    for guess in attempts {
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(guess));
        if let Ok(mut camera) = Camera::new(index.clone(), requested)
            && camera.open_stream().is_ok()
        {
            return Ok(camera);
        }
    }
    let fallback = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
    let mut camera =
        Camera::new(index.clone(), fallback).map_err(|why| format!("cannot open camera: {why}"))?;
    camera.open_stream().map_err(|why| format!("cannot start stream: {why}"))?;
    Ok(camera)
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
            *pixel = (u32::from(src[i]) << 16) | (u32::from(src[i + 1]) << 8) | u32::from(src[i + 2]);
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
