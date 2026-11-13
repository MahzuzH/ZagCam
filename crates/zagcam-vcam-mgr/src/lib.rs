//! Registers and drives the ZagCam virtual camera through `MFCreateVirtualCamera`.
//!
//! Adapted from the CamLights project (crates/camlights/src/vcam.rs, Apache-2.0,
//! https://github.com/pdinsd/CamLights). See THIRD_PARTY_NOTICES.md.
//!
//! The camera is created with session lifetime, so it exists only while this process holds the
//! `IMFVirtualCamera` and disappears when it is dropped or removed.

use std::path::PathBuf;

use zagcam_vcam_shared::{CLSID_STRING, FRIENDLY_NAME, FrameChannel};
use windows::core::PCWSTR;
use windows::Win32::Media::MediaFoundation::{
    IMFVirtualCamera, MF_VERSION, MFCreateVirtualCamera, MFStartup, MFVirtualCameraAccess_CurrentUser,
    MFVirtualCameraLifetime_Session, MFVirtualCameraType_SoftwareCameraSource, MFShutdown,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::HSTRING;

/// Keeps COM initialised for the lifetime of the value.
struct ComGuard;

impl ComGuard {
    fn init() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|why| format!("CoInitializeEx failed: {why}"))?;
        Ok(Self)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// Keeps Media Foundation started for the lifetime of the value.
struct MfGuard;

impl MfGuard {
    fn init() -> Result<Self, String> {
        unsafe { MFStartup(MF_VERSION, 0) }.map_err(|why| format!("MFStartup failed: {why}"))?;
        Ok(Self)
    }
}

impl Drop for MfGuard {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
    }
}

fn create() -> Result<IMFVirtualCamera, String> {
    unsafe {
        MFCreateVirtualCamera(
            MFVirtualCameraType_SoftwareCameraSource,
            MFVirtualCameraLifetime_Session,
            MFVirtualCameraAccess_CurrentUser,
            &HSTRING::from(FRIENDLY_NAME),
            &HSTRING::from(CLSID_STRING),
            None,
        )
    }
    .map_err(|why| {
        format!(
            "MFCreateVirtualCamera failed: {why} (is the media source DLL registered? run `zagcam vcam install`)"
        )
    })
}

/// A session-lifetime virtual camera. Dropping it removes the camera from the system.
pub struct VirtualCamera {
    camera: IMFVirtualCamera,
    _mf: MfGuard,
    _com: ComGuard,
}

impl VirtualCamera {
    /// Creates and starts the virtual camera. Must run on the thread that keeps the process alive.
    pub fn start() -> Result<Self, String> {
        let com = ComGuard::init()?;
        let mf = MfGuard::init()?;
        let camera = create()?;
        unsafe { camera.Start(None) }.map_err(|why| format!("starting the virtual camera failed: {why}"))?;
        Ok(Self { camera, _mf: mf, _com: com })
    }

    /// Stops the stream; the camera stays registered until dropped or removed.
    pub fn stop(&self) -> Result<(), String> {
        unsafe { self.camera.Stop() }.map_err(|why| format!("stopping the virtual camera failed: {why}"))
    }
}

impl Drop for VirtualCamera {
    fn drop(&mut self) {
        unsafe {
            let _ = self.camera.Stop();
            let _ = self.camera.Remove();
        }
    }
}

/// Removes a session camera created by another process, then shuts Media Foundation down.
pub fn remove() -> Result<(), String> {
    let com = ComGuard::init()?;
    let mf = MfGuard::init()?;
    let camera = create()?;
    unsafe { camera.Remove() }.map_err(|why| format!("removing the virtual camera failed: {why}"))?;
    drop(camera);
    drop(mf);
    drop(com);
    Ok(())
}

/// The DLL path COM has registered for our CLSID, if any.
pub fn registered_dll() -> Option<PathBuf> {
    let subkey = HSTRING::from(format!(r"Software\Classes\CLSID\{CLSID_STRING}\InprocServer32"));
    let mut buffer = [0u16; 1024];
    let mut bytes = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &subkey,
            PCWSTR::null(),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = (bytes as usize / 2).saturating_sub(1);
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
}

/// Whether a media source is currently publishing the shared frame channel.
pub fn source_active() -> bool {
    FrameChannel::open().is_ok()
}

/// Registration and runtime status for the `vcam status` command.
pub struct Status {
    pub registered: Option<PathBuf>,
    pub source_active: bool,
}

pub fn status() -> Status {
    Status { registered: registered_dll(), source_active: source_active() }
}

pub use zagcam_vcam_shared::FRIENDLY_NAME as CAMERA_NAME;
