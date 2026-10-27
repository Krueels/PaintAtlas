use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorStyle {
    #[default]
    Brush,
    Circle,
    Dot,
}
impl CursorStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Brush => "Brush Shape",
            Self::Circle => "Circle",
            Self::Dot => "Dot",
        }
    }
}

#[derive(Clone)]
pub struct Gpu {
    pub id: String,
    pub name: String,
}
#[cfg(target_os = "linux")]
fn discover_gpus() -> Vec<Gpu> {
    let mut devices = vec![Gpu {
        id: String::new(),
        name: "Automatic (system)".into(),
    }];
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if !file.starts_with("card") || !file[4..].chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let Ok(path) = std::fs::canonicalize(entry.path().join("device")) else {
                continue;
            };
            let Some(pci) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if pci.len() != 12 || !pci.contains(':') {
                continue;
            }
            let driver = std::fs::read_link(path.join("driver")).ok();
            let proprietary = driver
                .as_ref()
                .and_then(|p| p.file_name())
                .is_some_and(|s| s == "nvidia");
            let id = if proprietary {
                "nvidia".into()
            } else {
                format!("pci-{}", pci.replace([':', '.'], "_"))
            };
            if devices.iter().any(|g| g.id == id) {
                continue;
            }
            let name = std::process::Command::new("lspci")
                .args(["-s", pci])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("GPU {pci}"));
            devices.push(Gpu { id, name });
        }
    }
    devices[1..].sort_by(|a, b| a.id.cmp(&b.id));
    devices
}
#[cfg(not(target_os = "linux"))]
fn discover_gpus() -> Vec<Gpu> {
    vec![Gpu {
        id: String::new(),
        name: "Automatic (system)".into(),
    }]
}

/// Select the driver before any window, GL context or background thread exists.
/// Re-exec avoids changing the process environment after threads have started.
#[cfg(target_os = "linux")]
pub fn configure_gpu() {
    use std::os::unix::process::CommandExt;
    let settings = crate::presets::settings_path()
        .and_then(|p| crate::presets::Settings::load(&p).ok())
        .unwrap_or_default();
    let id = settings.render_gpu;
    if id.is_empty() || std::env::var("PAINTATLAS_GPU_APPLIED").ok().as_deref() == Some(&id) {
        return;
    }
    if !discover_gpus().iter().any(|g| g.id == id) {
        eprintln!("The selected GPU is unavailable; using automatic selection.");
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut command = std::process::Command::new(exe);
    command
        .args(std::env::args_os().skip(1))
        .env("PAINTATLAS_GPU_APPLIED", &id);
    if id == "nvidia" {
        command
            .env_remove("DRI_PRIME")
            .env("__NV_PRIME_RENDER_OFFLOAD", "1")
            .env("__GLX_VENDOR_LIBRARY_NAME", "nvidia");
    } else {
        command
            .env("DRI_PRIME", &id)
            .env_remove("__NV_PRIME_RENDER_OFFLOAD")
            .env_remove("__NV_PRIME_RENDER_OFFLOAD_PROVIDER")
            .env_remove("__GLX_VENDOR_LIBRARY_NAME");
    }
    eprintln!("Could not start with the selected GPU: {}", command.exec());
}
#[cfg(not(target_os = "linux"))]
pub fn configure_gpu() {}

pub struct Preferences {
    shortcuts: crate::shortcut_settings::Editor,
    tab: usize,
    devices: Vec<Gpu>,
    pub renderer: String,
    startup_gpu: String,
    startup_backend: crate::presets::RenderBackend,
    update_receiver: Option<std::sync::mpsc::Receiver<Result<UpdateInfo, String>>>,
    update_status: UpdateStatus,
}
#[derive(Clone)]
struct UpdateInfo {
    version: String,
    url: String,
}
enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Failed(String),
}
impl Preferences {
    pub fn new(startup_gpu: String, startup_backend: crate::presets::RenderBackend) -> Self {
        Self {
            shortcuts: Default::default(),
            tab: 0,
            devices: discover_gpus(),
            renderer: "Not available in this context".into(),
            startup_gpu,
            startup_backend,
            update_receiver: None,
            update_status: UpdateStatus::Idle,
        }
    }
    pub fn start_update_check(&mut self) {
        if matches!(self.update_status, UpdateStatus::Checking) {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.update_receiver = Some(receiver);
        self.update_status = UpdateStatus::Checking;
        std::thread::spawn(move || {
            let result = check_latest_release();
            let _ = sender.send(result);
        });
    }
    pub fn cancel_shortcut_capture(&mut self) {
        self.shortcuts.cancel_capture();
    }
    pub fn capturing_shortcut(&self) -> bool {
        self.shortcuts.capturing()
    }
    pub fn shortcuts_tab(&mut self) {
        self.tab = 4;
    }
    pub fn appearance_tab(&mut self) {
        self.tab = 3;
    }
    pub fn show(&mut self, ui: &mut egui::Ui, settings: &mut crate::presets::Settings) -> bool {
        let mut changed = false;
        if let Some(receiver) = &self.update_receiver
            && let Ok(result) = receiver.try_recv()
        {
            self.update_status = match result {
                Ok(info) if is_newer_version(&info.version, env!("CARGO_PKG_VERSION")) => {
                    UpdateStatus::Available(info)
                }
                Ok(_) => UpdateStatus::UpToDate,
                Err(error) => UpdateStatus::Failed(error),
            };
            self.update_receiver = None;
        }
        let language = settings.language;
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(language, "settings.language"));
            egui::ComboBox::from_id_salt("application_language")
                .selected_text(settings.language.native_name())
                .show_ui(ui, |ui| {
                    for option in crate::i18n::Language::ALL {
                        let name = match (language, option) {
                            (
                                crate::i18n::Language::PortugueseBrazil,
                                crate::i18n::Language::English,
                            ) => crate::i18n::text(language, "settings.english"),
                            (
                                crate::i18n::Language::English,
                                crate::i18n::Language::PortugueseBrazil,
                            ) => crate::i18n::text(language, "settings.portuguese_brazil"),
                            _ => option.native_name(),
                        };
                        changed |= ui
                            .selectable_value(&mut settings.language, option, name)
                            .changed();
                    }
                });
        });
        crate::theme::caption(ui, crate::i18n::text(language, "settings.language_hint"));
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            for (i, key) in [
                "settings.cursor",
                "settings.rendering",
                "settings.colors",
                "settings.appearance",
                "settings.shortcuts",
                "settings.updates",
                "settings.drawing",
            ]
            .iter()
            .enumerate()
            {
                ui.selectable_value(&mut self.tab, i, crate::i18n::text(language, key));
            }
        });
        ui.separator();
        ui.scope(|ui| match self.tab {
                4 => { changed |= self.shortcuts.show(ui, &mut settings.shortcuts); }

                6 => {
                    egui::CollapsingHeader::new(crate::i18n::ui(ui.ctx(), "QuickShape")).default_open(true).show(ui, |ui| {
                        changed |= ui.checkbox(&mut settings.quickshape.enabled, crate::i18n::text(language, "quickshape.enabled")).changed();
                        changed |= ui.add(egui::Slider::new(&mut settings.quickshape.delay, 0.2..=2.0).text(crate::i18n::text(language, "quickshape.delay")).suffix(" s")).changed();
                        crate::theme::caption(ui, crate::i18n::text(language, "quickshape.hint").replace("{}", &settings.shortcuts.label(crate::shortcuts::Action::QuickShapeRegular)));
                    });
                }

                5 => {
                    changed |= crate::theme::toggle(
                        ui,
                        &mut settings.check_for_updates,
                        &crate::i18n::ui(ui.ctx(), "Check for updates automatically"),
                    )
                    .changed();
                    ui.horizontal(|ui| {
                        if crate::theme::button(
                            ui,
                            "Check now",
                        )
                        .clicked()
                        {
                            self.start_update_check();
                        }
                        match &self.update_status {
                            UpdateStatus::Idle => {}
                            UpdateStatus::Checking => {
                                ui.spinner();
                                ui.label(crate::i18n::ui(ui.ctx(), "Checking GitHub releases…"));
                            }
                            UpdateStatus::UpToDate => {
                                ui.label(crate::i18n::ui(ui.ctx(), "PaintAtlas is up to date."));
                            }
                            UpdateStatus::Available(info) => {
                                ui.label(crate::i18n::ui(ui.ctx(), format!(
                                    "{} {}",
                                    "Version available:",
                                    info.version.trim_start_matches('v')
                                )));
                                ui.hyperlink_to(
                                    "Open release",
                                    &info.url,
                                );
                            }
                            UpdateStatus::Failed(error) => {
                                ui.colored_label(egui::Color32::LIGHT_RED, crate::i18n::ui(ui.ctx(), error));
                            }
                        }
                    });
                    crate::theme::caption(
                        ui,
                        crate::i18n::ui(ui.ctx(), if language == crate::i18n::Language::English {
                            "Checks the official PaintAtlas GitHub releases. You can disable automatic checks at any time."
                        } else {
                            "Checks official PaintAtlas releases on GitHub. You can disable automatic checks at any time."
                        }),
                    );
                }

                0 => {
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.brush_eraser")).default_open(true).show(ui, |ui| {
                        ui.label(crate::i18n::text(language, "settings.cursor_type"));
                        for style in [CursorStyle::Brush, CursorStyle::Circle, CursorStyle::Dot] {
                            changed |= ui.radio_value(&mut settings.cursor_style, style, style.label()).changed();
                        }
                        if settings.cursor_style == CursorStyle::Circle {
                            changed |= crate::theme::toggle(ui, &mut settings.circle_cursor_pressure, &crate::i18n::ui(ui.ctx(), "Resize the circle with pen pressure")).changed();
                            crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), "Disabled: the circle shows the configured brush size without varying with pressure."));
                        }
                        crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), "Brush Shape follows pen pressure. The dot keeps a fixed screen size. These options affect only the cursor."));
                    });
                }
                1 => {
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.graphics_device")).default_open(true).show(ui, |ui| {
                        ui.label(crate::i18n::text(language, "settings.render_backend"));
                        egui::ComboBox::from_id_salt("render_backend")
                            .selected_text(crate::i18n::ui(ui.ctx(), match settings.render_backend {
                                crate::presets::RenderBackend::OpenGl => "OpenGL",
                                crate::presets::RenderBackend::Vulkan => "Vulkan",
                            }))
                            .show_ui(ui, |ui| {
                                changed |= ui.selectable_value(
                                    &mut settings.render_backend,
                                    crate::presets::RenderBackend::OpenGl,
                                    crate::i18n::ui(ui.ctx(), "OpenGL"),
                                ).changed();
                                changed |= ui.selectable_value(
                                    &mut settings.render_backend,
                                    crate::presets::RenderBackend::Vulkan,
                                    crate::i18n::ui(ui.ctx(), "Vulkan"),
                                ).changed();
                            });
                        if settings.render_backend != self.startup_backend {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                crate::i18n::text(language, "settings.restart_backend"),
                            );
                        }
                        crate::theme::caption(ui, crate::i18n::text(language, "settings.vulkan_note"));
                        ui.label(crate::i18n::text(language, "settings.gpu_opengl"));
                        let selected = self.devices.iter().find(|g| g.id == settings.render_gpu).map(|g| g.name.as_str()).unwrap_or("Saved GPU unavailable");
                        egui::ComboBox::from_id_salt("render_gpu").selected_text(selected).truncate().width(ui.available_width().min(460.0)).show_ui(ui, |ui| {
                            for gpu in &self.devices {
                                changed |= ui.selectable_value(&mut settings.render_gpu, gpu.id.clone(), &gpu.name).changed();
                            }
                        });
                        if crate::theme::button(ui, "Refresh GPU list").clicked() { self.devices = discover_gpus(); }
                        crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), "The choice takes effect after restarting the app. The canvas and interface share the same OpenGL context."));
                        crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), "Mesa: select by PCI address. Proprietary NVIDIA: PRIME offload; the driver chooses the available NVIDIA GPU."));
                        if settings.render_gpu != self.startup_gpu { ui.colored_label(egui::Color32::YELLOW, crate::i18n::ui(ui.ctx(), "Restart the application to apply this change.")); }
                    });
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.diagnostics")).default_open(true).show(ui, |ui| {
                        ui.label(crate::i18n::text(language, "settings.active_renderer"));
                        ui.label(&self.renderer);
                        crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), "Selection depends on PRIME support and the graphics drivers installed on Linux."));
                    });
                }
                2 => {
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.color_selector")).default_open(true).show(ui, |ui| {
                        changed |= ui.radio_value(&mut settings.triangular_color_selector, false, "Rectangular").changed();
                        changed |= ui.radio_value(&mut settings.triangular_color_selector, true, "Triangular").changed();
                    });
                }
                _ => {
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.colors")).default_open(true).show(ui, |ui| {
                        ui.label(crate::i18n::text(language, "settings.accent_color"));
                        ui.horizontal_wrapped(|ui| {
                            for accent in crate::theme::Accent::ALL {
                                changed |= ui.selectable_value(&mut settings.appearance.accent, accent,
                                    egui::RichText::new(accent.name()).color(accent.color())).changed();
                            }
                        });
                        ui.label(crate::i18n::text(language, "settings.glass_intensity"));
                        let mut percent = crate::theme::glass_strength(settings.appearance.transparency) * 100.0;
                        if ui.add(egui::Slider::new(&mut percent, 35.0..=85.0).suffix(" %")).changed() {
                            settings.appearance.transparency = percent / 100.0;
                            changed = true;
                        }
                        crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), "All panels use transparency and blur over the canvas. Adjusts the effect strength; image colors are not changed."));
                    });
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.motion")).default_open(true).show(ui, |ui| {
                        changed |= crate::theme::toggle(ui, &mut settings.appearance.reduced_motion, crate::i18n::text(language, "settings.reduce_motion")).changed();
                        crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), "Disables pulsing, elasticity, waves, and transitions. The dragged layer follows the pointer directly."));
                    });
                    if crate::theme::button(ui, crate::i18n::text(language, "settings.restore_appearance")).clicked() {
                        settings.appearance = Default::default();
                        changed = true;
                    }
                }
            });
        ui.separator();
        crate::theme::caption(ui, crate::i18n::text(language, "settings.saved_hint"));
        changed
    }
}

fn check_latest_release() -> Result<UpdateInfo, String> {
    let mut command = std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "15",
            "--user-agent",
            concat!("PaintAtlas/", env!("CARGO_PKG_VERSION")),
            "https://api.github.com/repos/Krueels/PaintAtlas/releases/latest",
        ])
        .output()
        .map_err(|error| format!("Could not start curl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "GitHub returned an error: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let release: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid response from GitHub: {error}"))?;
    let version = release
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "The GitHub release does not contain tag_name.".to_owned())?;
    let url = release
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.starts_with("https://github.com/Krueels/PaintAtlas/releases/"))
        .ok_or_else(|| "The GitHub release does not contain a valid link.".to_owned())?;
    Ok(UpdateInfo {
        version: version.to_owned(),
        url: url.to_owned(),
    })
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    fn components(version: &str) -> Option<Vec<u64>> {
        version
            .trim_start_matches('v')
            .split('.')
            .map(|part| part.parse().ok())
            .collect()
    }
    let (Some(latest), Some(current)) = (components(latest), components(current)) else {
        return false;
    };
    let len = latest.len().max(current.len());
    (0..len)
        .map(|index| {
            latest
                .get(index)
                .copied()
                .unwrap_or(0)
                .cmp(&current.get(index).copied().unwrap_or(0))
        })
        .find(|ordering| !ordering.is_eq())
        .is_some_and(|ordering| ordering.is_gt())
}

#[cfg(test)]
mod update_tests {
    use super::is_newer_version;

    #[test]
    fn compares_release_tags_to_the_installed_version() {
        assert!(is_newer_version("v0.2.0", "0.1.0"));
        assert!(!is_newer_version("v0.1.0", "0.1.0"));
        assert!(!is_newer_version("v0.0.9", "0.1.0"));
        assert!(is_newer_version("v1.0", "0.9.99"));
        assert!(!is_newer_version("draft", "0.1.0"));
    }
}
