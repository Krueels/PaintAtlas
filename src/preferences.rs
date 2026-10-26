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
            Self::Brush => "Formato do pincel",
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

/// Select the driver before any window, GL context or background thread exists.
/// Re-exec avoids changing the process environment after threads have started.
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

pub struct Preferences {
    shortcuts: crate::shortcut_settings::Editor,
    tab: usize,
    devices: Vec<Gpu>,
    pub renderer: String,
    startup_gpu: String,
}
impl Preferences {
    pub fn new(startup_gpu: String) -> Self {
        Self {
            shortcuts: Default::default(),
            tab: 0,
            devices: discover_gpus(),
            renderer: "Não disponível neste contexto".into(),
            startup_gpu,
        }
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
        ui.horizontal_wrapped(|ui| {
            for (i, label) in [
                "Cursor",
                "Renderização",
                "Cores",
                "Aparência",
                "Atalhos",
                "Desenho",
            ]
            .iter()
            .enumerate()
            {
                ui.selectable_value(&mut self.tab, i, *label);
            }
        });
        ui.separator();
        ui.scope(|ui| match self.tab {
                4 => { changed |= self.shortcuts.show(ui, &mut settings.shortcuts); }
                5 => {
                    egui::CollapsingHeader::new("QuickShape").default_open(true).show(ui, |ui| {
                        changed |= ui.checkbox(&mut settings.quickshape.enabled, "Reconhecer formas ao desenhar e segurar").changed();
                        changed |= ui.add(egui::Slider::new(&mut settings.quickshape.delay, 0.2..=2.0).text("Espera").suffix(" s")).changed();
                        crate::theme::caption(ui, format!("Desenhe com pincel ou borracha e mantenha a caneta parada no final. Arraste para redimensionar e girar. {} cria formas regulares. Depois de soltar, use Editar forma.", settings.shortcuts.label(crate::shortcuts::Action::QuickShapeRegular)));
                    });
                }

                0 => {
                    egui::CollapsingHeader::new("Pincel e borracha").default_open(true).show(ui, |ui| {
                        ui.label("Tipo de cursor");
                        for style in [CursorStyle::Brush, CursorStyle::Circle, CursorStyle::Dot] {
                            changed |= ui.radio_value(&mut settings.cursor_style, style, style.label()).changed();
                        }
                        if settings.cursor_style == CursorStyle::Circle {
                            changed |= crate::theme::toggle(ui, &mut settings.circle_cursor_pressure, "Redimensionar círculo com a pressão da caneta").changed();
                            crate::theme::caption(ui, "Desativado: o círculo mostra o tamanho configurado do pincel, sem variar com a pressão.");
                        }
                        crate::theme::caption(ui, "Formato do pincel acompanha a pressão. O ponto mantém um tamanho fixo na tela. Essas opções alteram apenas o cursor.");
                    });
                }
                1 => {
                    egui::CollapsingHeader::new("Dispositivo gráfico").default_open(true).show(ui, |ui| {
                        ui.label("Placa de vídeo para renderização OpenGL");
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
                    egui::CollapsingHeader::new("Diagnóstico").default_open(true).show(ui, |ui| {
                        ui.label("Renderizador ativo:");
                        ui.label(&self.renderer);
                        crate::theme::caption(ui, "A seleção depende do suporte PRIME e dos drivers instalados no Linux.");
                    });
                }
                2 => {
                    egui::CollapsingHeader::new("Seletor de cor").default_open(true).show(ui, |ui| {
                        changed |= ui.radio_value(&mut settings.triangular_color_selector, false, "Retangular").changed();
                        changed |= ui.radio_value(&mut settings.triangular_color_selector, true, "Triangular").changed();
                    });
                }
                _ => {
                    egui::CollapsingHeader::new("Cores e superfícies").default_open(true).show(ui, |ui| {
                        ui.label("Cor de destaque");
                        ui.horizontal_wrapped(|ui| {
                            for accent in crate::theme::Accent::ALL {
                                changed |= ui.selectable_value(&mut settings.appearance.accent, accent,
                                    egui::RichText::new(accent.name()).color(accent.color())).changed();
                            }
                        });
                        ui.label("Intensidade do vidro");
                        let mut percent = crate::theme::glass_strength(settings.appearance.transparency) * 100.0;
                        if ui.add(egui::Slider::new(&mut percent, 35.0..=85.0).suffix(" %")).changed() {
                            settings.appearance.transparency = percent / 100.0;
                            changed = true;
                        }
                        crate::theme::caption(ui, "Todos os painéis usam transparência e blur sobre o canvas. Ajusta a intensidade do efeito; as cores do arquivo não são alteradas.");
                    });
                    egui::CollapsingHeader::new("Movimento").default_open(true).show(ui, |ui| {
                        changed |= crate::theme::toggle(ui, &mut settings.appearance.reduced_motion, "Reduzir movimento").changed();
                        crate::theme::caption(ui, "Desativa pulsação, elasticidade, ondas e transições. A camada arrastada acompanha diretamente o ponteiro.");
                    });
                    if crate::theme::button(ui, "Restaurar aparência padrão").clicked() {
                        settings.appearance = Default::default();
                        changed = true;
                    }
                }
            });
        ui.separator();
        crate::theme::caption(
            ui,
            "Preferências salvas automaticamente. Aparência e cursor mudam imediatamente.",
        );
        changed
    }
}
