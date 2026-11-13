//! Persisted configuration in `%APPDATA%\ZagCam\settings.json`.

use std::fs;
use std::path::{Path, PathBuf};

pub const VERSION: u32 = 1;
pub const DEFAULT_BLUR: u8 = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Blur,
    Image,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Blur => "blur",
            Self::Image => "image",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "blur" => Some(Self::Blur),
            "image" => Some(Self::Image),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub version: u32,
    pub camera_name: Option<String>,
    pub camera_index: usize,
    pub mode: Mode,
    pub blur: u8,
    pub background: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            camera_name: None,
            camera_index: 0,
            mode: Mode::Off,
            blur: DEFAULT_BLUR,
            background: None,
        }
    }
}

impl Settings {
    /// `%APPDATA%\ZagCam\settings.json`, or `None` when `APPDATA` is not set.
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("APPDATA")
            .map(|base| PathBuf::from(base).join("ZagCam").join("settings.json"))
    }

    /// Reads the saved settings. A missing file means defaults; damages are logged once and the
    /// valid fields are kept.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(text) => {
                let parsed = Self::parse(&text);
                if let Some(problem) = parsed.problem {
                    eprintln!("zagcam: {problem} ({}); using defaults for the rest", path.display());
                }
                parsed.settings
            }
            Err(why) if why.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(why) => {
                eprintln!(
                    "zagcam: cannot read {}: {why}; using defaults",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Writes a temporary file and renames it over the old settings.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("APPDATA is not set")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|why| format!("creating {} failed: {why}", dir.display()))?;
        }
        let temp = path.with_extension(format!("json.tmp-{}", std::process::id()));
        fs::write(&temp, self.to_json()).map_err(|why| {
            format!("writing {} failed: {why}", temp.display())
        })?;
        fs::rename(&temp, &path).map_err(|why| {
            let _ = fs::remove_file(&temp);
            format!("replacing {} failed: {why}", path.display())
        })
    }

    pub fn to_json(&self) -> String {
        let camera_name = match &self.camera_name {
            Some(name) => serde_json::Value::String(name.clone()),
            None => serde_json::Value::Null,
        };
        let background = match &self.background {
            Some(path) => serde_json::Value::String(path.display().to_string()),
            None => serde_json::Value::Null,
        };
        serde_json::json!({
            "version": self.version,
            "camera_name": camera_name,
            "camera_index": self.camera_index,
            "mode": self.mode.as_str(),
            "blur": self.blur,
            "background": background,
        })
        .to_string()
    }

    fn parse(text: &str) -> Parsed {
        let mut settings = Self::default();
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(text) else {
            return Parsed {
                settings,
                problem: Some("settings.json is corrupt".to_string()),
            };
        };
        let mut problem = false;

        if let Some(value) = map.get("version") {
            match value.as_u64() {
                Some(version) => settings.version = version as u32,
                None => problem = true,
            }
        }
        if let Some(value) = map.get("camera_name") {
            match value {
                serde_json::Value::Null => {}
                serde_json::Value::String(name) if !name.is_empty() => {
                    settings.camera_name = Some(name.clone());
                }
                _ => problem = true,
            }
        }
        if let Some(value) = map.get("camera_index") {
            match value.as_u64() {
                Some(index) => settings.camera_index = index as usize,
                None => problem = true,
            }
        }
        if let Some(value) = map.get("mode") {
            match value.as_str().and_then(Mode::parse) {
                Some(mode) => settings.mode = mode,
                None => problem = true,
            }
        }
        if let Some(value) = map.get("blur") {
            match value.as_u64() {
                Some(blur) if blur <= u8::MAX as u64 => settings.blur = blur as u8,
                _ => problem = true,
            }
        }
        if let Some(value) = map.get("background") {
            match value {
                serde_json::Value::Null => {}
                serde_json::Value::String(path) if !path.is_empty() => {
                    settings.background = Some(PathBuf::from(path));
                }
                _ => problem = true,
            }
        }

        Parsed {
            settings,
            problem: problem.then(|| "settings.json has invalid fields".to_string()),
        }
    }

    /// The selected camera, preferring the stable name and falling back to the index.
    pub fn camera_index(&self, cameras: &[nokhwa::utils::CameraInfo]) -> Option<usize> {
        if let Some(name) = &self.camera_name
            && let Some(position) = cameras.iter().position(|camera| camera.human_name() == *name)
        {
            return Some(position);
        }
        (self.camera_index < cameras.len()).then_some(self.camera_index)
    }
}

struct Parsed {
    settings: Settings,
    problem: Option<String>,
}

/// Decodes only enough of the image to reject missing, corrupt or absurd backgrounds.
pub fn validate_background(path: &Path) -> Result<(), String> {
    let reader = image::ImageReader::open(path)
        .map_err(|why| format!("cannot open {}: {why}", path.display()))?
        .with_guessed_format()
        .map_err(|why| format!("cannot read {}: {why}", path.display()))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|why| format!("cannot decode {}: {why}", path.display()))?;
    const MAX_DIMENSION: u32 = 16_384;
    const MAX_PIXELS: u64 = 100_000_000;
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(format!(
            "{} is {}x{}; refusing an absurd background size",
            path.display(),
            width,
            height
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Mode, Settings};
    use std::path::PathBuf;

    #[test]
    fn defaults_are_off_with_no_background() {
        let settings = Settings::default();
        assert_eq!(settings.version, 1);
        assert_eq!(settings.camera_index, 0);
        assert_eq!(settings.mode, Mode::Off);
        assert_eq!(settings.blur, 50);
        assert_eq!(settings.background, None);
    }

    #[test]
    fn round_trip_keeps_every_field() {
        let settings = Settings {
            version: 1,
            camera_name: Some("HD Webcam".to_string()),
            camera_index: 2,
            mode: Mode::Image,
            blur: 70,
            background: Some(PathBuf::from(r"C:\Pictures\bg.png")),
        };
        let parsed = Settings::parse(&settings.to_json());
        assert_eq!(parsed.settings, settings);
        assert!(parsed.problem.is_none());
    }

    #[test]
    fn corrupt_json_falls_back_to_defaults() {
        let parsed = Settings::parse("{not json");
        assert_eq!(parsed.settings, Settings::default());
        assert!(parsed.problem.is_some());
    }

    #[test]
    fn invalid_fields_keep_the_valid_ones() {
        let parsed = Settings::parse(
            r#"{"camera_name":"Cam","camera_index":"bad","mode":"image","blur":900,"background":"C:\\bg.png"}"#,
        );
        assert_eq!(parsed.settings.camera_name.as_deref(), Some("Cam"));
        assert_eq!(parsed.settings.camera_index, 0);
        assert_eq!(parsed.settings.mode, Mode::Image);
        assert_eq!(parsed.settings.blur, 50);
        assert_eq!(
            parsed.settings.background.as_deref(),
            Some(std::path::Path::new(r"C:\bg.png"))
        );
        assert!(parsed.problem.is_some());
    }
}
