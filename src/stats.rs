use std::time::Duration;

pub struct Snapshot {
    pub working_set: usize,
    pub cpu_percent: f64,
}

impl Snapshot {
    pub fn title(&self, frames: u64, elapsed: Duration) -> String {
        format!(
            "ZagCam | {:.1} fps | {:.1} MiB | {:.1}% cpu",
            frames as f64 / elapsed.as_secs_f64().max(f64::EPSILON),
            self.working_set as f64 / 1_048_576.0,
            self.cpu_percent
        )
    }
}

#[cfg(windows)]
mod platform {
    use super::Snapshot;
    use std::mem::{size_of, zeroed};
    use std::time::Instant;
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    pub struct Sampler {
        last_cpu: u64,
        last_at: Instant,
    }

    impl Sampler {
        pub fn new() -> Self {
            Self { last_cpu: cpu_time_100ns(), last_at: Instant::now() }
        }

        pub fn snapshot(&mut self) -> Snapshot {
            let now = Instant::now();
            let cpu = cpu_time_100ns();
            let elapsed = now.duration_since(self.last_at).as_secs_f64();
            let cpu_percent = if elapsed > 0.0 && cpu >= self.last_cpu {
                (cpu - self.last_cpu) as f64 / 10_000_000.0 / elapsed * 100.0
            } else {
                0.0
            };
            self.last_cpu = cpu;
            self.last_at = now;
            Snapshot { working_set: working_set(), cpu_percent }
        }
    }

    fn cpu_time_100ns() -> u64 {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let ok = unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        };
        if ok == 0 { 0 } else { filetime(kernel) + filetime(user) }
    }

    fn filetime(t: FILETIME) -> u64 {
        (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
    }

    fn working_set() -> usize {
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { zeroed() };
        counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
        if ok == 0 { 0 } else { counters.WorkingSetSize }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Snapshot;

    pub struct Sampler;

    impl Sampler {
        pub fn new() -> Self {
            Self
        }

        pub fn snapshot(&mut self) -> Snapshot {
            Snapshot { working_set: 0, cpu_percent: 0.0 }
        }
    }
}

pub use platform::Sampler;
