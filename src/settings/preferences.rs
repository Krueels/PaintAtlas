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
            Self::Circle => "Círculo",
            Self::Dot => "Ponto",
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
        name: "Automática (sistema)".into(),
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
        name: "Automática (sistema)".into(),
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
        eprintln!("GPU configurada indisponível; usando seleção automática.");
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
    eprintln!(
        "Não foi possível iniciar com a GPU selecionada: {}",
        command.exec()
    );
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
            renderer: "Não disponível neste contexto".into(),
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

                5 => {
                    changed |= crate::theme::toggle(
                        ui,
                        &mut settings.check_for_updates,
                        if language == crate::i18n::Language::English {
                            "Check for updates automatically"
                        } else {
                            "Verificar atualizações automaticamente"
                        },
                    )
                    .changed();
                    ui.horizontal(|ui| {
                        if crate::theme::button(
                            ui,
                            if language == crate::i18n::Language::English {
                                "Check now"
                            } else {
                                "Verificar agora"
                            },
                        )
                        .clicked()
                        {
                            self.start_update_check();
                        }
                        match &self.update_status {
                            UpdateStatus::Idle => {}
                            UpdateStatus::Checking => {
                                ui.spinner();
                                ui.label(if language == crate::i18n::Language::English {
                                    "Checking GitHub releases…"
                                } else {
                                    "Verificando releases do GitHub…"
                                });
                            }
                            UpdateStatus::UpToDate => {
                                ui.label(if language == crate::i18n::Language::English {
                                    "PaintAtlas is up to date."
                                } else {
                                    "O PaintAtlas está atualizado."
                                });
                            }
                            UpdateStatus::Available(info) => {
                                ui.label(format!(
                                    "{} {}",
                                    if language == crate::i18n::Language::English {
                                        "Version available:"
                                    } else {
                                        "Versão disponível:"
                                    },
                                    info.version.trim_start_matches('v')
                                ));
                                ui.hyperlink_to(
                                    if language == crate::i18n::Language::English {
                                        "Open release"
                                    } else {
                                        "Abrir release"
                                    },
                                    &info.url,
                                );
                            }
                            UpdateStatus::Failed(error) => {
                                ui.colored_label(egui::Color32::LIGHT_RED, error);
                            }
                        }
                    });
                    crate::theme::caption(
                        ui,
                        if language == crate::i18n::Language::English {
                            "Checks the official PaintAtlas GitHub releases. You can disable automatic checks at any time."
                        } else {
                            "Consulta as releases oficiais do PaintAtlas no GitHub. Você pode desativar a verificação automática quando quiser."
                        },
                    );
                }

                0 => {
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.brush_eraser")).default_open(true).show(ui, |ui| {
                        ui.label(crate::i18n::text(language, "settings.cursor_type"));
                        for style in [CursorStyle::Brush, CursorStyle::Circle, CursorStyle::Dot] {
                            changed |= ui.radio_value(&mut settings.cursor_style, style, style.label()).changed();
                        }
                        if settings.cursor_style == CursorStyle::Circle {
                            changed |= crate::theme::toggle(ui, &mut settings.circle_cursor_pressure, "Redimensionar círculo com a pressão da caneta").changed();
                            crate::theme::caption(ui, "Desativado: o círculo mostra o tamanho configurado do pincel, sem variar com a pressão.");
                        }
                        crate::theme::caption(ui, "Brush Shape acompanha a pressão. O ponto mantém um tamanho fixo na tela. Essas opções alteram apenas o cursor.");
                    });
                }
                1 => {
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.graphics_device")).default_open(true).show(ui, |ui| {
                        ui.label(crate::i18n::text(language, "settings.render_backend"));
                        egui::ComboBox::from_id_salt("render_backend")
                            .selected_text(match settings.render_backend {
                                crate::presets::RenderBackend::OpenGl => "OpenGL",
                                crate::presets::RenderBackend::Vulkan => "Vulkan",
                            })
                            .show_ui(ui, |ui| {
                                changed |= ui.selectable_value(
                                    &mut settings.render_backend,
                                    crate::presets::RenderBackend::OpenGl,
                                    "OpenGL",
                                ).changed();
                                changed |= ui.selectable_value(
                                    &mut settings.render_backend,
                                    crate::presets::RenderBackend::Vulkan,
                                    "Vulkan",
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
                        let selected = self.devices.iter().find(|g| g.id == settings.render_gpu).map(|g| g.name.as_str()).unwrap_or("GPU salva indisponível");
                        egui::ComboBox::from_id_salt("render_gpu").selected_text(selected).truncate().width(ui.available_width().min(460.0)).show_ui(ui, |ui| {
                            for gpu in &self.devices {
                                changed |= ui.selectable_value(&mut settings.render_gpu, gpu.id.clone(), &gpu.name).changed();
                            }
                        });
                        if crate::theme::button(ui, "Atualizar lista de GPUs").clicked() { self.devices = discover_gpus(); }
                        crate::theme::caption(ui, "A escolha entra em vigor ao fechar e abrir o aplicativo. O canvas e a interface compartilham o mesmo contexto OpenGL.");
                        crate::theme::caption(ui, "Mesa: seleção por endereço PCI. NVIDIA proprietário: PRIME offload; o driver escolhe a GPU NVIDIA disponível.");
                        if settings.render_gpu != self.startup_gpu { ui.colored_label(egui::Color32::YELLOW, "Reinicie o aplicativo para aplicar a alteração."); }
                    });
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.diagnostics")).default_open(true).show(ui, |ui| {
                        ui.label(crate::i18n::text(language, "settings.active_renderer"));
                        ui.label(&self.renderer);
                        crate::theme::caption(ui, "A seleção depende do suporte PRIME e dos drivers instalados no Linux.");
                    });
                }
                2 => {
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.color_selector")).default_open(true).show(ui, |ui| {
                        changed |= ui.radio_value(&mut settings.triangular_color_selector, false, "Retangular").changed();
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
                        crate::theme::caption(ui, "Todos os painéis usam transparência e blur sobre o canvas. Ajusta a intensidade do efeito; as cores do arquivo não são alteradas.");
                    });
                    egui::CollapsingHeader::new(crate::i18n::text(language, "settings.motion")).default_open(true).show(ui, |ui| {
                        changed |= crate::theme::toggle(ui, &mut settings.appearance.reduced_motion, crate::i18n::text(language, "settings.reduce_motion")).changed();
                        crate::theme::caption(ui, "Desativa pulsação, elasticidade, ondas e transições. A camada arrastada acompanha diretamente o ponteiro.");
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
        .map_err(|error| format!("Não foi possível iniciar curl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "GitHub retornou erro: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let release: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Resposta inválida do GitHub: {error}"))?;
    let version = release
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "A release do GitHub não contém tag_name.".to_owned())?;
    let url = release
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.starts_with("https://github.com/Krueels/PaintAtlas/releases/"))
        .ok_or_else(|| "A release do GitHub não contém um link válido.".to_owned())?;
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
