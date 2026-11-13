//! `zagcam vcam ...` — install and drive the virtual camera media source.

use std::path::PathBuf;
use std::process::Command;

use zagcam_vcam_mgr::{CAMERA_NAME, VirtualCamera};

const USAGE: &str = "Usage: zagcam vcam <command>

Commands:
  start [--seconds N]  create and start the virtual camera, then wait (default: until Ctrl+C)
  stop                 stop and remove a session camera created by another process
  status               show whether the media source is registered and if a source is live
  install              register the provider DLL (needs an elevated shell)
  uninstall            unregister the provider DLL (needs an elevated shell)";

pub fn run(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(());
    };
    match command {
        "start" => start(&args[1..]),
        "stop" => stop(),
        "status" => status(),
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
    println!("media source:   {}", if status.source_active { "live" } else { "not running" });
    Ok(())
}

fn parse_seconds(args: &[String]) -> Result<Option<f64>, String> {
    let mut seconds = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--seconds" | "-s" => {
                let raw = iter.next().ok_or("--seconds needs a value")?;
                seconds = Some(raw.parse::<f64>().map_err(|_| format!("bad seconds value {raw}"))?);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(seconds)
}

fn provider_dll() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|why| format!("cannot find this executable: {why}"))?;
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
        return Err(format!("`vcam {}` takes no arguments", if unregister { "uninstall" } else { "install" }));
    }
    let dll = if unregister {
        zagcam_vcam_mgr::registered_dll().map_or_else(provider_dll, Ok)?
    } else {
        provider_dll()?
    };
    let system = std::env::var_os("SystemRoot").ok_or("SystemRoot is not set")?;
    let regsvr32 = PathBuf::from(&system).join("System32").join("regsvr32.exe");
    let mut command = Command::new(regsvr32);
    command.arg("/s");
    if unregister {
        command.arg("/u");
    }
    command.arg(&dll);
    let status = command.status().map_err(|why| format!("running regsvr32 failed: {why}"))?;
    if !status.success() {
        return Err(format!(
            "regsvr32 failed ({status}); run the command from an elevated shell, or use tools/vcam/install-dev.ps1"
        ));
    }
    println!("{} {}", if unregister { "unregistered" } else { "registered" }, dll.display());
    Ok(())
}
