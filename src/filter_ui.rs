//! Filter categories, gallery cards, and controls; processing uses the image editor.
use super::*;
use crate::filters::{self, Kind, Settings};
use crate::image_ops::{Adjustment, Job};
type Thumbnails = Vec<(Kind, Vec<u8>)>;

pub(super) struct Gallery {
    category: String,
    search: String,
    thumbnails: Vec<Option<egui::TextureHandle>>,
    job: Option<Job<Thumbnails>>,
    error: Option<String>,
    preview: Option<egui::TextureHandle>,
}
fn demonstration() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(112 * 72 * 4);
    for y in 0..72 {
        for x in 0..112 {
            let mut color = [
                (x as f32 / 111.0 * 255.0) as u8,
                (y as f32 / 71.0 * 255.0) as u8,
                160,
                255,
            ];
            if (x as f32 - 58.0).powi(2) + (y as f32 - 36.0).powi(2) < 19.0f32.powi(2) {
                color = [245, 180, 65, 255];
            }
            if (8..31).contains(&x) && (12..57).contains(&y) {
                color = if (x + y) % 7 < 3 {
                    [25, 40, 65, 255]
                } else {
                    [240, 245, 250, 255]
                };
            }
            if x > 95 && y < 14 {
                color = [0; 4];
            }
            pixels.extend(color);
        }
    }
    pixels
}
impl Gallery {
    pub(super) fn new(ctx: &egui::Context) -> Self {
        let source = demonstration();
        let result = Job::start(ctx.clone(), move |cancel| {
            let mut thumbnails = Vec::with_capacity(Kind::ALL.len());
            for &kind in Kind::ALL {
                let settings = Settings::new(kind, 112, 72);
                thumbnails.push((kind, filters::apply(&source, None, &settings, cancel)?));
            }
            Ok(thumbnails)
        });
        let (job, error) = match result {
            Ok(job) => (Some(job), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            category: String::new(),
            search: String::new(),
            thumbnails: vec![None; Kind::ALL.len()],
            job,
            error,
            preview: None,
        }
    }
    pub(super) fn set_preview(
        &mut self,
        ctx: &egui::Context,
        pixels: &[u8],
        width: usize,
        height: usize,
    ) {
        let scale = (384.0 / width.max(height) as f32).min(1.0);
        let w = (width as f32 * scale).round().max(1.0) as usize;
        let h = (height as f32 * scale).round().max(1.0) as usize;
        let mut colors = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let px = (x as f32 + 0.5) * width as f32 / w as f32 - 0.5;
                let py = (y as f32 + 0.5) * height as f32 / h as f32 - 0.5;
                let ix = px.floor() as i32;
                let iy = py.floor() as i32;
                let fx = px - px.floor();
                let fy = py - py.floor();
                let mut sum = [0.0; 4];
                for (dx, dy, weight) in [
                    (0, 0, (1.0 - fx) * (1.0 - fy)),
                    (1, 0, fx * (1.0 - fy)),
                    (0, 1, (1.0 - fx) * fy),
                    (1, 1, fx * fy),
                ] {
                    let sx = (ix + dx).clamp(0, width as i32 - 1) as usize;
                    let sy = (iy + dy).clamp(0, height as i32 - 1) as usize;
                    let p = &pixels[(sy * width + sx) * 4..][..4];
                    let alpha = p[3] as f32 / 255.0;
                    for c in 0..3 {
                        sum[c] += p[c] as f32 * alpha * weight;
                    }
                    sum[3] += alpha * weight;
                }
                colors.push(if sum[3] <= 1e-6 {
                    Color32::TRANSPARENT
                } else {
                    Color32::from_rgba_unmultiplied(
                        (sum[0] / sum[3]).round() as u8,
                        (sum[1] / sum[3]).round() as u8,
                        (sum[2] / sum[3]).round() as u8,
                        (sum[3] * 255.0).round() as u8,
                    )
                });
            }
        }
        let image = egui::ColorImage::new([w, h], colors);
        if let Some(texture) = &mut self.preview {
            texture.set(image, egui::TextureOptions::LINEAR);
        } else {
            self.preview = Some(ctx.load_texture(
                "filter_actual_preview",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }
    fn show_preview(&self, ui: &mut egui::Ui) {
        crate::theme::caption(ui, "Prévia da camada ativa");
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(
                ui.available_width(),
                (ui.ctx().content_rect().height() * 0.22).clamp(140.0, 220.0),
            ),
            Sense::hover(),
        );
        ui.painter().rect_filled(rect, 8.0, Color32::from_gray(22));
        let painter = ui.painter().with_clip_rect(rect);
        if let Some(texture) = &self.preview {
            let size = texture.size_vec2();
            let scale = (rect.width() / size.x).min(rect.height() / size.y);
            let image_rect = egui::Rect::from_center_size(rect.center(), size * scale);
            let cols = (image_rect.width() / 16.0).ceil() as usize;
            let rows = (image_rect.height() / 16.0).ceil() as usize;
            for y in 0..rows {
                for x in 0..cols {
                    let tile = egui::Rect::from_min_size(
                        image_rect.min + Vec2::new(x as f32 * 16.0, y as f32 * 16.0),
                        Vec2::splat(16.0),
                    )
                    .intersect(image_rect);
                    painter.rect_filled(
                        tile,
                        0.0,
                        Color32::from_gray(if (x + y).is_multiple_of(2) { 175 } else { 215 }),
                    );
                }
            }
            painter.image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        ui.add_space(6.0);
    }
    pub(super) fn ready(&self) -> bool {
        self.job.is_none() && self.thumbnails.iter().all(Option::is_some)
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui, settings: &mut Settings) {
        if let Some(result) = self.job.as_ref().and_then(Job::poll) {
            self.job = None;
            match result {
                Ok(thumbnails) => {
                    for (kind, pixels) in thumbnails {
                        let index = Kind::ALL.iter().position(|k| *k == kind).unwrap();
                        self.thumbnails[index] = Some(ui.ctx().load_texture(
                            format!("filter_thumb_{index}"),
                            egui::ColorImage::from_rgba_unmultiplied([112, 72], &pixels),
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
        let width = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(width * 0.53, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_max_height((ui.ctx().content_rect().height() - 280.0).max(240.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Buscar filtro…")
                            .desired_width(ui.available_width()),
                    );
                    egui::ComboBox::from_id_salt("filter_category")
                        .selected_text(if self.category.is_empty() {
                            "Todas as categorias"
                        } else {
                            &self.category
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.category,
                                String::new(),
                                "Todas as categorias",
                            );
                            for &category in filters::CATEGORIES {
                                ui.selectable_value(&mut self.category, category.into(), category);
                            }
                        });
                    crate::theme::caption(ui, "Miniaturas demonstrativas · prévia real à direita");
                    super::image_menu::processing_status(
                        ui,
                        self.job.is_some(),
                        "Preparando miniaturas…",
                        self.error.as_deref(),
                    );
                    let query = self.search.to_lowercase();
                    let kinds: Vec<_> = Kind::ALL
                        .iter()
                        .enumerate()
                        .filter(|(_, kind)| {
                            (self.category.is_empty() || kind.category() == self.category)
                                && (query.is_empty()
                                    || format!("{} {}", kind.label(), kind.category())
                                        .to_lowercase()
                                        .contains(&query))
                        })
                        .collect();
                    let columns = if ui.available_width() >= 330.0 { 3 } else { 2 };
                    let card_width = (ui.available_width()
                        - ui.spacing().item_spacing.x * (columns - 1) as f32)
                        / columns as f32;
                    egui::ScrollArea::vertical()
                        .id_salt("filter_gallery_cards")
                        .max_height((ui.ctx().content_rect().height() - 420.0).clamp(160.0, 520.0))
                        .show(ui, |ui| {
                            egui::Grid::new("filter_cards").show(ui, |ui| {
                                for (n, (index, kind)) in kinds.iter().enumerate() {
                                    let (rect, response) = ui.allocate_exact_size(
                                        Vec2::new(card_width, 104.0),
                                        Sense::click(),
                                    );
                                    let selected = settings.kind == **kind;
                                    ui.painter().rect_filled(
                                        rect,
                                        10.0,
                                        if selected {
                                            crate::theme::accent(ui.ctx()).gamma_multiply(0.22)
                                        } else {
                                            Color32::from_gray(if response.hovered() {
                                                53
                                            } else {
                                                35
                                            })
                                        },
                                    );
                                    if selected {
                                        ui.painter().rect_stroke(
                                            rect,
                                            10.0,
                                            Stroke::new(1.5_f32, crate::theme::accent(ui.ctx())),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    let image_rect = egui::Rect::from_min_size(
                                        rect.min + Vec2::splat(5.0),
                                        Vec2::new(rect.width() - 10.0, 64.0),
                                    );
                                    if let Some(texture) = &self.thumbnails[*index] {
                                        let size = texture.size_vec2();
                                        let scale = (image_rect.width() / size.x)
                                            .min(image_rect.height() / size.y);
                                        let fitted = egui::Rect::from_center_size(
                                            image_rect.center(),
                                            size * scale,
                                        );
                                        ui.painter().image(
                                            texture.id(),
                                            fitted,
                                            egui::Rect::from_min_max(
                                                Pos2::ZERO,
                                                Pos2::new(1.0, 1.0),
                                            ),
                                            Color32::WHITE,
                                        );
                                    }
                                    let text = ui.painter().layout(
                                        kind.label().into(),
                                        egui::FontId::proportional(11.0),
                                        Color32::WHITE,
                                        rect.width() - 10.0,
                                    );
                                    ui.painter().galley(
                                        Pos2::new(rect.left() + 5.0, rect.top() + 74.0),
                                        text,
                                        Color32::WHITE,
                                    );
                                    let response = response.on_hover_text(format!(
                                        "{} · {}",
                                        kind.category(),
                                        kind.label()
                                    ));
                                    if response.clicked() {
                                        select_kind(settings, **kind);
                                    }
                                    if (n + 1) % columns == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                        });
                    if kinds.is_empty() {
                        ui.label("Nenhum filtro encontrado.");
                    }
                },
            );
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                Vec2::new((width * 0.43).max(190.0), 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_max_height((ui.ctx().content_rect().height() - 280.0).max(240.0));
                    ui.heading(settings.kind.label());
                    self.show_preview(ui);
                    egui::ScrollArea::vertical()
                        .id_salt("filter_gallery_parameters")
                        .max_height((ui.ctx().content_rect().height() - 620.0).clamp(140.0, 380.0))
                        .show(ui, |ui| controls(ui, settings));
                },
            );
        });
    }
}
fn select_kind(settings: &mut Settings, kind: Kind) {
    if settings.kind == kind {
        return;
    }
    let old = settings.clone();
    *settings = Settings::new(kind, old.width, old.height);
    settings.alpha_lock = old.alpha_lock;
    settings.preserve_alpha = old.preserve_alpha;
    settings.seed = old.seed;
    if kind != Kind::NeonGlow || old.colors[0] != [0; 3] {
        settings.colors = old.colors;
    }
    settings.map = old.map;
}
fn options(kind: Kind, index: usize) -> Option<&'static [&'static str]> {
    match (kind, index) {
        (Kind::ShapeBlur, 1) => Some(&["Disco", "Hexágono", "Quadrado"]),
        (Kind::AddNoise | Kind::FilmGrain | Kind::Grain, 1) => Some(&["Uniforme", "Gaussiana"]),
        (Kind::Polar, 0) => Some(&["Retangular → polar", "Polar → retangular"]),
        (Kind::Wind, 1) => Some(&["Da esquerda", "Da direita"]),
        (Kind::Deinterlace, 0) => Some(&["Linhas pares", "Linhas ímpares"]),
        _ => None,
    }
}
pub(super) fn controls(ui: &mut egui::Ui, settings: &mut Settings) {
    if settings.kind.approximation() {
        crate::theme::caption(
            ui,
            "Implementação aproximada do PaintAtlas; não reproduz o algoritmo da Adobe.",
        );
    }
    if settings.kind == Kind::Custom {
        ui.label("Matriz de convolução 3 × 3");
        egui::Grid::new("custom_filter_kernel").show(ui, |ui| {
            for row in 0..3 {
                for col in 0..3 {
                    ui.add(
                        egui::DragValue::new(&mut settings.p[row * 3 + col])
                            .range(-100.0..=100.0)
                            .speed(0.1),
                    );
                }
                ui.end_row();
            }
        });
    }
    for control in settings.kind.controls() {
        let value = &mut settings.p[control.index];
        if let Some(options) = options(settings.kind, control.index) {
            ui.label(control.label.split(':').next().unwrap_or(control.label));
            egui::ComboBox::from_id_salt((settings.kind.label(), control.index))
                .selected_text(options[(*value as usize).min(options.len() - 1)])
                .show_ui(ui, |ui| {
                    for (i, label) in options.iter().enumerate() {
                        ui.selectable_value(value, i as f32, *label);
                    }
                });
        } else if control.label == "Monocromático" || control.label == "Colorido" {
            let mut enabled = *value > 0.5;
            if ui.checkbox(&mut enabled, control.label).changed() {
                *value = if enabled { 1.0 } else { 0.0 };
            }
        } else {
            let slider = egui::Slider::new(value, control.min..=control.max).text(control.label);
            ui.add(if control.integer {
                slider.integer()
            } else {
                slider
            });
        }
    }
    if settings.kind == Kind::Displace {
        if ui.button("Importar mapa de deslocamento…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Imagem", &["png", "jpg", "jpeg", "webp", "bmp"])
                .pick_file()
        {
            match Document::import(&path) {
                Ok(mut doc) => {
                    ui.ctx().data_mut(|d| {
                        d.remove::<String>(egui::Id::new("filter_map_error"));
                    });
                    settings.map = Some(std::sync::Arc::new(filters::Map {
                        width: doc.width as usize,
                        height: doc.height as usize,
                        pixels: std::mem::take(&mut doc.layers[0].pixels),
                    }))
                }
                Err(error) => {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::new("filter_map_error"), error));
                }
            }
        }
        if settings.map.is_some() {
            ui.label("Mapa externo carregado.");
            if ui.small_button("Remover mapa").clicked() {
                settings.map = None;
            }
        } else {
            crate::theme::caption(
                ui,
                "Sem mapa externo: usa os canais vermelho/verde da própria camada.",
            );
        }
        if let Some(error) = ui
            .ctx()
            .data(|d| d.get_temp::<String>(egui::Id::new("filter_map_error")))
        {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
    if settings.kind.seeded() && ui.button("Nova semente").clicked() {
        settings.seed = settings.seed.wrapping_mul(1664525).wrapping_add(1013904223);
    }
    let color_controls: &[(usize, &str)] = match settings.kind {
        Kind::Clouds | Kind::DifferenceClouds | Kind::Fibers => {
            &[(0, "Cor principal"), (1, "Cor de fundo")]
        }
        Kind::NeonGlow => &[(0, "Cor do brilho")],
        Kind::StainedGlass => &[(0, "Cor da borda")],
        Kind::Pointillize | Kind::MosaicTiles => &[(1, "Cor de fundo")],
        _ => &[],
    };
    for &(index, label) in color_controls {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.color_edit_button_srgb(&mut settings.colors[index]);
        });
    }
    if matches!(settings.kind.category(), "Distorcer")
        || matches!(
            settings.kind,
            Kind::Offset
                | Kind::Diffuse
                | Kind::MotionBlur
                | Kind::RadialBlur
                | Kind::ZoomBlur
                | Kind::ShapeBlur
                | Kind::Wind
        )
    {
        ui.label("Bordas");
        egui::ComboBox::from_id_salt("filter_edge_mode")
            .selected_text(match settings.edge {
                filters::Edge::Clamp => "Repetir borda",
                filters::Edge::Wrap => "Repetir imagem",
                filters::Edge::Transparent => "Transparente",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut settings.edge, filters::Edge::Clamp, "Repetir borda");
                ui.selectable_value(&mut settings.edge, filters::Edge::Wrap, "Repetir imagem");
                ui.selectable_value(
                    &mut settings.edge,
                    filters::Edge::Transparent,
                    "Transparente",
                );
            });
    }
    if settings.kind.changes_alpha() {
        let mut preserve = settings.preserve_alpha || settings.alpha_lock;
        ui.add_enabled_ui(!settings.alpha_lock, |ui| {
            if ui
                .checkbox(&mut preserve, "Preservar transparência")
                .changed()
            {
                settings.preserve_alpha = preserve;
            }
        });
        if settings.alpha_lock {
            crate::theme::caption(ui, "A camada está com o alfa bloqueado.");
        }
    }
    ui.add(egui::Slider::new(&mut settings.mix, 0.0..=100.0).text("Intensidade do efeito (%)"));
}
impl PaintAtlas {
    pub(super) fn filter_shortcut(&mut self, ctx: &egui::Context, action: Shortcut) -> bool {
        if action == Shortcut::FilterGallery {
            self.open_filter(
                ctx,
                self.last_filter
                    .as_ref()
                    .map_or(Kind::GaussianBlur, |f| f.kind),
                true,
                false,
            );
            return true;
        }
        if action == Shortcut::RepeatFilter {
            if let Some(filter) = &self.last_filter {
                self.open_filter(ctx, filter.kind, false, true);
            } else {
                self.status = "Aplique um filtro antes de repetir.".into();
            }
            return true;
        }
        if let Some(&kind) = Kind::ALL.iter().find(|k| k.shortcut() == action) {
            self.open_filter(ctx, kind, false, false);
            return true;
        }
        false
    }
    pub(super) fn open_filter(
        &mut self,
        ctx: &egui::Context,
        kind: Kind,
        gallery: bool,
        repeat: bool,
    ) {
        if !self.image_operation_allowed() {
            return;
        }
        let mut filter = if let Some(filter) = &self.last_filter
            && filter.kind == kind
        {
            filter.clone()
        } else {
            Settings::new(kind, self.doc.width as usize, self.doc.height as usize)
        };
        filter.width = self.doc.width as usize;
        filter.height = self.doc.height as usize;
        filter.alpha_lock = self.doc.layers[self.doc.active].alpha_lock;
        let mut settings = crate::image_ops::Settings::new(Adjustment::Filter);
        settings.filter = Some(filter);
        self.open_effect(ctx, settings, gallery, repeat);
    }
    pub(super) fn filter_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let enabled = self.image_operation_allowed();
        let menu = ui.menu_button("Filtro", |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                let label = self.last_filter.as_ref().map_or_else(
                    || "Último filtro".to_owned(),
                    |f| format!("Último filtro: {}", f.kind.label()),
                );
                if ui
                    .add_enabled(
                        self.last_filter.is_some(),
                        egui::Button::new(
                            self.settings.shortcuts.menu(Shortcut::RepeatFilter, &label),
                        ),
                    )
                    .clicked()
                {
                    self.filter_shortcut(&ctx, Shortcut::RepeatFilter);
                    ui.close();
                }
                ui.separator();
                if ui
                    .button(
                        self.settings
                            .shortcuts
                            .menu(Shortcut::FilterGallery, "Galeria de filtros…"),
                    )
                    .clicked()
                {
                    self.filter_shortcut(&ctx, Shortcut::FilterGallery);
                    ui.close();
                }
                ui.separator();
                for &category in filters::CATEGORIES {
                    let submenu = ui.menu_button(category, |ui| {
                        for &kind in Kind::ALL.iter().filter(|kind| kind.category() == category) {
                            if ui
                                .button(
                                    self.settings
                                        .shortcuts
                                        .menu(kind.shortcut(), &format!("{}…", kind.label())),
                                )
                                .clicked()
                            {
                                self.open_filter(&ctx, kind, false, false);
                                ui.close();
                            }
                        }
                    });
                    if self.screenshot.is_some() && category == "Desfoque" {
                        ctx.data_mut(|d| {
                            d.insert_temp(
                                egui::Id::new("filter_blur_menu_rect"),
                                submenu.response.rect,
                            )
                        });
                    }
                }
            });
        });
        if self.screenshot.is_some() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("filter_menu_rect"), menu.response.rect));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn finish(app: &mut PaintAtlas, ctx: &egui::Context, apply: bool) {
        for _ in 0..2000 {
            let _ = ctx.run(
                egui::RawInput {
                    events: if apply {
                        vec![
                            egui::Event::Key {
                                key: egui::Key::Enter,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: Default::default(),
                            },
                            egui::Event::Key {
                                key: egui::Key::Enter,
                                physical_key: None,
                                pressed: false,
                                repeat: false,
                                modifiers: Default::default(),
                            },
                        ]
                    } else {
                        vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        }]
                    },
                    ..Default::default()
                },
                |ctx| app.image_dialog(ctx),
            );
            if app.image_editor.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("Filter job did not complete");
    }
    #[test]
    fn gallery_first_row_labels_and_effect_strength_are_visible() {
        let ctx = egui::Context::default();
        let mut gallery = Gallery {
            category: String::new(),
            search: String::new(),
            thumbnails: vec![None; Kind::ALL.len()],
            job: None,
            error: None,
            preview: None,
        };
        let mut settings = Settings::new(Kind::GaussianBlur, 8, 8);
        let mut output = egui::FullOutput::default();
        for _ in 0..4 {
            output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(1100.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::Window::new("Gallery layout test")
                        .default_pos(egui::pos2(20.0, 20.0))
                        .show(ctx, |ui| {
                            ui.set_width(920.0);
                            egui::ScrollArea::vertical()
                                .max_height(540.0)
                                .show(ui, |ui| gallery.show(ui, &mut settings));
                        });
                },
            );
        }
        let mut labels = std::collections::BTreeSet::new();
        fn inspect(
            shape: &egui::Shape,
            clip: egui::Rect,
            labels: &mut std::collections::BTreeSet<String>,
        ) {
            match shape {
                egui::Shape::Text(text) => {
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    if clip.contains(rect.center()) {
                        labels.insert(text.galley.text().to_owned());
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        inspect(shape, clip, labels);
                    }
                }
                _ => {}
            }
        }
        for shape in &output.shapes {
            inspect(&shape.shape, shape.clip_rect, &mut labels);
        }
        for label in [
            "Média",
            "Desfoque",
            "Desfocar mais",
            "Intensidade do efeito (%)",
        ] {
            assert!(
                labels.iter().any(|text| text.contains(label)),
                "Clipped or missing: {label}"
            );
        }
    }

    #[test]
    fn apply_cancel_repeat_and_undo_keep_history_atomic() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(12, 8, false), None, false);
        app.doc.layers[0].pixels = [100, 100, 100, 128].repeat(96);
        let original = app.doc.layers[0].pixels.clone();
        app.open_filter(&ctx, Kind::AddNoise, false, false);
        finish(&mut app, &ctx, true);
        assert!(app.dirty);
        assert_eq!(app.history.counts(), (1, 0));
        assert_eq!(app.last_filter.as_ref().unwrap().kind, Kind::AddNoise);
        let filtered = app.doc.layers[0].pixels.clone();
        assert_ne!(filtered, original);
        app.undo();
        assert_eq!(app.doc.layers[0].pixels, original);
        app.redo();
        assert_eq!(app.doc.layers[0].pixels, filtered);
        app.filter_shortcut(&ctx, Shortcut::RepeatFilter);
        finish(&mut app, &ctx, true);
        assert_eq!(app.history.counts(), (2, 0));
        assert_ne!(app.doc.layers[0].pixels, filtered);
        let before = app.doc.layers[0].pixels.clone();
        let history = app.history.counts();
        app.open_filter(&ctx, Kind::GaussianBlur, false, false);
        finish(&mut app, &ctx, false);
        assert_eq!(app.doc.layers[0].pixels, before);
        assert_eq!(app.history.counts(), history);
        assert_eq!(app.last_filter.as_ref().unwrap().kind, Kind::AddNoise);
    }
    #[test]
    fn last_filter_rebases_dimensions_and_layer_alpha_lock() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(4, 4, false), None, false);
        app.open_filter(&ctx, Kind::Clouds, false, false);
        finish(&mut app, &ctx, true);
        assert!(
            app.doc.layers[0]
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[3] == 255)
        );
        app.add_document(Document::new(7, 3, false), None, false);
        app.doc.layers[0].alpha_lock = true;
        app.filter_shortcut(&ctx, Shortcut::RepeatFilter);
        finish(&mut app, &ctx, true);
        assert_eq!(app.doc.layers[0].pixels.len(), 7 * 3 * 4);
        assert!(app.doc.layers[0].pixels.iter().all(|v| *v == 0));
        assert!(!app.dirty);
        assert_eq!(app.history.counts(), (0, 0));
        app.add_document(Document::new(5, 6, false), None, false);
        app.filter_shortcut(&ctx, Shortcut::RepeatFilter);
        finish(&mut app, &ctx, true);
        assert!(
            app.doc.layers[0]
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[3] == 255)
        );
        assert!(app.dirty);
    }
    #[test]
    fn gallery_finishes_all_thumbnail_jobs_and_filters_have_configurable_shortcuts() {
        let ctx = egui::Context::default();
        let mut gallery = Gallery::new(&ctx);
        let mut settings = Settings::new(Kind::GaussianBlur, 8, 8);
        for _ in 0..2000 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(1100.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| gallery.show(ui, &mut settings));
                },
            );
            if gallery.job.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(gallery.error.is_none(), "{:?}", gallery.error);
        assert!(gallery.thumbnails.iter().all(Option::is_some));
        let mut ids = std::collections::BTreeSet::new();
        for &kind in Kind::ALL {
            assert!(ids.insert(kind.shortcut().id()));
            assert_eq!(kind.shortcut().category(), "Filtros");
        }
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, false);
        let mut binding = crate::shortcuts::Binding::from_modifiers(egui::Modifiers::CTRL);
        binding.key = Some("J".into());
        assert!(
            app.settings
                .shortcuts
                .assign(Shortcut::GaussianBlur, 0, binding, true)
        );
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::J,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(app.image_editor.is_some());
    }
}
