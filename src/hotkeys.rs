//! Fixed global hotkeys for toggle, next preset, and off.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyEvent {
    Toggle,
    NextPreset,
    Off,
    Unavailable,
}

const TOGGLE: i32 = 1;
const NEXT: i32 = 2;
const OFF: i32 = 3;

pub struct Hotkeys {
    rx: Receiver<HotkeyEvent>,
    thread_id: Arc<AtomicU32>,
    join: Option<JoinHandle<()>>,
}

impl Hotkeys {
    pub fn start() -> Self {
        let (tx, rx) = channel();
        let thread_id = Arc::new(AtomicU32::new(0));
        let id = Arc::clone(&thread_id);
        let join = std::thread::Builder::new()
            .name("zagcam-hotkeys".into())
            .spawn(move || run(tx, id))
            .ok();
        for _ in 0..100 {
            if thread_id.load(Ordering::Relaxed) != 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Self {
            rx,
            thread_id,
            join,
        }
    }

    pub fn try_recv(&self) -> Option<HotkeyEvent> {
        self.rx.try_recv().ok()
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        let id = self.thread_id.load(Ordering::Relaxed);
        if id != 0 {
            #[cfg(windows)]
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                    id,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT,
                    0,
                    0,
                );
            }
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(windows)]
fn run(tx: std::sync::mpsc::Sender<HotkeyEvent>, thread_id: Arc<AtomicU32>) {
    use std::mem::zeroed;
    use std::ptr::null_mut;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, MSG,
        WM_HOTKEY,
    };

    unsafe {
        thread_id.store(GetCurrentThreadId(), Ordering::Relaxed);
        let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
        let registered = [
            RegisterHotKey(null_mut(), TOGGLE, modifiers, b'B' as u32),
            RegisterHotKey(null_mut(), NEXT, modifiers, b'N' as u32),
            RegisterHotKey(null_mut(), OFF, modifiers, b'O' as u32),
        ];
        if registered.contains(&0) {
            for id in [TOGGLE, NEXT, OFF] {
                UnregisterHotKey(null_mut(), id);
            }
            let _ = tx.send(HotkeyEvent::Unavailable);
            return;
        }
        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            if message.message == WM_HOTKEY {
                let event = match message.wParam as i32 {
                    TOGGLE => Some(HotkeyEvent::Toggle),
                    NEXT => Some(HotkeyEvent::NextPreset),
                    OFF => Some(HotkeyEvent::Off),
                    _ => None,
                };
                if let Some(event) = event {
                    let _ = tx.send(event);
                }
            }
        }
        for id in [TOGGLE, NEXT, OFF] {
            UnregisterHotKey(null_mut(), id);
        }
    }
}

#[cfg(not(windows))]
fn run(tx: std::sync::mpsc::Sender<HotkeyEvent>, _thread_id: Arc<AtomicU32>) {
    let _ = tx.send(HotkeyEvent::Unavailable);
}

#[cfg(test)]
mod tests {
    use super::{HotkeyEvent, NEXT, OFF, TOGGLE};

    #[test]
    fn hotkey_ids_and_events_are_distinct() {
        assert_ne!(TOGGLE, NEXT);
        assert_ne!(NEXT, OFF);
        assert_ne!(TOGGLE, OFF);
        assert_ne!(HotkeyEvent::Toggle, HotkeyEvent::Off);
    }
}

