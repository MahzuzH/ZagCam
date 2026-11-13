//! The contract between the ZagCam engine and its virtual camera media source.
//!
//! Adapted from the CamLights project (crates/camlights-vcam-shared, Apache-2.0,
//! https://github.com/pdinsd/CamLights). See THIRD_PARTY_NOTICES.md.
//!
//! The media source runs inside the Windows Camera Frame Server service, so it creates a
//! `Global\` section (services hold the privilege to do that) and the engine opens it. The
//! engine writes NV12 frames under a sequence lock; the source publishes the resolution its
//! client selected and a heartbeat, so the engine only converts frames while someone watches.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
    OpenFileMappingW, PAGE_READWRITE, UnmapViewOfFile,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::core::{GUID, HSTRING, w};

pub const CLSID_VIRTUAL_CAMERA: GUID = GUID::from_u128(0x8e7b2a4c_1d9f_4e3b_9c5a_6f0d2b7e4a19);
pub const CLSID_STRING: &str = "{8E7B2A4C-1D9F-4E3B-9C5A-6F0D2B7E4A19}";
/// Name under which applications list the virtual camera.
pub const FRIENDLY_NAME: &str = "ZagCam Camera";

pub const MAX_WIDTH: u32 = 1920;
pub const MAX_HEIGHT: u32 = 1080;
pub const FPS: u32 = 30;

/// A frame or heartbeat older than this is treated as absent.
pub const STALE_AFTER_MS: u64 = 1500;

const MAPPING_NAME: &str = r"Global\ZagCamVirtualCamera";
/// Production channel name. `Global\` lets the Frame Server service and the engine share it.
pub const CHANNEL_NAME: &str = MAPPING_NAME;
const MAGIC: u32 = u32::from_le_bytes(*b"ZGvc");
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 64;
const DATA_CAPACITY: usize = nv12_len(MAX_WIDTH, MAX_HEIGHT);
const MAPPING_BYTES: usize = HEADER_BYTES + DATA_CAPACITY;

pub const fn nv12_len(width: u32, height: u32) -> usize {
    width as usize * height as usize * 3 / 2
}

pub fn now_ms() -> u64 {
    unsafe { GetTickCount64() }
}

#[repr(C)]
struct Header {
    magic: AtomicU32,
    version: AtomicU32,
    // Written by the media source.
    requested_width: AtomicU32,
    requested_height: AtomicU32,
    consumer_tick: AtomicU64,
    // Written by the engine, guarded by `seq` (odd while a write is in progress).
    seq: AtomicU64,
    width: AtomicU32,
    height: AtomicU32,
    producer_tick: AtomicU64,
}

const _: () = assert!(size_of::<Header>() <= HEADER_BYTES);

/// A mapped view of the frame channel.
pub struct FrameChannel {
    mapping: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

// The view is shared memory accessed only through atomics and the sequence lock.
unsafe impl Send for FrameChannel {}
unsafe impl Sync for FrameChannel {}

impl FrameChannel {
    /// Creates (or opens an existing) channel. Called by the media source in the Frame Server.
    pub fn create() -> windows::core::Result<Self> {
        Self::create_named(CHANNEL_NAME)
    }

    /// Creates a channel with a specific name. Tests use `Local\...`, because a normal user
    /// cannot create a `Global\` section.
    pub fn create_named(name: &str) -> windows::core::Result<Self> {
        // SYSTEM, LocalService and administrators get full access; interactive users read and write.
        let sddl = w!("D:P(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;BA)(A;;GRGW;;;IU)");
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, SDDL_REVISION_1, &mut descriptor, None)?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let mapping = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                Some(&attributes),
                PAGE_READWRITE,
                0,
                MAPPING_BYTES as u32,
                &HSTRING::from(name),
            );
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            let channel = Self::map(mapping?)?;
            let header = channel.header();
            if header.magic.load(Ordering::Acquire) != MAGIC {
                header.version.store(VERSION, Ordering::Relaxed);
                header.magic.store(MAGIC, Ordering::Release);
            }
            Ok(channel)
        }
    }

    /// Opens the channel created by a running media source. Called by the engine.
    pub fn open() -> windows::core::Result<Self> {
        Self::open_named(CHANNEL_NAME)
    }

    /// Opens a channel with a specific name (tests use a `Local\...` name).
    pub fn open_named(name: &str) -> windows::core::Result<Self> {
        let mapping = unsafe {
            OpenFileMappingW((FILE_MAP_READ | FILE_MAP_WRITE).0, false, &HSTRING::from(name))?
        };
        let channel = Self::map(mapping)?;
        let header = channel.header();
        if header.magic.load(Ordering::Acquire) != MAGIC || header.version.load(Ordering::Relaxed) != VERSION {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_UNEXPECTED,
                "virtual camera channel has an incompatible layout",
            ));
        }
        Ok(channel)
    }

    fn map(mapping: HANDLE) -> windows::core::Result<Self> {
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, MAPPING_BYTES) };
        if view.Value.is_null() {
            let error = windows::core::Error::from_thread();
            unsafe {
                let _ = CloseHandle(mapping);
            }
            return Err(error);
        }
        Ok(Self { mapping, view })
    }

    fn header(&self) -> &Header {
        unsafe { &*(self.view.Value as *const Header) }
    }

    fn data(&self) -> *mut u8 {
        unsafe { (self.view.Value as *mut u8).add(HEADER_BYTES) }
    }

    // ---- media source side ----

    /// Records the resolution the client selected and that a client is reading frames now.
    pub fn announce_consumer(&self, width: u32, height: u32) {
        let header = self.header();
        header.requested_width.store(width, Ordering::Relaxed);
        header.requested_height.store(height, Ordering::Relaxed);
        header.consumer_tick.store(now_ms(), Ordering::Release);
    }

    /// Copies the latest frame into `out` if it is fresh and exactly `width`×`height`.
    /// Retries briefly when a write is in flight so a fast producer does not cause a miss.
    pub fn read_frame(&self, width: u32, height: u32, out: &mut [u8]) -> bool {
        for _ in 0..4 {
            if self.try_read_frame(width, height, out) {
                return true;
            }
            std::hint::spin_loop();
        }
        false
    }

    fn try_read_frame(&self, width: u32, height: u32, out: &mut [u8]) -> bool {
        let header = self.header();
        let len = nv12_len(width, height);
        if out.len() < len || len > DATA_CAPACITY {
            return false;
        }
        let before = header.seq.load(Ordering::Acquire);
        if before % 2 == 1
            || header.width.load(Ordering::Relaxed) != width
            || header.height.load(Ordering::Relaxed) != height
            || now_ms().saturating_sub(header.producer_tick.load(Ordering::Relaxed)) > STALE_AFTER_MS
        {
            return false;
        }
        unsafe { std::ptr::copy_nonoverlapping(self.data(), out.as_mut_ptr(), len) };
        fence(Ordering::Acquire);
        header.seq.load(Ordering::Relaxed) == before
    }

    // ---- engine side ----

    /// The resolution a client is reading, if any client read within [`STALE_AFTER_MS`].
    pub fn active_request(&self) -> Option<(u32, u32)> {
        let header = self.header();
        let tick = header.consumer_tick.load(Ordering::Acquire);
        if now_ms().saturating_sub(tick) > STALE_AFTER_MS {
            return None;
        }
        let (width, height) =
            (header.requested_width.load(Ordering::Relaxed), header.requested_height.load(Ordering::Relaxed));
        (width > 0 && height > 0 && nv12_len(width, height) <= DATA_CAPACITY).then_some((width, height))
    }

    /// Size of the last frame the engine published, if any.
    pub fn producer_size(&self) -> Option<(u32, u32)> {
        let header = self.header();
        let (width, height) =
            (header.width.load(Ordering::Relaxed), header.height.load(Ordering::Relaxed));
        (width > 0 && height > 0).then_some((width, height))
    }

    /// Milliseconds since the engine last published a frame, if it ever did.
    pub fn producer_age_ms(&self) -> Option<u64> {
        self.producer_size()?;
        let tick = self.header().producer_tick.load(Ordering::Relaxed);
        (tick > 0).then(|| now_ms().saturating_sub(tick))
    }

    /// Whether the engine published a frame within [`STALE_AFTER_MS`].
    pub fn producer_is_fresh(&self) -> bool {
        self.producer_age_ms().is_some_and(|age| age <= STALE_AFTER_MS)
    }

    /// Publishes a `width`×`height` NV12 frame produced by `fill`.
    pub fn write_frame(&self, width: u32, height: u32, fill: impl FnOnce(&mut [u8])) {
        let len = nv12_len(width, height);
        assert!(len <= DATA_CAPACITY, "frame exceeds the channel capacity");
        let header = self.header();
        let seq = header.seq.load(Ordering::Relaxed);
        header.seq.store(seq | 1, Ordering::Relaxed);
        fence(Ordering::Release);
        header.width.store(width, Ordering::Relaxed);
        header.height.store(height, Ordering::Relaxed);
        fill(unsafe { std::slice::from_raw_parts_mut(self.data(), len) });
        header.producer_tick.store(now_ms(), Ordering::Relaxed);
        header.seq.store((seq | 1) + 1, Ordering::Release);
    }
}

impl Drop for FrameChannel {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.mapping);
        }
    }
}

#[cfg(test)]
impl FrameChannel {
    fn set_producer_tick(&self, tick: u64) {
        self.header().producer_tick.store(tick, Ordering::Relaxed);
    }

    fn set_consumer_tick(&self, tick: u64) {
        self.header().consumer_tick.store(tick, Ordering::Relaxed);
    }

    fn set_seq(&self, seq: u64) {
        self.header().seq.store(seq, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameChannel, STALE_AFTER_MS, nv12_len, now_ms};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A `Local\` channel with a unique name, safe to create as a normal user and in parallel tests.
    fn test_channel() -> FrameChannel {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!(r"Local\ZagCamTest-{}-{}", std::process::id(), n);
        FrameChannel::create_named(&name).expect("create test channel")
    }

    fn filled(width: u32, height: u32, value: u8) -> Vec<u8> {
        vec![value; nv12_len(width, height)]
    }

    #[test]
    fn round_trip_returns_the_written_frame() {
        let channel = test_channel();
        let sent = filled(640, 480, 200);
        channel.write_frame(640, 480, |out| out.copy_from_slice(&sent));

        let mut got = filled(640, 480, 0);
        assert!(channel.read_frame(640, 480, &mut got));
        assert_eq!(got, sent);
    }

    #[test]
    fn torn_write_is_rejected() {
        let channel = test_channel();
        channel.write_frame(640, 480, |out| out.fill(10));
        channel.set_seq(1); // odd: a write is "in progress"

        let mut got = filled(640, 480, 0);
        assert!(!channel.read_frame(640, 480, &mut got));
    }

    #[test]
    fn size_mismatch_is_rejected() {
        let channel = test_channel();
        channel.write_frame(640, 480, |out| out.fill(10));

        let mut got = filled(1280, 720, 0);
        assert!(!channel.read_frame(1280, 720, &mut got));
    }

    #[test]
    fn stale_producer_is_rejected() {
        let channel = test_channel();
        channel.write_frame(640, 480, |out| out.fill(10));
        channel.set_producer_tick(now_ms().saturating_sub(STALE_AFTER_MS + 1000));

        let mut got = filled(640, 480, 0);
        assert!(!channel.read_frame(640, 480, &mut got));
        assert!(!channel.producer_is_fresh());
        assert!(channel.producer_age_ms().is_some_and(|age| age > STALE_AFTER_MS));
    }

    #[test]
    fn consumer_request_tracks_heartbeat() {
        let channel = test_channel();
        assert_eq!(channel.active_request(), None);

        channel.announce_consumer(1280, 720);
        assert_eq!(channel.active_request(), Some((1280, 720)));

        channel.set_consumer_tick(now_ms().saturating_sub(STALE_AFTER_MS + 1000));
        assert_eq!(channel.active_request(), None);
    }
}
