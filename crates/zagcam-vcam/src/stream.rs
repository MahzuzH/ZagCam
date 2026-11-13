//! The video stream: serves the engine's latest frame, or an animated test pattern while the
//! engine is not sending.
//!
//! Adapted from the CamLights project (Apache-2.0). See THIRD_PARTY_NOTICES.md.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use zagcam_vcam_shared::{FPS, FrameChannel, fill_test_pattern, nv12_len, now_ms};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::core::{ComObject, GUID, IUnknown, Interface, Ref, implement};

use crate::log::vlog;

const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / FPS as u64);
const CHANNEL_RETRY_MS: u64 = 1000;
/// Keep the last live frame through a transient read miss instead of flashing the pattern.
const LIVE_HOLD_MS: u64 = 250;

#[implement(IMFMediaStream2)]
pub struct MediaStream {
    state: Mutex<State>,
}

struct State {
    /// `None` once shut down.
    queue: Option<IMFMediaEventQueue>,
    source: Option<IMFMediaSource>,
    descriptor: IMFStreamDescriptor,
    stream_state: MF_STREAM_STATE,
    width: u32,
    height: u32,
    frame: Vec<u8>,
    channel: Option<FrameChannel>,
    channel_attempt_ms: Option<u64>,
    last_delivery: Option<Instant>,
    served: u64,
    last_live: Option<bool>,
    live_tick_ms: u64,
}

impl MediaStream {
    pub fn new(source: IMFMediaSource, descriptor: IMFStreamDescriptor) -> windows::core::Result<ComObject<Self>> {
        Ok(ComObject::new(MediaStream {
            state: Mutex::new(State {
                queue: Some(unsafe { MFCreateEventQueue()? }),
                source: Some(source),
                descriptor,
                stream_state: MF_STREAM_STATE_STOPPED,
                width: 0,
                height: 0,
                frame: Vec::new(),
                channel: None,
                channel_attempt_ms: None,
                last_delivery: None,
                served: 0,
                last_live: None,
                live_tick_ms: 0,
            }),
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn queue(&self) -> windows::core::Result<IMFMediaEventQueue> {
        self.lock().queue.clone().ok_or_else(|| MF_E_SHUTDOWN.into())
    }

    pub fn start(&self, width: u32, height: u32) -> windows::core::Result<()> {
        let mut state = self.lock();
        let queue = state.queue.clone().ok_or_else(|| windows::core::Error::from(MF_E_SHUTDOWN))?;
        state.width = width;
        state.height = height;
        state.frame = vec![0; nv12_len(width, height)];
        state.stream_state = MF_STREAM_STATE_RUNNING;
        state.served = 0;
        state.last_live = None;
        state.live_tick_ms = 0;
        unsafe {
            queue.QueueEventParamVar(MEStreamStarted.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::default())
        }
    }

    pub fn stop(&self) -> windows::core::Result<()> {
        let mut state = self.lock();
        let queue = state.queue.clone().ok_or_else(|| windows::core::Error::from(MF_E_SHUTDOWN))?;
        state.stream_state = MF_STREAM_STATE_STOPPED;
        unsafe {
            queue.QueueEventParamVar(MEStreamStopped.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::default())
        }
    }

    pub fn shutdown(&self) {
        let mut state = self.lock();
        if let Some(queue) = state.queue.take() {
            let _ = unsafe { queue.Shutdown() };
        }
        state.source = None;
        state.channel = None;
    }
}

impl State {
    fn ensure_channel(&mut self) {
        if self.channel.is_some() {
            return;
        }
        let now = now_ms();
        if self.channel_attempt_ms.is_some_and(|t| now.saturating_sub(t) < CHANNEL_RETRY_MS) {
            return;
        }
        self.channel_attempt_ms = Some(now);
        match FrameChannel::create() {
            Ok(channel) => {
                vlog!("frame channel ready");
                self.channel = Some(channel);
            }
            Err(e) => vlog!("creating frame channel failed: {e}"),
        }
    }

    /// Fills `frame` with the engine's latest frame, or the test pattern; returns whether it was live.
    fn render(&mut self) -> bool {
        self.ensure_channel();
        let (width, height) = (self.width, self.height);
        let now = now_ms();
        let mut live = self.channel.as_ref().is_some_and(|channel| {
            channel.announce_consumer(width, height);
            channel.read_frame(width, height, &mut self.frame)
        });
        if live {
            self.live_tick_ms = now;
        } else if self.live_tick_ms != 0 && now.saturating_sub(self.live_tick_ms) <= LIVE_HOLD_MS {
            // Keep the last live frame through a transient miss.
            live = true;
        } else {
            fill_test_pattern(&mut self.frame, width, height, now);
        }
        if self.last_live != Some(live) {
            vlog!("source is now {}", if live { "live (engine frames)" } else { "the test pattern" });
            self.last_live = Some(live);
        }
        live
    }
}

impl IMFMediaEventGenerator_Impl for MediaStream_Impl {
    fn GetEvent(&self, flags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> windows::core::Result<IMFMediaEvent> {
        let queue = self.queue()?;
        unsafe { queue.GetEvent(flags.0 as u32) }
    }
    fn BeginGetEvent(&self, callback: Ref<IMFAsyncCallback>, state: Ref<IUnknown>) -> windows::core::Result<()> {
        unsafe { self.queue()?.BeginGetEvent(callback.as_ref(), state.as_ref()) }
    }
    fn EndGetEvent(&self, result: Ref<IMFAsyncResult>) -> windows::core::Result<IMFMediaEvent> {
        unsafe { self.queue()?.EndGetEvent(result.as_ref()) }
    }
    fn QueueEvent(
        &self,
        kind: u32,
        extended: *const GUID,
        status: windows::core::HRESULT,
        value: *const PROPVARIANT,
    ) -> windows::core::Result<()> {
        unsafe { self.queue()?.QueueEventParamVar(kind, extended, status, value) }
    }
}

impl IMFMediaStream_Impl for MediaStream_Impl {
    fn GetMediaSource(&self) -> windows::core::Result<IMFMediaSource> {
        self.lock().source.clone().ok_or_else(|| MF_E_SHUTDOWN.into())
    }

    fn GetStreamDescriptor(&self) -> windows::core::Result<IMFStreamDescriptor> {
        let state = self.lock();
        if state.queue.is_none() {
            return Err(MF_E_SHUTDOWN.into());
        }
        Ok(state.descriptor.clone())
    }

    fn RequestSample(&self, token: Ref<IUnknown>) -> windows::core::Result<()> {
        let mut state = self.lock();
        let queue = state.queue.clone().ok_or_else(|| windows::core::Error::from(MF_E_SHUTDOWN))?;
        if state.width == 0 {
            return Err(MF_E_MEDIA_SOURCE_WRONGSTATE.into());
        }

        // Deliver at the advertised rate even if the client asks faster.
        if let Some(last) = state.last_delivery {
            let elapsed = last.elapsed();
            if elapsed < FRAME_INTERVAL {
                std::thread::sleep(FRAME_INTERVAL - elapsed);
            }
        }
        state.last_delivery = Some(Instant::now());

        let live = state.render();
        if state.served == 0 {
            vlog!("first sample ({})", if live { "live" } else { "test pattern" });
        }
        state.served += 1;

        unsafe {
            let len = state.frame.len() as u32;
            let buffer = MFCreateMemoryBuffer(len)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(state.frame.as_ptr(), data, state.frame.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(len)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(MFGetSystemTime())?;
            sample.SetSampleDuration(10_000_000 / FPS as i64)?;
            if let Some(token) = token.as_ref() {
                sample.SetUnknown(&MFSampleExtension_Token, token)?;
            }
            let unknown: IUnknown = sample.cast()?;
            queue.QueueEventParamUnk(MEMediaSample.0 as u32, &GUID::zeroed(), S_OK, &unknown)
        }
    }
}

impl IMFMediaStream2_Impl for MediaStream_Impl {
    fn SetStreamState(&self, value: MF_STREAM_STATE) -> windows::core::Result<()> {
        self.lock().stream_state = value;
        Ok(())
    }

    fn GetStreamState(&self) -> windows::core::Result<MF_STREAM_STATE> {
        Ok(self.lock().stream_state)
    }
}
