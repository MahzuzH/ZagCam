//! Persisted configuration in `%APPDATA%\ZagCam\settings.json`.

use std::fs;
use std::path::{Path, PathBuf};

pub const VERSION: u32 = 3;
pub const DEFAULT_BLUR: u8 = 50;
pub const MAX_PRESETS: usize = 32;
pub const MAX_ADJUST: i8 = 100;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quality {
    Eco,
    #[default]
    Balanced,
    High,
}

impl Quality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eco => "eco",
            Self::Balanced => "balanced",
            Self::High => "quality",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "eco" => Some(Self::Eco),
            "balanced" => Some(Self::Balanced),
            "quality" => Some(Self::High),
            _ => None,
        }
    }

    /// Run the model every N frames.
    pub fn infer_every(self) -> u64 {
        match self {
            Self::Eco => 4,
            Self::Balanced => 2,
            Self::High => 1,
        }
    }

    /// Each mask pixel covers a `mask_div` x `mask_div` block of the frame.
    pub fn mask_div(self) -> usize {
        match self {
            Self::Eco => 4,
            Self::Balanced | Self::High => 2,
        }
    }

    /// Requested capture size. Eco trades resolution for CPU; output is still scaled to 720p.
    pub fn capture_size(self) -> (usize, usize) {
        match self {
            Self::Eco => (640, 360),
            Self::Balanced | Self::High => (1280, 720),
        }
    }
}

/// A named copy of the effect fields, applied without touching camera/quality globals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub name: String,
    pub mode: Mode,
    pub blur: u8,
    pub background: Option<PathBuf>,
    pub quality: Quality,
    pub mirror: bool,
    pub flip: bool,
    pub brightness: i8,
    pub contrast: i8,
}

impl Preset {
    pub fn capture(name: String, settings: &Settings) -> Self {
        Self {
            name,
            mode: settings.mode,
            blur: settings.blur,
            background: settings.background.clone(),
            quality: settings.quality,
            mirror: settings.mirror,
            flip: settings.flip,
            brightness: settings.brightness,
            contrast: settings.contrast,
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
    pub quality: Quality,
    pub mirror: bool,
    pub flip: bool,
    pub brightness: i8,
    pub contrast: i8,
    pub presets: Vec<Preset>,
    pub auto_start: bool,
    pub hotkeys: bool,
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
            quality: Quality::default(),
            mirror: false,
            flip: false,
            brightness: 0,
            contrast: 0,
            presets: Vec::new(),
            auto_start: false,
            hotkeys: false,
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
        serde_json::json!({
            "version": self.version,
            "camera_name": self.camera_name,
            "camera_index": self.camera_index,
            "mode": self.mode.as_str(),
            "blur": self.blur,
            "background": path_json(self.background.as_deref()),
            "quality": self.quality.as_str(),
            "mirror": self.mirror,
            "flip": self.flip,
            "brightness": self.brightness,
            "contrast": self.contrast,
            "presets": self.presets.iter().map(preset_json).collect::<Vec<_>>(),
            "auto_start": self.auto_start,
            "hotkeys": self.hotkeys,
        })
        .to_string()
    }

    /// Applies a preset's effect fields; camera and global options are untouched.
    pub fn apply_preset(&mut self, preset: &Preset) {
        self.mode = preset.mode;
        self.blur = preset.blur;
        self.background = preset.background.clone();
        self.quality = preset.quality;
        self.mirror = preset.mirror;
        self.flip = preset.flip;
        self.brightness = preset.brightness;
        self.contrast = preset.contrast;
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
                Some(1..=3) => settings.version = VERSION,
                _ => problem = true,
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
                Some(blur) if blur <= 100 => settings.blur = blur as u8,
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
        if let Some(value) = map.get("quality") {
            match value.as_str().and_then(Quality::parse) {
                Some(quality) => settings.quality = quality,
                None => problem = true,
            }
        }
        problem |= parse_bool(&map, "mirror", &mut settings.mirror);
        problem |= parse_bool(&map, "flip", &mut settings.flip);
        problem |= parse_adjust(&map, "brightness", &mut settings.brightness);
        problem |= parse_adjust(&map, "contrast", &mut settings.contrast);
        problem |= parse_bool(&map, "auto_start", &mut settings.auto_start);
        problem |= parse_bool(&map, "hotkeys", &mut settings.hotkeys);
        if let Some(value) = map.get("presets") {
            match value.as_array() {
                Some(items) => {
                    for item in items.iter().take(MAX_PRESETS) {
                        match parse_preset(item) {
                            Some(preset) => settings.presets.push(preset),
                            None => problem = true,
                        }
                    }
                }
                None => problem = true,
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

fn path_json(path: Option<&Path>) -> serde_json::Value {
    match path {
        Some(path) => serde_json::Value::String(path.display().to_string()),
        None => serde_json::Value::Null,
    }
}

fn preset_json(preset: &Preset) -> serde_json::Value {
    serde_json::json!({
        "name": preset.name,
        "mode": preset.mode.as_str(),
        "blur": preset.blur,
        "background": path_json(preset.background.as_deref()),
        "quality": preset.quality.as_str(),
        "mirror": preset.mirror,
        "flip": preset.flip,
        "brightness": preset.brightness,
        "contrast": preset.contrast,
    })
}

fn parse_bool(map: &serde_json::Map<String, serde_json::Value>, key: &str, out: &mut bool) -> bool {
    match map.get(key) {
        None => false,
        Some(value) => match value.as_bool() {
            Some(value) => {
                *out = value;
                false
            }
            None => true,
        },
    }
}

fn parse_adjust(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    out: &mut i8,
) -> bool {
    match map.get(key) {
        None => false,
        Some(value) => match value.as_i64() {
            Some(value) if (-i64::from(MAX_ADJUST)..=i64::from(MAX_ADJUST)).contains(&value) => {
                *out = value as i8;
                false
            }
            _ => true,
        },
    }
}

fn parse_preset(value: &serde_json::Value) -> Option<Preset> {
    let map = value.as_object()?;
    let name = map.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let mode = map.get("mode")?.as_str().and_then(Mode::parse)?;
    let blur = map.get("blur")?.as_u64().filter(|blur| *blur <= 100)? as u8;
    let background = match map.get("background") {
        Some(serde_json::Value::Null) | None => None,
        Some(serde_json::Value::String(path)) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => return None,
    };
    let quality = map.get("quality")?.as_str().and_then(Quality::parse)?;
    let mut preset = Preset {
        name: name.to_string(),
        mode,
        blur,
        background,
        quality,
        mirror: false,
        flip: false,
        brightness: 0,
        contrast: 0,
    };
    if parse_bool(map, "mirror", &mut preset.mirror)
        || parse_bool(map, "flip", &mut preset.flip)
        || parse_adjust(map, "brightness", &mut preset.brightness)
        || parse_adjust(map, "contrast", &mut preset.contrast)
    {
        return None;
    }
    Some(preset)
}

struct Parsed {
    settings: Settings,
    problem: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundKind {
    Still,
    Gif,
    Video,
}

/// Validates the file and returns how the pipeline should decode it.
pub fn validate_background(path: &Path) -> Result<BackgroundKind, String> {
    if !path.is_file() {
        return Err(format!("{} does not exist", path.display()));
    }
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let kind = match extension.as_str() {
        "gif" => BackgroundKind::Gif,
        "mp4" | "m4v" | "mov" | "avi" | "wmv" | "mkv" => BackgroundKind::Video,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" => BackgroundKind::Still,
        _ => {
            return Err(format!(
                "{} is not a supported image, GIF, or video",
                path.display()
            ));
        }
    };
    if kind == BackgroundKind::Video {
        let size = fs::metadata(path)
            .map_err(|why| format!("cannot read {}: {why}", path.display()))?
            .len();
        if size == 0 {
            return Err(format!("{} is empty", path.display()));
        }
        return Ok(kind);
    }

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
    Ok(kind)
}

#[cfg(test)]
mod tests {
    use super::{MAX_PRESETS, Mode, Preset, Quality, Settings, VERSION, validate_background};
    use std::path::PathBuf;

    fn sample_preset(name: &str) -> Preset {
        Preset {
            name: name.to_string(),
            mode: Mode::Image,
            blur: 70,
            background: Some(PathBuf::from(r"C:\Pictures\bg.gif")),
            quality: Quality::High,
            mirror: true,
            flip: false,
            brightness: -12,
            contrast: 20,
        }
    }

    #[test]
    fn defaults_are_off_with_no_background() {
        let settings = Settings::default();
        assert_eq!(settings.version, VERSION);
        assert_eq!(settings.camera_index, 0);
        assert_eq!(settings.mode, Mode::Off);
        assert_eq!(settings.blur, 50);
        assert_eq!(settings.background, None);
        assert_eq!(settings.quality, Quality::Balanced);
        assert!(!settings.mirror);
        assert!(!settings.flip);
        assert_eq!(settings.brightness, 0);
        assert_eq!(settings.contrast, 0);
        assert!(settings.presets.is_empty());
        assert!(!settings.auto_start);
        assert!(!settings.hotkeys);
    }

    #[test]
    fn round_trip_keeps_every_field() {
        let settings = Settings {
            version: VERSION,
            camera_name: Some("HD Webcam".to_string()),
            camera_index: 2,
            mode: Mode::Image,
            blur: 70,
            background: Some(PathBuf::from(r"C:\Pictures\bg.png")),
            quality: Quality::High,
            mirror: true,
            flip: true,
            brightness: -30,
            contrast: 40,
            presets: vec![sample_preset("Office")],
            auto_start: true,
            hotkeys: true,
        };
        let parsed = Settings::parse(&settings.to_json());
        assert_eq!(parsed.settings, settings);
        assert!(parsed.problem.is_none());
    }

    #[test]
    fn old_settings_get_new_defaults() {
        let parsed = Settings::parse(r#"{"version":2,"mode":"blur","blur":60}"#);
        assert_eq!(parsed.settings.version, VERSION);
        assert_eq!(parsed.settings.mode, Mode::Blur);
        assert_eq!(parsed.settings.quality, Quality::Balanced);
        assert!(parsed.settings.presets.is_empty());
        assert!(!parsed.settings.hotkeys);
        assert!(parsed.problem.is_none());
    }

    #[test]
    fn corrupt_json_falls_back_to_defaults() {
        let parsed = Settings::parse("{not json");
        assert_eq!(parsed.settings, Settings::default());
        assert!(parsed.problem.is_some());
    }

    #[test]
    fn out_of_range_blur_and_unknown_version_use_defaults() {
        let parsed = Settings::parse(r#"{"version":99,"blur":101,"mode":"blur"}"#);
        assert_eq!(parsed.settings.version, VERSION);
        assert_eq!(parsed.settings.blur, 50);
        assert_eq!(parsed.settings.mode, Mode::Blur);
        assert!(parsed.problem.is_some());
    }

    #[test]
    fn invalid_quality_falls_back_to_balanced() {
        let parsed = Settings::parse(r#"{"version":3,"quality":"ultra"}"#);
        assert_eq!(parsed.settings.quality, Quality::Balanced);
        assert!(parsed.problem.is_some());
    }

    #[test]
    fn invalid_preset_is_skipped_and_kept_valid_ones() {
        let parsed = Settings::parse(
            r#"{"version":3,"presets":[
                {"name":"Good","mode":"blur","blur":40,"quality":"eco"},
                {"name":"Bad","mode":"ultra","blur":40,"quality":"eco"},
                {"name":"","mode":"blur","blur":40,"quality":"eco"}
            ]}"#,
        );
        assert_eq!(parsed.settings.presets.len(), 1);
        assert_eq!(parsed.settings.presets[0].name, "Good");
        assert!(parsed.problem.is_some());
    }

    #[test]
    fn preset_capture_and_apply_round_trip_the_effect() {
        let mut settings = Settings {
            mode: Mode::Image,
            blur: 80,
            background: Some(PathBuf::from(r"C:\Pictures\bg.mp4")),
            mirror: true,
            brightness: -5,
            ..Settings::default()
        };
        let preset = Preset::capture("Movie".to_string(), &settings);
        settings.mode = Mode::Off;
        settings.blur = 0;
        settings.background = None;
        settings.mirror = false;
        settings.brightness = 0;
        settings.apply_preset(&preset);
        assert_eq!(settings.mode, Mode::Image);
        assert_eq!(settings.blur, 80);
        assert!(settings.mirror);
        assert_eq!(settings.brightness, -5);
    }

    #[test]
    fn preset_count_is_capped() {
        let presets = (0..MAX_PRESETS + 5)
            .map(|index| format!(r#"{{"name":"P{index}","mode":"blur","blur":40,"quality":"eco"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let parsed = Settings::parse(&format!(r#"{{"version":3,"presets":[{presets}]}}"#));
        assert_eq!(parsed.settings.presets.len(), MAX_PRESETS);
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

    #[test]
    fn background_kind_uses_the_extension() {
        let path = std::env::temp_dir().join(format!("zagcam-kind-{}.gif", std::process::id()));
        let gif: &[u8] = &[
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x21, 0xF9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00,
            0x3B,
        ];
        std::fs::write(&path, gif).expect("write gif");
        assert_eq!(validate_background(&path), Ok(super::BackgroundKind::Gif));
        let _ = std::fs::remove_file(&path);
        let video = std::env::temp_dir().join("zagcam-kind.mp4");
        std::fs::write(&video, b"not really a video").expect("write video placeholder");
        assert_eq!(validate_background(&video), Ok(super::BackgroundKind::Video));
        let _ = std::fs::remove_file(&video);
    }
}
