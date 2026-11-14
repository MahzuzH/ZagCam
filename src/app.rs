//! The tray app: a tray icon, the settings window, and the engine worker it drives.

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use zagcam_vcam_mgr::VirtualCamera;

use crate::capture;
use crate::engine::Engine;
use crate::settings::{Mode, Quality, Settings, validate_background};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(model: PathBuf) -> Result<(), String> {
    let _instance = InstanceGuard::acquire()?;
    let settings = Settings::load();
    let camera = VirtualCamera::start()?;
    let engine = Engine::spawn(settings.clone(), model)?;

    let tray = Tray::build(&settings);
    let visible = true;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("ZagCam {VERSION}"))
            .with_inner_size([440.0, 380.0])
            .with_visible(visible),
        ..Default::default()
    };
    let result = eframe::run_native(
        "ZagCam",
        options,
        Box::new(move |cc| {
            let app = App::new(cc, settings, engine, tray, visible);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    );
    drop(camera);
    result.map_err(|why| format!("the settings window failed: {why}"))
}

struct App {
    engine: Engine,
    current: Settings,
    draft: Settings,
    last_effect: Mode,
    devices: Vec<String>,
    device_index: usize,
    saved_device_index: usize,
    window_visible: bool,
    allow_exit: bool,
    message: Option<String>,
    error: Option<String>,
    tray: Option<Tray>,
}

impl App {
    fn new(
        _cc: &eframe::CreationContext<'_>,
        settings: Settings,
        engine: Engine,
        tray: Option<Tray>,
        window_visible: bool,
    ) -> Self {
        let cameras = capture::list().unwrap_or_default();
        let device_index = settings.camera_index(&cameras).unwrap_or(0);
        let last_effect = match settings.mode {
            Mode::Off => Mode::Blur,
            mode => mode,
        };
        Self {
            engine,
            draft: settings.clone(),
            current: settings,
            last_effect,
            devices: cameras.iter().map(|camera| camera.human_name()).collect(),
            device_index,
            saved_device_index: device_index,
            window_visible,
            allow_exit: false,
            message: None,
            error: None,
            tray,
        }
    }

    fn refresh_devices(&mut self) {
        let cameras = capture::list().unwrap_or_default();
        self.devices = cameras.iter().map(|camera| camera.human_name()).collect();
        let index = self.current.camera_index(&cameras).unwrap_or(0);
        self.saved_device_index = index.min(self.devices.len().saturating_sub(1));
        self.device_index = self.saved_device_index;
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.refresh_devices();
        self.window_visible = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn commit(&mut self, settings: Settings) -> bool {
        let saved = match settings.save() {
            Ok(()) => {
                self.current = settings.clone();
                self.draft = settings.clone();
                self.engine.send(settings);
                self.error = None;
                self.message = Some("Saved".to_string());
                true
            }
            Err(why) => {
                self.error = Some(format!("cannot save settings: {why}"));
                false
            }
        };
        if let Some(tray) = &self.tray {
            tray.sync(&self.current);
        }
        saved
    }

    fn set_mode(&mut self, ctx: &egui::Context, mode: Mode) {
        let mut next = self.current.clone();
        next.mode = mode;
        if mode == Mode::Off {
            self.last_effect = self.current.mode;
        } else {
            self.last_effect = mode;
            if mode == Mode::Image {
                let valid = next
                    .background
                    .as_deref()
                    .is_some_and(|path| validate_background(path).is_ok());
                if !valid {
                    self.error = Some("Choose a valid background image first".to_string());
                    self.show_window(ctx);
                    if let Some(tray) = &self.tray {
                        tray.sync(&self.current);
                    }
                    return;
                }
            }
        }
        self.commit(next);
    }

    fn toggle(&mut self, ctx: &egui::Context) {
        let mode = if self.current.mode == Mode::Off {
            self.last_effect
        } else {
            Mode::Off
        };
        self.set_mode(ctx, mode);
    }

    fn save_draft(&mut self) {
        let mut next = self.draft.clone();
        next.camera_name = self.devices.get(self.device_index).cloned();
        next.camera_index = self.device_index;
        if next.mode == Mode::Image {
            match next.background.as_deref() {
                Some(path) => {
                    if let Err(why) = validate_background(path) {
                        self.error = Some(why);
                        return;
                    }
                }
                None => {
                    self.error = Some("image mode needs a background image".to_string());
                    return;
                }
            }
        }
        if self.commit(next) {
            self.saved_device_index = self.device_index;
        }
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.0.as_str() {
                "toggle" => self.toggle(ctx),
                "off" => self.set_mode(ctx, Mode::Off),
                "blur" => self.set_mode(ctx, Mode::Blur),
                "image" => self.set_mode(ctx, Mode::Image),
                "open" => self.show_window(ctx),
                "quit" => {
                    self.allow_exit = true;
                    self.engine.shutdown();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!("ZagCam {VERSION}"));
        ui.label("Apps see ZagCam Camera; the webcam opens only while an app reads it.");
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Camera");
            let selected = self
                .devices
                .get(self.device_index)
                .cloned()
                .unwrap_or_else(|| "none found".to_string());
            egui::ComboBox::from_id_salt("camera").selected_text(selected).show_ui(
                ui,
                |ui| {
                    for (index, name) in self.devices.iter().enumerate() {
                        if ui.selectable_value(&mut self.device_index, index, name).clicked() {
                            self.message = None;
                        }
                    }
                },
            );
            if ui.button("Refresh").clicked() {
                self.refresh_devices();
                self.message = None;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Mode");
            for (mode, label) in [(Mode::Off, "Off"), (Mode::Blur, "Blur"), (Mode::Image, "Image")] {
                if ui.selectable_value(&mut self.draft.mode, mode, label).clicked() {
                    self.message = None;
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label("Quality");
            for (quality, label) in [
                (Quality::Eco, "Eco"),
                (Quality::Balanced, "Balanced"),
                (Quality::High, "Quality"),
            ] {
                if ui
                    .selectable_value(&mut self.draft.quality, quality, label)
                    .clicked()
                {
                    self.message = None;
                }
            }
        });

        let blur = ui.add_enabled(
            self.draft.mode == Mode::Blur,
            egui::Slider::new(&mut self.draft.blur, 0..=100).text("Blur intensity"),
        );
        if blur.changed() {
            self.message = None;
        }

        ui.horizontal(|ui| {
            let background = self
                .draft
                .background
                .as_ref()
                .map_or_else(|| "(none)".to_string(), |path| path.display().to_string());
            ui.label("Background");
            ui.label(background);
        });
        if ui.button("Choose image…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg"])
                .pick_file()
        {
            self.draft.background = Some(path);
            self.message = None;
        }

        ui.separator();
        let in_use = self.engine.camera_in_use();
        ui.label(if in_use {
            "Camera in use: yes"
        } else {
            "Camera in use: no"
        });

        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.save_draft();
            }
            if ui.button("Cancel").clicked() {
                self.draft = self.current.clone();
                self.device_index = self.saved_device_index;
                self.error = None;
                self.message = None;
            }
            if ui.button("Quit").clicked() {
                self.allow_exit = true;
                self.engine.shutdown();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        } else if let Some(error) = self.engine.last_error() {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        } else if let Some(message) = &self.message {
            ui.label(message);
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);
        if ctx.input(|input| input.viewport().close_requested()) {
            if self.allow_exit {
                return;
            }
            if self.tray.is_none() {
                self.allow_exit = true;
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.window_visible = false;
        }
        let interval = if self.window_visible { 150 } else { 1_000 };
        ctx.request_repaint_after(Duration::from_millis(interval));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.window_visible {
            return;
        }
        ui.add_space(10.0);
        self.settings_ui(ui);
    }
}

struct Tray {
    _icon: TrayIcon,
    enable: CheckMenuItem,
}

impl Tray {
    fn build(settings: &Settings) -> Option<Self> {
        let enable = CheckMenuItem::with_id(
            "toggle",
            "Enabled",
            true,
            settings.mode != Mode::Off,
            None,
        );
        let off = MenuItem::with_id("off", "Off", true, None);
        let blur = MenuItem::with_id("blur", "Blur", true, None);
        let image = MenuItem::with_id("image", "Image", true, None);
        let open = MenuItem::with_id("open", "Settings…", true, None);
        let quit = MenuItem::with_id("quit", "Quit", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &enable,
            &off,
            &blur,
            &image,
            &PredefinedMenuItem::separator(),
            &open,
            &quit,
        ])
        .ok()?;
        let icon = Icon::from_rgba(icon_rgba(), 16, 16).ok()?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("ZagCam")
            .with_icon(icon)
            .build()
            .map_err(|why| {
                eprintln!("zagcam: tray icon unavailable: {why}; showing the window instead");
                why
            })
            .ok()?;
        Some(Self {
            _icon: icon,
            enable,
        })
    }

    fn sync(&self, settings: &Settings) {
        self.enable.set_checked(settings.mode != Mode::Off);
    }
}

/// A 16x16 filled square, so the tray shows something without shipping an asset.
fn icon_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16u32 {
        for x in 0..16u32 {
            let inside = (3..13).contains(&x) && (3..13).contains(&y);
            if inside {
                rgba.extend_from_slice(&[0x59, 0x84, 0xE8, 0xFF]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    rgba
}

#[cfg(windows)]
struct InstanceGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl InstanceGuard {
    fn acquire() -> Result<Self, String> {
        Self::acquire_named("Local\\ZagCam-SingleInstance")
    }

    fn acquire_named(name: &str) -> Result<Self, String> {
        use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let mut name: Vec<u16> = name.encode_utf16().collect();
        name.push(0);
        let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
        if handle.is_null() {
            return Err(format!(
                "cannot create the single-instance lock: {}",
                std::io::Error::last_os_error()
            ));
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return Err("ZagCam is already running (look for it in the tray)".into());
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(not(windows))]
struct InstanceGuard;

#[cfg(not(windows))]
impl InstanceGuard {
    fn acquire() -> Result<Self, String> {
        Ok(Self)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::InstanceGuard;

    #[test]
    fn a_second_instance_is_rejected() {
        let name = format!("Local\\ZagCam-Test-{}", std::process::id());
        let _first = InstanceGuard::acquire_named(&name).expect("first instance");
        assert!(InstanceGuard::acquire_named(&name).is_err());
    }
}
