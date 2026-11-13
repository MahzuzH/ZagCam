//! `zagcam vcam ...` — install and drive the virtual camera media source.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use zagcam_vcam_mgr::{CAMERA_NAME, FrameChannel, VirtualCamera};

const USAGE: &str = "Usage: zagcam vcam <command>

Commands:
  start [--seconds N]  create and start the virtual camera, then wait (default: until Ctrl+C)
  stop                 stop and remove a session camera created by another process
  status               show whether the media source is registered and if a source is live
  watch [--seconds N]  show the shared frame channel state (client resolution, engine freshness)
  feed [--seconds N]   publish test frames into the channel (dev check that the camera serves them)
  install              copy and register the provider DLL (asks for elevation once)
  uninstall            unregister and remove the provider DLL (asks for elevation once)";

pub fn run(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(());
    };
    match command {
        "start" => start(&args[1..]),
        "stop" => stop(),
        "status" => status(),
        "watch" => watch(&args[1..]),
        "feed" => feed(&args[1..]),
        "install" => register(&args[1..], false),
        "uninstall" => register(&args[1..], true),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown vcam command {other}\n\n{USAGE}")),
    }
}

fn start(args: &[String]) -> Result<(), String> {
    let seconds = parse_seconds(args)?;
    let camera = VirtualCamera::start()?;
    println!("virtual camera '{CAMERA_NAME}' started (showing the built-in test pattern)");
    match seconds {
        Some(secs) => {
            println!("running for {secs} s; open the Windows Camera app or ffmpeg to see it");
            std::thread::sleep(std::time::Duration::from_secs_f64(secs));
            camera.stop()?;
        }
        None => {
            println!("press Ctrl+C to remove it");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
    Ok(())
}

fn stop() -> Result<(), String> {
    zagcam_vcam_mgr::remove()?;
    println!("removed the session virtual camera (if one was running)");
    Ok(())
}

fn status() -> Result<(), String> {
    let status = zagcam_vcam_mgr::status();
    match &status.registered {
        Some(path) => println!("registered DLL: {}", path.display()),
        None => println!("registered DLL: none (run `zagcam vcam install`)"),
    }
    println!("camera name:    {CAMERA_NAME} Windows Virtual Camera");
    println!(
        "media source:   {}",
        if status.source_active {
            "live"
        } else {
            "not running"
        }
    );
    Ok(())
}

fn watch(args: &[String]) -> Result<(), String> {
    let seconds = parse_seconds(args)?;
    let start = std::time::Instant::now();
    loop {
        match zagcam_vcam_mgr::channel_info() {
            None => println!("media source: not running (is `zagcam vcam start` running?)"),
            Some(info) => {
                let requested = info
                    .requested
                    .map_or_else(|| "none".to_string(), |(w, h)| format!("{w}x{h}"));
                let engine = match (info.producer, info.producer_age_ms, info.producer_fresh) {
                    (Some((w, h)), Some(age), true) => format!("{w}x{h}, fresh ({age} ms)"),
                    (Some((w, h)), Some(age), false) => format!("{w}x{h}, stale ({age} ms)"),
                    (Some((w, h)), None, _) => format!("{w}x{h}, never"),
                    _ => "none".to_string(),
                };
                println!("client reading: {requested} | engine frame: {engine}");
            }
        }
        if seconds.is_some_and(|limit| start.elapsed().as_secs_f64() >= limit) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn feed(args: &[String]) -> Result<(), String> {
    let seconds = parse_seconds(args)?;
    println!(
        "waiting for the virtual camera channel (run `zagcam vcam start` and open the camera)..."
    );
    let channel = open_channel_with_retry()?;
    println!("channel open; publishing test frames while a client reads");

    let start = Instant::now();
    let mut last: Option<(u32, u32)> = None;
    loop {
        match channel.active_request() {
            Some((w, h)) => {
                if last != Some((w, h)) {
                    println!("feeding {w}x{h}");
                    last = Some((w, h));
                }
                let tick = producer_now_ms();
                channel.write_frame(w, h, |out| fill_producer_pattern(out, w, h, tick));
            }
            None => {
                if last.take().is_some() {
                    println!("no client reading; idle");
                }
            }
        }
        if seconds.is_some_and(|limit| start.elapsed().as_secs_f64() >= limit) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(33));
    }
}

fn open_channel_with_retry() -> Result<FrameChannel, String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(channel) = FrameChannel::open() {
            return Ok(channel);
        }
        if Instant::now() >= deadline {
            return Err(
                "the shared channel never appeared; is `zagcam vcam start` running and an app reading the camera?"
                    .into(),
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn producer_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Distinct NV12 pattern for the dev feed: moving color bars plus a travelling bright block, so a
/// live feed is obviously different from the provider's fallback pattern.
fn fill_producer_pattern(frame: &mut [u8], width: u32, height: u32, tick: u64) {
    let (w, h) = (width as usize, height as usize);
    let luma = w * h;
    let phase = (tick / 40) as usize;
    for y in 0..h {
        for x in 0..w {
            let bar = ((x * 8 / w.max(1)) + phase) % 8;
            frame[y * w + x] = match bar {
                0 => 40,
                1 => 90,
                2 => 140,
                3 => 190,
                4 => 70,
                5 => 120,
                6 => 170,
                7 => 220,
                _ => 128,
            };
        }
    }
    // Travelling white block in the luma plane.
    let block = w / 8;
    let bx = (tick / 8) as usize % (w + block);
    for y in (h / 3)..(h / 3 + block).min(h) {
        for x in bx..(bx + block).min(w) {
            frame[y * w + x] = 235;
        }
    }
    // Chroma bars (U/V interleaved, half resolution).
    let cw = w / 2;
    for y in 0..h / 2 {
        for x in 0..cw {
            let bar = ((x * 8 / cw.max(1)) + phase) % 8;
            let (u, v) = match bar {
                0 => (128, 128),
                1 => (90, 200),
                2 => (200, 90),
                3 => (128, 255),
                4 => (255, 128),
                5 => (90, 90),
                6 => (200, 200),
                _ => (160, 40),
            };
            let i = luma + y * w + x * 2;
            frame[i] = u;
            frame[i + 1] = v;
        }
    }
}

fn parse_seconds(args: &[String]) -> Result<Option<f64>, String> {
    let mut seconds = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--seconds" | "-s" => {
                let raw = iter.next().ok_or("--seconds needs a value")?;
                seconds = Some(
                    raw.parse::<f64>()
                        .map_err(|_| format!("bad seconds value {raw}"))?,
                );
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(seconds)
}

fn provider_dll() -> Result<PathBuf, String> {
    let exe =
        std::env::current_exe().map_err(|why| format!("cannot find this executable: {why}"))?;
    let dll = exe.with_file_name("zagcam_vcam.dll");
    if dll.exists() {
        return Ok(dll);
    }
    Err(format!(
        "{} not found next to the executable; build it with `cargo build -p zagcam-vcam --release`",
        dll.display()
    ))
}

fn register(args: &[String], unregister: bool) -> Result<(), String> {
    if !args.is_empty() {
        return Err(format!(
            "`vcam {}` takes no arguments",
            if unregister { "uninstall" } else { "install" }
        ));
    }
    if !is_elevated() {
        return elevate(if unregister { "uninstall" } else { "install" });
    }
    if unregister { uninstall() } else { install() }
}

fn install_root() -> Option<PathBuf> {
    Some(program_files()?.join("ZagCam").join("VirtualCamera"))
}

#[cfg(windows)]
fn program_files() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_ProgramFiles)
}

#[cfg(not(windows))]
fn program_files() -> Option<PathBuf> {
    std::env::var_os("ProgramFiles").map(PathBuf::from)
}

#[cfg(windows)]
fn system32() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_System)
}

#[cfg(not(windows))]
fn system32() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn known_folder(folder: &windows_sys::core::GUID) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};

    unsafe {
        let mut raw: *mut u16 = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(
            folder,
            KF_FLAG_DEFAULT as u32,
            std::ptr::null_mut(),
            &mut raw,
        );
        if hr < 0 || raw.is_null() {
            return None;
        }
        let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
        let path = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(raw, len)));
        CoTaskMemFree(raw.cast());
        Some(path)
    }
}

fn install_dll(source: &Path) -> Result<PathBuf, String> {
    let bytes =
        fs::read(source).map_err(|why| format!("reading {} failed: {why}", source.display()))?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let root = install_root().ok_or("ProgramFiles is not set")?;
    let dir = root.join(hash);
    fs::create_dir_all(&dir).map_err(|why| format!("creating {} failed: {why}", dir.display()))?;
    let dest = dir.join("zagcam_vcam.dll");
    if !dest.exists() {
        let temp = dir.join(format!("zagcam_vcam.dll.tmp-{}", std::process::id()));
        fs::write(&temp, &bytes)
            .map_err(|why| format!("writing {} failed: {why}", temp.display()))?;
        fs::rename(&temp, &dest)
            .map_err(|why| format!("moving the DLL into {} failed: {why}", dest.display()))?;
    }
    Ok(dest)
}

fn prune_installs(root: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep || !path.is_dir() {
            continue;
        }
        if let Err(why) = fs::remove_dir_all(&path) {
            println!("could not remove old install {}: {why}", path.display());
        }
    }
}

fn install() -> Result<(), String> {
    let source = provider_dll()?;
    let installed = install_dll(&source)?;
    run_regsvr32(&installed, false)?;
    println!("registered {}", installed.display());
    if let (Some(hash_dir), Some(root)) = (
        installed.parent(),
        installed.parent().and_then(Path::parent),
    ) {
        prune_installs(root, hash_dir);
    }
    Ok(())
}

fn uninstall() -> Result<(), String> {
    if let Err(why) = zagcam_vcam_mgr::remove() {
        println!("session camera: {why}");
    }
    match zagcam_vcam_mgr::registered_dll() {
        Some(dll) if dll.exists() => run_regsvr32(&dll, true)?,
        _ => zagcam_vcam_mgr::unregister_clsid()?,
    }
    if let Some(root) = install_root() {
        match fs::remove_dir_all(&root) {
            Ok(()) => println!("removed {}", root.display()),
            Err(why) if why.kind() == std::io::ErrorKind::NotFound => {}
            Err(why) => println!(
                "could not remove {}: {why} (close apps using the camera and retry)",
                root.display()
            ),
        }
    }
    println!("uninstalled");
    Ok(())
}

fn run_regsvr32(dll: &Path, unregister: bool) -> Result<(), String> {
    let regsvr32 = system32()
        .ok_or("cannot find the Windows system directory")?
        .join("regsvr32.exe");
    let mut command = Command::new(regsvr32);
    command.arg("/s");
    if unregister {
        command.arg("/u");
    }
    command.arg(dll);
    let status = command
        .status()
        .map_err(|why| format!("running regsvr32 failed: {why}"))?;
    if !status.success() {
        return Err(format!("regsvr32 failed ({status}) for {}", dll.display()));
    }
    Ok(())
}

#[cfg(windows)]
fn is_elevated() -> bool {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
fn is_elevated() -> bool {
    false
}

#[cfg(windows)]
fn elevate(command: &str) -> Result<(), String> {
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, INFINITE, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe =
        std::env::current_exe().map_err(|why| format!("cannot find this executable: {why}"))?;
    let exe = exe
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let verb = "runas".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let params = format!("vcam {command}")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: params.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(format!(
            "elevation failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        windows_sys::Win32::Foundation::CloseHandle(info.hProcess);
        if code == 0 {
            Ok(())
        } else {
            Err(format!(
                "elevated `vcam {command}` failed with exit code {code}"
            ))
        }
    }
}

#[cfg(not(windows))]
fn elevate(_command: &str) -> Result<(), String> {
    Err("virtual camera installation is Windows-only".into())
}
