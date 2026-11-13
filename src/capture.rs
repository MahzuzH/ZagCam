use nokhwa::utils::{ApiBackend, CameraFormat, CameraIndex, CameraInfo, FrameFormat, RequestedFormat, RequestedFormatType};
use nokhwa::{query, Camera};

pub const MAX_W: usize = 1280;
pub const MAX_H: usize = 720;

pub fn list() -> Result<Vec<CameraInfo>, String> {
    query(ApiBackend::Auto).map_err(|why| format!("camera query failed: {why}"))
}

pub fn open(index: &CameraIndex) -> Result<Camera, String> {
    let w = MAX_W as u32;
    let h = MAX_H as u32;
    let attempts = [
        CameraFormat::new_from(w, h, FrameFormat::NV12, 30),
        CameraFormat::new_from(w, h, FrameFormat::MJPEG, 30),
        CameraFormat::new_from(w, h, FrameFormat::YUYV, 30),
    ];
    for guess in attempts {
        let requested = RequestedFormat::new::<nokhwa::pixel_format::RgbFormat>(
            RequestedFormatType::Closest(guess),
        );
        if let Ok(mut camera) = Camera::new(index.clone(), requested)
            && camera.open_stream().is_ok()
        {
            return Ok(camera);
        }
    }
    let fallback = RequestedFormat::new::<nokhwa::pixel_format::RgbFormat>(
        RequestedFormatType::AbsoluteHighestResolution,
    );
    let mut camera =
        Camera::new(index.clone(), fallback).map_err(|why| format!("cannot open camera: {why}"))?;
    camera.open_stream().map_err(|why| format!("cannot start stream: {why}"))?;
    Ok(camera)
}
