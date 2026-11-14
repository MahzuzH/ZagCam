//! Optional launch-at-login through the per-user Run key.

use std::path::Path;
use std::process::Command;

pub const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE_NAME: &str = "ZagCam";

pub fn value_for(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|why| format!("cannot find this executable: {why}"))?;
    if enabled {
        let value = value_for(&exe);
        let status = Command::new("reg.exe")
            .args([
                "add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &value, "/f",
            ])
            .status()
            .map_err(|why| format!("running reg add failed: {why}"))?;
        if !status.success() {
            return Err(format!("reg add failed ({status})"));
        }
    } else {
        let _ = Command::new("reg.exe")
            .args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])
            .status();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{value_for, VALUE_NAME};
    use std::path::Path;

    #[test]
    fn autostart_value_is_quoted_for_paths_with_spaces() {
        let value = value_for(Path::new(r"C:\Program Files\ZagCam\ZagCam.exe"));
        assert_eq!(value, r#""C:\Program Files\ZagCam\ZagCam.exe""#);
        assert_eq!(VALUE_NAME, "ZagCam");
    }
}