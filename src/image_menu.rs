//! Image menu, cancellable background processing, and non-destructive previews.
use super::*;
use crate::image_ops::{self, Adjustment, Geometry, Job, Settings as AdjustmentSettings};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) enum Editor {
    Adjustment(Box<AdjustmentEditor>),
    Geometry(GeometryEditor),
    Working { job: Job<Document>, name: String },
}
impl Editor {
    pub(super) fn gallery_ready(&self) -> bool {
        matches!(self, Self::Adjustment(editor) if editor.gallery.as_ref().is_some_and(super::filter_ui::Gallery::ready) && editor.latest.is_some())
    }
}
pub(super) struct AdjustmentEditor {
    settings: AdjustmentSettings,
    auto_apply: bool,
    gallery: Option<super::filter_ui::Gallery>,
    original: Arc<[u8]>,
    mask: Option<Arc<crate::selection::Mask>>,
    layer: usize,
    was_dirty: bool,
    preview: bool,
    preview_applied: bool,
    latest: Option<Arc<[u8]>>,
    job: Option<Job<Vec<u8>>>,
    edited: Instant,
    error: Option<String>,
}
#[derive(Clone, Copy)]
pub(super) enum GeometryKind {
    Resize,
    Canvas,
    Rotate,
    Trim,
}
pub(super) struct GeometryEditor {
    kind: GeometryKind,
    width: u32,
    height: u32,
    dpi: u32,
    proportion: bool,
    resample: bool,
    filter: usize,
    anchor: [u8; 2],
    angle: f32,
    corner: u8,
    tolerance: u8,
    sides: [bool; 4],
}
impl GeometryKind {
    fn label(self) -> &'static str {
        match self {
            Self::Resize => "Tamanho da imagem",
            Self::Canvas => "Tamanho da tela de pintura",
            Self::Rotate => "Rotação arbitrária",
            Self::Trim => "Aparar bordas",
        }
    }
}
fn adjustment_shortcut(kind: Adjustment) -> Shortcut {
    match kind {
        Adjustment::Filter => Shortcut::FilterGallery,
        Adjustment::BrightnessContrast => Shortcut::ImageBrightness,
        Adjustment::Levels => Shortcut::ImageLevels,
        Adjustment::Curves => Shortcut::ImageCurves,
        Adjustment::Exposure => Shortcut::ImageExposure,
        Adjustment::Vibrance => Shortcut::ImageVibrance,
        Adjustment::HueSaturation => Shortcut::ImageHue,
        Adjustment::ColorBalance => Shortcut::ImageBalance,
        Adjustment::BlackWhite => Shortcut::ImageBlackWhite,
        Adjustment::PhotoFilter => Shortcut::ImagePhotoFilter,
        Adjustment::ChannelMixer => Shortcut::ImageChannelMixer,
        Adjustment::Invert => Shortcut::ImageInvert,
        Adjustment::Posterize => Shortcut::ImagePosterize,
        Adjustment::Threshold => Shortcut::ImageThreshold,
        Adjustment::GradientMap => Shortcut::ImageGradientMap,
        Adjustment::ShadowsHighlights => Shortcut::ImageShadows,
        Adjustment::Desaturate => Shortcut::ImageDesaturate,
        Adjustment::ReplaceColor => Shortcut::ImageReplaceColor,
        Adjustment::Equalize => Shortcut::ImageEqualize,
        Adjustment::AutoTone => Shortcut::ImageAutoTone,
        Adjustment::AutoContrast => Shortcut::ImageAutoContrast,
        Adjustment::AutoColor => Shortcut::ImageAutoColor,
    }
}
impl PaintAtlas {
    pub(super) fn image_operation_allowed(&self) -> bool {
        !self.startup_canvas_required
            && !self.documents.is_empty()
            && self.image_editor.is_none()
            && self.selection.floating.is_none()
            && self.gesture.is_none()
            && !self.creating_selection()
            && self.pending.is_none()
            && !self.new_dialog
            && self.brush_editor.is_none()
    }
    pub(super) fn image_shortcut(&mut self, ctx: &egui::Context, action: Shortcut) -> bool {
        if self.filter_shortcut(ctx, action) {
            return true;
        }
        for &kind in Adjustment::ALL.iter().chain(
            [
                Adjustment::AutoTone,
                Adjustment::AutoContrast,
                Adjustment::AutoColor,
            ]
            .iter(),
        ) {
            if adjustment_shortcut(kind) == action {
                self.open_adjustment(ctx, kind);
                return true;
            }
        }
        match action {
            Shortcut::ImageSize => self.open_geometry(GeometryKind::Resize),
            Shortcut::ImageCanvasSize => self.open_geometry(GeometryKind::Canvas),
            Shortcut::ImageRotate => self.open_geometry(GeometryKind::Rotate),
            Shortcut::ImageTrim => self.open_geometry(GeometryKind::Trim),
            Shortcut::ImageCrop => self.crop_image(ctx),
            Shortcut::ImageDuplicate => self.duplicate_image(),
            Shortcut::ImageRotate90 => {
                self.start_geometry(ctx, Geometry::Rotate90, "Rotação 90° horário", None)
            }
            Shortcut::ImageRotate180 => {
                self.start_geometry(ctx, Geometry::Rotate180, "Rotação 180°", None)
            }
            Shortcut::ImageRotate270 => {
                self.start_geometry(ctx, Geometry::Rotate270, "Rotação 90° anti-horário", None)
            }
            Shortcut::ImageFlipHorizontal => self.start_geometry(
                ctx,
                Geometry::FlipHorizontal,
                "Espelhar imagem horizontalmente",
                None,
            ),
            Shortcut::ImageFlipVertical => self.start_geometry(
                ctx,
                Geometry::FlipVertical,
                "Espelhar imagem verticalmente",
                None,
            ),
            _ => return false,
        }
        true
    }
    pub(super) fn image_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let enabled = self.image_operation_allowed();
        let menu = ui.menu_button("Imagem", |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                crate::theme::caption(ui, "RGB / Alfa · 8 bits por canal");
                let submenu = ui.menu_button("Ajustes", |ui| {
                    for &kind in Adjustment::ALL {
                        if matches!(
                            kind,
                            Adjustment::Invert
                                | Adjustment::ShadowsHighlights
                                | Adjustment::Desaturate
                        ) {
                            ui.separator();
                        }
                        let label = format!(
                            "{}{}",
                            kind.label(),
                            if kind.immediate() { "" } else { "…" }
                        );
                        if ui
                            .button(
                                self.settings
                                    .shortcuts
                                    .menu(adjustment_shortcut(kind), &label),
                            )
                            .clicked()
                        {
                            self.open_adjustment(&ctx, kind);
                            ui.close();
                        }
                    }
                });
                if self.screenshot.is_some() {
                    ctx.data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("image_adjustments_rect"),
                            submenu.response.rect,
                        )
                    });
                }
                for kind in [
                    Adjustment::AutoTone,
                    Adjustment::AutoContrast,
                    Adjustment::AutoColor,
                ] {
                    if ui
                        .button(
                            self.settings
                                .shortcuts
                                .menu(adjustment_shortcut(kind), kind.label()),
                        )
                        .clicked()
                    {
                        self.open_adjustment(&ctx, kind);
                        ui.close();
                    }
                }
                ui.separator();
                for (action, label) in [
                    (Shortcut::ImageSize, "Tamanho da imagem…"),
                    (Shortcut::ImageCanvasSize, "Tamanho da tela de pintura…"),
                ] {
                    if ui
                        .button(self.settings.shortcuts.menu(action, label))
                        .clicked()
                    {
                        self.image_shortcut(&ctx, action);
                        ui.close();
                    }
                }
                ui.menu_button("Rotação da imagem", |ui| {
                    for (action, label) in [
                        (Shortcut::ImageRotate180, "180°"),
                        (Shortcut::ImageRotate90, "90° horário"),
                        (Shortcut::ImageRotate270, "90° anti-horário"),
                        (Shortcut::ImageRotate, "Arbitrária…"),
                        (Shortcut::ImageFlipHorizontal, "Espelhar horizontalmente"),
                        (Shortcut::ImageFlipVertical, "Espelhar verticalmente"),
                    ] {
                        if ui
                            .button(self.settings.shortcuts.menu(action, label))
                            .clicked()
                        {
                            self.image_shortcut(&ctx, action);
                            ui.close();
                        }
                    }
                    crate::theme::caption(
                        ui,
                        "Altera os pixels de todas as camadas, não apenas a vista.",
                    );
                });
                if ui
                    .add_enabled(
                        self.selection.mask.is_some(),
                        egui::Button::new(
                            self.settings
                                .shortcuts
                                .menu(Shortcut::ImageCrop, "Cortar pela seleção"),
                        ),
                    )
                    .on_hover_text(
                        "Recorta todas as camadas pelos limites retangulares da seleção.",
                    )
                    .clicked()
                {
                    self.crop_image(&ctx);
                    ui.close();
                }
                if ui
                    .button(self.settings.shortcuts.menu(Shortcut::ImageTrim, "Aparar…"))
                    .clicked()
                {
                    self.open_geometry(GeometryKind::Trim);
                    ui.close();
                }
                ui.separator();
                if ui
                    .button(
                        self.settings
                            .shortcuts
                            .menu(Shortcut::ImageDuplicate, "Duplicar imagem"),
                    )
                    .clicked()
                {
                    self.duplicate_image();
                    ui.close();
                }
            });
        });
        if self.screenshot.is_some() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("image_menu_rect"), menu.response.rect));
        }
    }
    pub(super) fn open_adjustment(&mut self, ctx: &egui::Context, kind: Adjustment) {
        self.open_effect(ctx, AdjustmentSettings::new(kind), false, false);
    }
    pub(super) fn open_effect(
        &mut self,
        ctx: &egui::Context,
        settings: AdjustmentSettings,
        gallery: bool,
        repeat: bool,
    ) {
        if !self.image_operation_allowed() {
            return;
        }
        let layer = self.doc.active;
        if self.doc.layers[layer].effects.len() >= 32 {
            self.status = "Limite de 32 efeitos no grupo atingido.".into();
            return;
        }
        if !self.doc.layers[layer].visible {
            self.status = "Torne a camada ativa visível antes de ajustar suas cores.".into();
            return;
        }
        self.commit_name();
        self.history.commit(&self.doc);
        self.opacity_edit = false;
        let mut editor = AdjustmentEditor {
            auto_apply: repeat || settings.kind.immediate(),
            settings,
            gallery: gallery.then(|| super::filter_ui::Gallery::new(ctx)),
            original: self.doc.layers[layer].pixels.clone().into(),
            mask: self.selection.mask.clone(),
            layer,
            was_dirty: self.dirty,
            preview: true,
            preview_applied: false,
            latest: None,
            job: None,
            edited: Instant::now() - Duration::from_secs(1),
            error: None,
        };
        if let Some(gallery) = &mut editor.gallery {
            gallery.set_preview(
                ctx,
                &editor.original,
                self.doc.width as usize,
                self.doc.height as usize,
            );
        }
        editor.start_job(ctx);
        self.image_editor = Some(Editor::Adjustment(Box::new(editor)));
    }
    fn open_geometry(&mut self, kind: GeometryKind) {
        if !self.image_operation_allowed() {
            return;
        }
        self.commit_name();
        self.history.commit(&self.doc);
        self.image_editor = Some(Editor::Geometry(GeometryEditor {
            kind,
            width: self.doc.width,
            height: self.doc.height,
            dpi: self.doc.dpi,
            proportion: true,
            resample: true,
            filter: 3,
            anchor: [1, 1],
            angle: 15.0,
            corner: 0,
            tolerance: 0,
            sides: [true; 4],
        }));
    }
    fn start_geometry(&mut self, ctx: &egui::Context, op: Geometry, name: &str, dpi: Option<u32>) {
        if !self.image_operation_allowed() {
            return;
        }
        self.commit_name();
        self.history.commit(&self.doc);
        let source = self.doc.clone();
        match Job::start(ctx.clone(), move |cancel| {
            let mut doc = image_ops::geometry(source, op, cancel)?;
            if let Some(dpi) = dpi {
                doc.dpi = dpi.clamp(1, crate::document::MAX_DPI);
            }
            Ok(doc)
        }) {
            Ok(job) => {
                self.image_editor = Some(Editor::Working {
                    job,
                    name: name.into(),
                })
            }
            Err(error) => self.status = error,
        }
    }
    fn crop_image(&mut self, ctx: &egui::Context) {
        if let Some(mask) = &self.selection.mask {
            let r = mask.bounds;
            self.start_geometry(
                ctx,
                Geometry::Crop([
                    r.min.x as u32,
                    r.min.y as u32,
                    r.max.x as u32,
                    r.max.y as u32,
                ]),
                "Cortar imagem",
                None,
            );
        } else {
            self.status = "Crie uma seleção para recortar a imagem.".into();
        }
    }
    fn duplicate_image(&mut self) {
        if !self.image_operation_allowed() {
            return;
        }
        self.commit_name();
        let mut doc = self.doc.clone();
        doc.selection = None;
        doc.damage = None;
        doc.dirty_tiles.clear();
        *doc.stroke_tiles.get_mut() = None;
        self.add_document(doc, None, true);
        self.status = "Imagem duplicada em um novo canvas, com todas as camadas.".into();
    }
    fn preview_pixels(&mut self, layer: usize, pixels: &[u8]) {
        self.doc.layers[layer].pixels.copy_from_slice(pixels);
        self.doc.rebuild_groups_after(None, Some(layer));
        // Only this layer changed. Keep every other GPU texture intact.
        self.doc.damage = Some([0, 0, self.doc.width as usize, self.doc.height as usize]);
        self.doc.dirty_tiles.clear();
        self.refresh = true;
    }
    fn finish_adjustment(&mut self, editor: AdjustmentEditor, apply: bool) {
        if apply {
            if let Some(filter) = &editor.settings.filter {
                self.last_filter = Some(filter.clone());
            }
            let is_group = self.doc.layers[editor.layer].group.is_some();
            if is_group {
                // Snapshot the original hierarchy, never its transient preview.
                self.preview_pixels(editor.layer, &editor.original);
                self.checkpoint();
            }
            if let Some(pixels) = &editor.latest {
                self.preview_pixels(editor.layer, pixels);
            }
            let changed =
                self.doc.layers[editor.layer].pixels.as_slice() != editor.original.as_ref();
            if is_group && changed {
                std::sync::Arc::make_mut(&mut self.doc.layers[editor.layer].effects).push(
                    crate::layer_groups::GroupEffect {
                        settings: editor.settings.clone(),
                        mask: editor
                            .mask
                            .as_ref()
                            .map(|m| std::sync::Arc::new(m.values.clone())),
                        width: self.doc.width,
                        height: self.doc.height,
                    },
                );
                self.doc.damage = None;
                // The worker already produced the final composite. Keep it;
                // recompute the stack only when a child changes.
                self.refresh = true;
            } else if !is_group {
                self.history
                    .record_layer_pixels(&self.doc, editor.layer, &editor.original);
            }
            self.dirty = editor.was_dirty || changed;
            self.status = if changed {
                format!(
                    "{} aplicado à camada {}.",
                    editor.settings.label(),
                    self.doc.layers[self.doc.active].name
                )
            } else {
                "Nenhuma alteração de pixels.".into()
            };
        } else {
            if editor.preview_applied {
                self.preview_pixels(editor.layer, &editor.original);
            }
            self.dirty = editor.was_dirty;
            self.status = "Ajuste cancelado; pixels originais preservados.".into();
        }
    }
    pub(super) fn image_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.image_editor.take() else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        // Reuse the configurable apply/cancel commands; text editors retain Enter.
        if !ctx.wants_keyboard_input() {
            ctx.input_mut(|i| {
                cancel = self.settings.shortcuts.consume(i, Shortcut::Cancel);
                apply = self.settings.shortcuts.consume(i, Shortcut::Apply);
            });
        }
        match &mut state {
            Editor::Adjustment(editor) => {
                if let Some(result) = editor.job.as_ref().and_then(Job::poll) {
                    editor.job = None;
                    match result {
                        Ok(pixels) => {
                            editor.latest = Some(pixels.into());
                            if let Some(gallery) = &mut editor.gallery {
                                gallery.set_preview(
                                    ctx,
                                    if editor.preview {
                                        editor.latest.as_ref().unwrap()
                                    } else {
                                        &editor.original
                                    },
                                    self.doc.width as usize,
                                    self.doc.height as usize,
                                );
                            }
                            if editor.preview {
                                self.preview_pixels(editor.layer, editor.latest.as_ref().unwrap());
                                editor.preview_applied = true;
                                ctx.request_repaint();
                            }
                        }
                        Err(error) => editor.error = Some(error),
                    }
                }
                if editor.job.is_none() && editor.latest.is_none() && editor.error.is_none() {
                    if editor.edited.elapsed() >= Duration::from_millis(120) {
                        editor.start_job(ctx);
                    } else {
                        ctx.request_repaint_after(
                            Duration::from_millis(120)
                                - editor.edited.elapsed().min(Duration::from_millis(120)),
                        );
                    }
                }
                if editor.auto_apply && editor.latest.is_some() {
                    apply = true;
                }
                let ready = editor.latest.is_some() && editor.job.is_none();
                let mut open = true;
                let title = if editor.gallery.is_some() {
                    "Galeria de filtros"
                } else {
                    editor.settings.label()
                };
                effect_window(
                    ctx,
                    if editor.gallery.is_some() {
                        "filter_gallery"
                    } else {
                        "image_adjustment"
                    },
                    title,
                    editor.gallery.is_some(),
                )
                .open(&mut open)
                .show(ctx, |ui| {
                    crate::theme::caption(
                        ui,
                        format!(
                            "Camada: {}{} · {}",
                            self.doc.layers[editor.layer].name,
                            if editor.mask.is_some() {
                                " · área selecionada"
                            } else {
                                ""
                            },
                            if editor.settings.filter.as_ref().is_some_and(|f| f
                                .kind
                                .changes_alpha()
                                && !f.alpha_lock
                                && !f.preserve_alpha)
                            {
                                "transparência processada"
                            } else {
                                "alfa preservado"
                            }
                        ),
                    );
                    if self.doc.layers[editor.layer].group.is_some() {
                        crate::theme::caption(
                            ui,
                            "O ajuste fica no grupo; suas camadas internas continuam editáveis.",
                        );
                    }
                    ui.add_space(8.0);
                    let before = editor.settings.clone();
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 260.0).max(120.0))
                        .show(ui, |ui| {
                            if let Some(gallery) = &mut editor.gallery {
                                if let Some(filter) = &mut editor.settings.filter {
                                    gallery.show(ui, filter);
                                }
                            } else {
                                adjustment_controls(ui, &mut editor.settings);
                            }
                        });
                    if before != editor.settings {
                        editor.job = None;
                        editor.latest = None;
                        editor.error = None;
                        editor.edited = Instant::now();
                        ctx.request_repaint_after(Duration::from_millis(120));
                    }
                    let old_preview = editor.preview;
                    ui.checkbox(&mut editor.preview, "Pré-visualizar no canvas");
                    if old_preview != editor.preview {
                        if let Some(gallery) = &mut editor.gallery {
                            gallery.set_preview(
                                ctx,
                                if editor.preview {
                                    editor.latest.as_deref().unwrap_or(&editor.original)
                                } else {
                                    &editor.original
                                },
                                self.doc.width as usize,
                                self.doc.height as usize,
                            );
                        }
                        ctx.request_repaint();
                        if editor.preview {
                            if let Some(pixels) = &editor.latest {
                                self.preview_pixels(editor.layer, pixels);
                                editor.preview_applied = true;
                            }
                        } else if editor.preview_applied {
                            self.preview_pixels(editor.layer, &editor.original);
                            editor.preview_applied = false;
                        }
                    }
                    processing_status(
                        ui,
                        editor.job.is_some() || editor.latest.is_none() && editor.error.is_none(),
                        "Processando…",
                        editor.error.as_deref(),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                ready && before == editor.settings,
                                egui::Button::new("Aplicar"),
                            )
                            .clicked()
                        {
                            apply = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            cancel = true;
                        }
                        if ui.button("Restaurar").clicked() {
                            editor.settings.reset();
                            editor.job = None;
                            editor.latest = None;
                            editor.error = None;
                            editor.edited = Instant::now() - Duration::from_secs(1);
                        }
                    });
                });
                cancel |= !open;
                if !ready || editor.latest.is_none() {
                    apply = false;
                }
            }
            Editor::Geometry(editor) => {
                let mut open = true;
                effect_window(ctx, "image_geometry", editor.kind.label(), false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        crate::theme::caption(
                            ui,
                            "Afeta todas as camadas. A alteração pode ser desfeita.",
                        );
                        geometry_controls(ui, editor, &self.doc);
                        ui.horizontal(|ui| {
                            if ui.button("Aplicar").clicked() {
                                apply = true;
                            }
                            if ui.button("Cancelar").clicked() {
                                cancel = true;
                            }
                        });
                    });
                cancel |= !open;
            }
            Editor::Working { job, name } => {
                if let Some(result) = job.poll() {
                    match result {
                        Ok(doc) => {
                            self.history.push(&self.doc);
                            self.doc = doc;
                            self.history.commit(&self.doc);
                            self.selection = Default::default();
                            self.fit = true;
                            self.zoom_animation = None;
                            self.rotation_target = None;
                            self.rename = self.doc.layers[self.doc.active].name.clone();
                            self.changed();
                            self.status = format!("{name} concluído.");
                        }
                        Err(error) => self.status = format!("Erro ao alterar imagem: {error}"),
                    }
                    ctx.request_repaint();
                    return;
                }
                let mut open = true;
                effect_window(ctx, "image_geometry", name, false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        processing_status(ui, true, "Processando todas as camadas…", None);
                        if ui.button("Cancelar").clicked() {
                            cancel = true;
                        }
                    });
                cancel |= !open;
                apply = false;
            }
        }
        if cancel {
            ctx.request_repaint();
            if let Editor::Adjustment(editor) = state {
                self.finish_adjustment(*editor, false);
            } else {
                self.status = "Operação de imagem cancelada.".into();
            }
        } else if apply {
            ctx.request_repaint();
            match state {
                Editor::Adjustment(editor) => self.finish_adjustment(*editor, true),
                Editor::Geometry(editor) => {
                    if matches!(editor.kind, GeometryKind::Resize) && !editor.resample {
                        let changed = self.doc.dpi != editor.dpi;
                        self.history.push_layer_properties(&self.doc);
                        self.doc.dpi = editor.dpi;
                        self.history.commit(&self.doc);
                        self.dirty |= changed;
                        self.status =
                            "Resolução de impressão atualizada; pixels preservados.".into();
                        return;
                    }
                    let op = match editor.kind {
                        GeometryKind::Resize => {
                            if editor.resample {
                                Geometry::Resize {
                                    width: editor.width,
                                    height: editor.height,
                                    filter: filters()[editor.filter].1,
                                }
                            } else {
                                Geometry::Canvas {
                                    width: self.doc.width,
                                    height: self.doc.height,
                                    anchor: [1, 1],
                                }
                            }
                        }
                        GeometryKind::Canvas => Geometry::Canvas {
                            width: editor.width,
                            height: editor.height,
                            anchor: editor.anchor,
                        },
                        GeometryKind::Rotate => Geometry::Rotate(editor.angle),
                        GeometryKind::Trim => Geometry::Trim {
                            corner: editor.corner,
                            tolerance: editor.tolerance,
                            sides: editor.sides,
                        },
                    };
                    self.start_geometry(ctx, op, editor.kind.label(), Some(editor.dpi));
                }
                Editor::Working { .. } => {}
            }
        } else {
            self.image_editor = Some(state);
        }
    }
}
impl AdjustmentEditor {
    fn start_job(&mut self, ctx: &egui::Context) {
        let source = self.original.clone();
        let mask = self.mask.clone();
        let settings = self.settings.clone();
        match Job::start(ctx.clone(), move |cancel| {
            image_ops::adjust(&source, mask.as_deref(), &settings, cancel)
        }) {
            Ok(job) => self.job = Some(job),
            Err(error) => self.error = Some(error),
        }
    }
}
// Stable identity keeps the user's position when the title or processing state changes.
fn effect_window<'a>(
    ctx: &egui::Context,
    id: &str,
    title: &'a str,
    gallery: bool,
) -> egui::Window<'a> {
    egui::Window::new(title)
        .id(egui::Id::new(id))
        .default_pos(ctx.content_rect().min + egui::vec2(24.0, 110.0))
        .default_width(if gallery {
            (ctx.content_rect().width() - 80.0).clamp(480.0, 920.0)
        } else {
            380.0
        })
        .movable(true)
        .resizable(true)
        .collapsible(false)
}

// Always reserve the same rectangle, including idle and error states. Never let
// asynchronous completion move the slider under the user's pointer.
pub(super) fn processing_status(ui: &mut egui::Ui, busy: bool, label: &str, error: Option<&str>) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    let mut status = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    status.set_clip_rect(status.clip_rect().intersect(rect));
    status.horizontal_centered(|ui| {
        if let Some(error) = error {
            ui.add(
                egui::Label::new(egui::RichText::new(error).color(ui.visuals().error_fg_color))
                    .truncate(),
            )
            .on_hover_text(error);
        } else if busy {
            ui.spinner();
            ui.add(egui::Label::new(label).truncate());
        }
    });
}

fn slider(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, label: &str) {
    ui.add(egui::Slider::new(value, range).text(label));
}
fn adjustment_controls(ui: &mut egui::Ui, settings: &mut AdjustmentSettings) {
    let p = &mut settings.p;
    match settings.kind {
        Adjustment::Filter => {
            if let Some(filter) = &mut settings.filter {
                super::filter_ui::controls(ui, filter);
            }
        }
        Adjustment::BrightnessContrast => {
            slider(ui, &mut p[0], -100.0..=100.0, "Brilho");
            slider(ui, &mut p[1], -100.0..=100.0, "Contraste");
        }
        Adjustment::Levels => {
            slider(ui, &mut p[0], 0.0..=254.0, "Entrada: preto");
            slider(ui, &mut p[1], 1.0..=255.0, "Entrada: branco");
            if p[0] >= p[1] {
                p[1] = p[0] + 1.0;
            }
            slider(ui, &mut p[2], 0.1..=5.0, "Gama");
            slider(ui, &mut p[3], 0.0..=255.0, "Saída: preto");
            slider(ui, &mut p[4], 0.0..=255.0, "Saída: branco");
        }
        Adjustment::Curves => {
            egui::ComboBox::from_id_salt("curve_channel")
                .selected_text(["RGB", "Vermelho", "Verde", "Azul"][p[5] as usize])
                .show_ui(ui, |ui| {
                    for (i, label) in ["RGB", "Vermelho", "Verde", "Azul"].iter().enumerate() {
                        ui.selectable_value(&mut p[5], i as f32, *label);
                    }
                });
            curve_editor(ui, settings);
        }

        Adjustment::Exposure => {
            slider(ui, &mut p[0], -10.0..=10.0, "Exposição (EV)");
            slider(ui, &mut p[1], -0.5..=0.5, "Deslocamento");
            slider(ui, &mut p[2], 0.1..=5.0, "Gama");
        }
        Adjustment::Vibrance => {
            slider(ui, &mut p[0], -100.0..=100.0, "Vibração");
            slider(ui, &mut p[1], -100.0..=100.0, "Saturação");
        }
        Adjustment::HueSaturation => {
            slider(ui, &mut p[0], -180.0..=180.0, "Matiz");
            slider(ui, &mut p[1], -100.0..=100.0, "Saturação");
            slider(ui, &mut p[2], -100.0..=100.0, "Luminosidade");
        }
        Adjustment::ColorBalance => {
            egui::ComboBox::from_id_salt("balance_range")
                .selected_text(["Sombras", "Meios-tons", "Realces"][p[3] as usize])
                .show_ui(ui, |ui| {
                    for (i, label) in ["Sombras", "Meios-tons", "Realces"].iter().enumerate() {
                        ui.selectable_value(&mut p[3], i as f32, *label);
                    }
                });
            slider(ui, &mut p[0], -100.0..=100.0, "Ciano ↔ vermelho");
            slider(ui, &mut p[1], -100.0..=100.0, "Magenta ↔ verde");
            slider(ui, &mut p[2], -100.0..=100.0, "Amarelo ↔ azul");
            ui.checkbox(&mut settings.preserve_luminosity, "Preservar luminosidade");
        }
        Adjustment::BlackWhite => {
            for (i, label) in ["Vermelho", "Verde", "Azul"].iter().enumerate() {
                slider(ui, &mut p[i], -100.0..=200.0, label);
            }
        }
        Adjustment::PhotoFilter => {
            ui.horizontal(|ui| {
                ui.label("Cor do filtro");
                ui.color_edit_button_srgb(&mut settings.colors[0]);
            });
            slider(ui, &mut p[0], 0.0..=100.0, "Densidade");
            ui.checkbox(&mut settings.preserve_luminosity, "Preservar luminosidade");
        }
        Adjustment::ChannelMixer => {
            for (row, label) in ["Saída vermelha", "Saída verde", "Saída azul"]
                .iter()
                .enumerate()
            {
                ui.label(*label);
                for (col, label) in ["Vermelho", "Verde", "Azul"].iter().enumerate() {
                    slider(ui, &mut p[row * 3 + col], -200.0..=200.0, label);
                }
            }
        }
        Adjustment::Posterize => {
            ui.add(
                egui::Slider::new(&mut p[0], 2.0..=64.0)
                    .integer()
                    .text("Níveis por canal"),
            );
        }
        Adjustment::Threshold => {
            slider(ui, &mut p[0], 0.0..=255.0, "Limiar");
        }
        Adjustment::GradientMap => {
            for (i, label) in ["Sombras", "Realces"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(*label);
                    ui.color_edit_button_srgb(&mut settings.colors[i]);
                });
            }
        }
        Adjustment::ShadowsHighlights => {
            slider(ui, &mut p[0], 0.0..=100.0, "Clarear sombras");
            slider(ui, &mut p[1], 0.0..=100.0, "Recuperar realces");
        }
        Adjustment::ReplaceColor => {
            for (i, label) in ["Cor de origem", "Nova cor"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(*label);
                    ui.color_edit_button_srgb(&mut settings.colors[i]);
                });
            }
            slider(ui, &mut p[0], 0.1..=100.0, "Tolerância");
            slider(ui, &mut p[1], 0.0..=100.0, "Intensidade");
        }
        _ => {}
    }
}
fn curve_editor(ui: &mut egui::Ui, settings: &mut AdjustmentSettings) {
    crate::theme::caption(
        ui,
        "Arraste os pontos ou clique na curva para adicionar. Clique direito remove um ponto.",
    );
    let nodes = |s: &AdjustmentSettings| {
        if s.curve_points.is_empty() {
            (0..5).map(|i| [i as f32 / 4.0, s.p[i]]).collect::<Vec<_>>()
        } else {
            s.curve_points.clone()
        }
    };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 220.0),
        Sense::click_and_drag(),
    );
    let graph = rect.shrink(9.0);
    let to_screen = |p: [f32; 2]| {
        Pos2::new(
            graph.left() + graph.width() * p[0],
            graph.bottom() - graph.height() * p[1],
        )
    };
    let active_id = response.id.with("curve_drag_point");
    let mut points = nodes(settings);
    let mut active = ui.ctx().data(|d| d.get_temp::<usize>(active_id));
    if let Some(pos) = response.interact_pointer_pos() {
        let nearest = points
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                to_screen(**a)
                    .distance_sq(pos)
                    .total_cmp(&to_screen(**b).distance_sq(pos))
            })
            .map(|(i, p)| (i, to_screen(*p).distance(pos)));
        if response.secondary_clicked() {
            if let Some((i, distance)) = nearest
                && distance < 16.0
                && i > 0
                && i + 1 < points.len()
            {
                points.remove(i);
                settings.curve_points = points.clone();
                active = None;
            }
        } else {
            if response.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed())
                || response.clicked()
            {
                active = nearest
                    .filter(|(_, distance)| *distance < 16.0)
                    .map(|(i, _)| i);
                if active.is_none() && points.len() < 16 && graph.contains(pos) {
                    let x = ((pos.x - graph.left()) / graph.width()).clamp(0.001, 0.999);
                    let y = ((graph.bottom() - pos.y) / graph.height()).clamp(0.0, 1.0);
                    let i = points.partition_point(|p| p[0] < x);
                    if i > 0
                        && i < points.len()
                        && x - points[i - 1][0] > 0.001
                        && points[i][0] - x > 0.001
                    {
                        points.insert(i, [x, y]);
                        active = Some(i);
                        settings.curve_points = points.clone();
                    }
                }
            }
            if response.dragged()
                && let Some(i) = active.filter(|&i| i < points.len())
            {
                if i > 0 && i + 1 < points.len() {
                    points[i][0] = ((pos.x - graph.left()) / graph.width())
                        .clamp(points[i - 1][0] + 0.0001, points[i + 1][0] - 0.0001);
                }
                points[i][1] = ((graph.bottom() - pos.y) / graph.height()).clamp(0.0, 1.0);
                settings.curve_points = points.clone();
            }
        }
    }
    ui.ctx().data_mut(|d| {
        if let Some(i) = active {
            d.insert_temp(active_id, i);
        } else {
            d.remove::<usize>(active_id);
        }
    });
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, Color32::from_gray(20));
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        painter.line_segment(
            [to_screen([t, 0.0]), to_screen([t, 1.0])],
            Stroke::new(1.0_f32, Color32::from_gray(45)),
        );
        painter.line_segment(
            [to_screen([0.0, t]), to_screen([1.0, t])],
            Stroke::new(1.0_f32, Color32::from_gray(45)),
        );
    }
    painter.line_segment(
        [to_screen([0.0, 0.0]), to_screen([1.0, 1.0])],
        Stroke::new(1.0_f32, Color32::from_gray(70)),
    );
    let curve = (0..=256)
        .map(|i| {
            let x = i as f32 / 256.0;
            to_screen([x, image_ops::curve_value(settings, x)])
        })
        .collect();
    painter.add(egui::Shape::line(
        curve,
        Stroke::new(2.0_f32, crate::theme::accent(ui.ctx())),
    ));
    for (i, p) in points.iter().enumerate() {
        painter.circle_filled(
            to_screen(*p),
            5.0,
            if active == Some(i) {
                Color32::WHITE
            } else {
                crate::theme::accent(ui.ctx())
            },
        );
    }
    egui::CollapsingHeader::new("Ajustar com sliders").show(ui, |ui| {
        let mut points = nodes(settings);
        let count = points.len();
        for i in 0..count {
            let low = if i > 0 {
                points[i - 1][0] + 0.0001
            } else {
                0.0
            };
            let high = if i + 1 < count {
                points[i + 1][0] - 0.0001
            } else {
                1.0
            };
            ui.push_id(i, |ui| {
                ui.label(format!("Ponto {}", i + 1));
                let mut changed = false;
                ui.add_enabled_ui(i > 0 && i + 1 < count, |ui| {
                    changed |= ui
                        .add(egui::Slider::new(&mut points[i][0], low..=high).text("Entrada"))
                        .changed();
                });
                changed |= ui
                    .add(egui::Slider::new(&mut points[i][1], 0.0..=1.0).text("Saída"))
                    .changed();
                if changed {
                    settings.curve_points = points.clone();
                }
            });
        }
    });
}

fn filters() -> [(&'static str, image::imageops::FilterType); 4] {
    use image::imageops::FilterType::*;
    [
        ("Vizinho mais próximo (pixel art)", Nearest),
        ("Bilinear", Triangle),
        ("Bicúbica", CatmullRom),
        ("Lanczos", Lanczos3),
    ]
}
fn geometry_controls(ui: &mut egui::Ui, editor: &mut GeometryEditor, doc: &Document) {
    match editor.kind {
        GeometryKind::Resize | GeometryKind::Canvas => {
            let old_w = editor.width;
            let old_h = editor.height;
            ui.add_enabled_ui(
                !matches!(editor.kind, GeometryKind::Resize) || editor.resample,
                |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Largura");
                        ui.add(
                            egui::DragValue::new(&mut editor.width)
                                .range(1..=MAX_SIDE)
                                .suffix(" px"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Altura");
                        ui.add(
                            egui::DragValue::new(&mut editor.height)
                                .range(1..=MAX_SIDE)
                                .suffix(" px"),
                        );
                    });
                    if matches!(editor.kind, GeometryKind::Resize) {
                        ui.checkbox(&mut editor.proportion, "Manter proporção");
                        if editor.proportion {
                            if editor.width != old_w {
                                editor.height = (editor.width as f64 * doc.height as f64
                                    / doc.width as f64)
                                    .round()
                                    .clamp(1.0, MAX_SIDE as f64)
                                    as u32;
                            } else if editor.height != old_h {
                                editor.width = (editor.height as f64 * doc.width as f64
                                    / doc.height as f64)
                                    .round()
                                    .clamp(1.0, MAX_SIDE as f64)
                                    as u32;
                            }
                        }
                    }
                },
            );
            if matches!(editor.kind, GeometryKind::Resize) {
                ui.horizontal(|ui| {
                    ui.label("Resolução");
                    ui.add(
                        egui::DragValue::new(&mut editor.dpi)
                            .range(1..=crate::document::MAX_DPI)
                            .suffix(" DPI"),
                    );
                });
                ui.checkbox(&mut editor.resample, "Reamostrar pixels");
                ui.add_enabled_ui(editor.resample, |ui| {
                    egui::ComboBox::from_id_salt("resize_filter")
                        .selected_text(filters()[editor.filter].0)
                        .show_ui(ui, |ui| {
                            for (i, (label, _)) in filters().iter().enumerate() {
                                ui.selectable_value(&mut editor.filter, i, *label);
                            }
                        });
                });
                let (w, h) = if editor.resample {
                    (editor.width, editor.height)
                } else {
                    (doc.width, doc.height)
                };
                crate::theme::caption(
                    ui,
                    format!(
                        "Impressão: {:.2} × {:.2} cm",
                        w as f64 / editor.dpi.max(1) as f64 * 2.54,
                        h as f64 / editor.dpi.max(1) as f64 * 2.54
                    ),
                );
                crate::theme::caption(
                    ui,
                    "Sem reamostragem, apenas o DPI muda; os pixels são preservados.",
                );
            } else {
                ui.label("Âncora do conteúdo");
                egui::Grid::new("canvas_anchor").show(ui, |ui| {
                    for y in 0..3 {
                        for x in 0..3 {
                            if ui
                                .selectable_label(
                                    editor.anchor == [x, y],
                                    [["↖", "↑", "↗"], ["←", "•", "→"], ["↙", "↓", "↘"]][y as usize]
                                        [x as usize],
                                )
                                .clicked()
                            {
                                editor.anchor = [x, y];
                            }
                        }
                        ui.end_row();
                    }
                });
                crate::theme::caption(
                    ui,
                    "A área nova é transparente. Reduzir a tela corta os pixels fora dela.",
                );
            }
            let bytes = editor.width as u64 * editor.height as u64 * 4 * doc.layers.len() as u64;
            if bytes > 256 * 1024 * 1024 {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "O resultado excede 256 MiB. Reduza as dimensões ou o número de camadas.",
                );
            }
        }
        GeometryKind::Rotate => {
            ui.add(
                egui::Slider::new(&mut editor.angle, -180.0..=180.0)
                    .suffix("°")
                    .text("Ângulo horário"),
            );
            crate::theme::caption(
                ui,
                "A tela expande para acomodar a imagem; os cantos ficam transparentes.",
            );
        }
        GeometryKind::Trim => {
            egui::ComboBox::from_id_salt("trim_mode")
                .selected_text(
                    [
                        "Pixels transparentes",
                        "Cor do canto superior esquerdo",
                        "Cor do canto inferior direito",
                    ][editor.corner as usize],
                )
                .show_ui(ui, |ui| {
                    for (i, label) in [
                        "Pixels transparentes",
                        "Cor do canto superior esquerdo",
                        "Cor do canto inferior direito",
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.selectable_value(&mut editor.corner, i as u8, *label);
                    }
                });
            ui.add(egui::Slider::new(&mut editor.tolerance, 0..=64).text("Tolerância"));
            ui.horizontal_wrapped(|ui| {
                for (i, label) in ["Esquerda", "Topo", "Direita", "Base"].iter().enumerate() {
                    ui.checkbox(&mut editor.sides[i], *label);
                }
            });
            crate::theme::caption(
                ui,
                "Os limites são calculados pela composição das camadas visíveis.",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app(ctx: &egui::Context) -> PaintAtlas {
        let mut app = PaintAtlas::from_context(ctx);
        app.reset(Document::new(8, 8, false), None, false);
        app.doc.layers[0].pixels = [30, 100, 200, 128].repeat(64);
        app
    }
    fn finish_job(app: &mut PaintAtlas, ctx: &egui::Context) {
        for _ in 0..300 {
            let _ = ctx.run(Default::default(), |ctx| app.image_dialog(ctx));
            if app.image_editor.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("Image operation did not finish");
    }
    #[test]
    fn automatic_adjustment_is_one_undo_step_and_respects_selection() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let original = app.doc.layers[0].pixels.clone();
        app.selection.mask = crate::selection::Mask::rectangle(
            8,
            8,
            egui::Rect::from_min_size(Pos2::ZERO, Vec2::splat(2.0)),
        )
        .map(Arc::new);
        app.doc.selection = app.selection.mask.clone();
        app.open_adjustment(&ctx, Adjustment::Invert);
        finish_job(&mut app, &ctx);
        assert_eq!(&app.doc.layers[0].pixels[..4], &[225, 155, 55, 128]);
        assert_eq!(app.doc.layers[0].pixels[8..32], original[8..32]);
        assert_eq!(app.history.counts(), (1, 0));
        assert!(app.dirty);
        assert!(app.selection.mask.is_some());
        app.undo();
        assert_eq!(app.doc.layers[0].pixels, original);
        app.redo();
        assert_eq!(&app.doc.layers[0].pixels[..4], &[225, 155, 55, 128]);
    }
    #[test]
    fn cancel_preview_restores_original_dirty_state_and_preserves_redo() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        app.history.push_layer_properties(&app.doc);
        app.doc.layers[0].opacity = 0.5;
        app.history.commit(&app.doc);
        app.history.undo(&mut app.doc);
        app.dirty = false;
        let original = app.doc.layers[0].pixels.clone();
        app.open_adjustment(&ctx, Adjustment::BrightnessContrast);
        assert!(!app.save(false));
        assert!(!app.document_change_allowed());
        let Some(Editor::Adjustment(editor)) = app.image_editor.take() else {
            panic!()
        };
        let mut editor = *editor;
        editor.job = None;
        editor.settings.p[0] = 20.0;
        let pixels = image_ops::adjust(
            &editor.original,
            None,
            &editor.settings,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        app.preview_pixels(0, &pixels);
        editor.preview_applied = true;
        assert_ne!(app.doc.layers[0].pixels, original);
        app.finish_adjustment(editor, false);
        assert_eq!(app.doc.layers[0].pixels, original);
        assert!(!app.dirty);
        assert_eq!(app.history.counts(), (0, 1));
    }
    #[test]
    fn geometry_history_restores_all_layers_dimensions_and_dpi() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        app.doc.add_layer();
        app.rename = app.doc.layers[1].name.clone();
        let original = app.doc.layers[0].pixels.clone();
        app.start_geometry(
            &ctx,
            Geometry::Canvas {
                width: 12,
                height: 10,
                anchor: [1, 1],
            },
            "Tamanho da tela",
            Some(600),
        );
        finish_job(&mut app, &ctx);
        assert_eq!((app.doc.width, app.doc.height, app.doc.dpi), (12, 10, 600));
        assert_eq!(app.doc.layers.len(), 2);
        assert_eq!(app.history.counts(), (1, 0));
        app.undo();
        assert_eq!((app.doc.width, app.doc.height, app.doc.dpi), (8, 8, 300));
        assert_eq!(app.doc.layers[0].pixels, original);
        app.redo();
        assert_eq!((app.doc.width, app.doc.height, app.doc.dpi), (12, 10, 600));
    }
    #[test]
    fn metadata_only_dpi_change_has_compact_reversible_history() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let original = app.doc.layers[0].pixels.clone();
        app.history.push_layer_properties(&app.doc);
        app.doc.dpi = 600;
        app.history.commit(&app.doc);
        assert_eq!(app.history.counts(), (1, 0));
        app.undo();
        assert_eq!(app.doc.dpi, 300);
        app.redo();
        assert_eq!(app.doc.dpi, 600);
        assert_eq!(app.doc.layers[0].pixels, original);
    }
    #[test]
    fn custom_image_shortcut_is_honored_and_default_can_be_disabled() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        let mut binding = crate::shortcuts::Binding::from_modifiers(egui::Modifiers::CTRL);
        binding.key = Some("J".into());
        assert!(
            app.settings
                .shortcuts
                .assign(Shortcut::ImageLevels, 0, binding, true)
        );
        for key in [egui::Key::L, egui::Key::J] {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::CTRL,
                    }],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
            assert_eq!(app.image_editor.is_some(), key == egui::Key::J);
        }
    }
    #[test]
    fn processing_does_not_move_controls_or_resize_window() {
        let ctx = egui::Context::default();
        let mut baseline = None;
        for (busy, error) in [
            (false, None),
            (true, None),
            (false, Some("A long processing error ".repeat(30))),
        ] {
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        let mut value = 0.5;
                        let mut slider_rect = egui::Rect::NOTHING;
                        let mut button_rect = egui::Rect::NOTHING;
                        let response = effect_window(ctx, "stable_effect_test", "Filtro", false)
                            .show(ctx, |ui| {
                                slider_rect = ui.add(egui::Slider::new(&mut value, 0.0..=1.0)).rect;
                                processing_status(ui, busy, "Processando…", error.as_deref());
                                button_rect = ui.button("Aplicar").rect;
                            })
                            .unwrap();
                        let layout = (response.response.rect, slider_rect, button_rect);
                        if let Some(expected) = baseline {
                            assert_eq!(layout, expected);
                        }
                        // The first frames let egui establish the window's measured size.
                        if baseline.is_none() && ctx.cumulative_frame_nr() >= 2 {
                            baseline = Some(layout);
                        }
                        assert!(ctx.memory(|memory| memory.top_modal_layer().is_none()));
                    },
                );
            }
        }
    }
    #[test]
    fn group_adjustment_preserves_editable_children_and_undo_restores_effects() {
        let ctx = egui::Context::default();
        let mut app = app(&ctx);
        app.doc.add_layer();
        app.doc.layers[1].pixels = [30, 80, 180, 220].repeat(64);
        app.doc.group_layers(&[0, 1]);
        app.history = History::new();
        let original = app.doc.composite();
        let children = [
            app.doc.layers[0].pixels.clone(),
            app.doc.layers[1].pixels.clone(),
        ];
        for apply in [false, true] {
            app.open_adjustment(&ctx, Adjustment::Invert);
            let Some(Editor::Adjustment(mut editor)) = app.image_editor.take() else {
                panic!("editor");
            };
            let result = image_ops::adjust(
                &editor.original,
                None,
                &editor.settings,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
            editor.latest = Some(result.clone().into());
            app.preview_pixels(editor.layer, &result);
            editor.preview_applied = true;
            app.finish_adjustment(*editor, apply);
            app.history.commit(&app.doc);
            assert!(app.doc.validate_groups().is_ok());
            if !apply {
                assert_eq!(app.doc.layers.len(), 3);
                assert_eq!(app.doc.composite(), original);
            } else {
                assert_eq!(app.doc.layers.len(), 3);
                assert_ne!(app.doc.composite(), original);
                assert_eq!(app.doc.layers[2].effects.len(), 1);
                assert_eq!(app.doc.layers[0].pixels, children[0]);
                assert_eq!(app.doc.layers[1].pixels, children[1]);
                let before_edit = app.doc.composite();
                let child = app.doc.layers[0].pixels.clone();
                app.doc.layers[0].pixels[..4].copy_from_slice(&[255, 255, 255, 255]);
                app.doc.rebuild_groups_after(Some([0, 0, 1, 1]), Some(0));
                assert_ne!(app.doc.composite(), before_edit);
                app.doc.layers[0].pixels = child;
                app.doc.rebuild_groups(None);
                app.undo();
                assert_eq!(app.doc.layers.len(), 3);
                assert_eq!(app.doc.composite(), original);
                app.redo();
                assert_eq!(app.doc.layers.len(), 3);
                assert!(app.doc.validate_groups().is_ok());
                app.select_layer(0, false, false);
                assert_eq!(app.doc.active, 0);
                assert_eq!(app.selected_layer_indices(), vec![0]);
                app.apply_layer_action(2, super::super::layer_menu::LayerAction::ClearEffects);
                app.history.commit(&app.doc);
                assert_eq!(app.doc.composite(), original);
                app.undo();
                assert_eq!(app.doc.layers[2].effects.len(), 1);
            }
        }
    }
    #[test]
    fn dragging_a_curve_control_point_changes_the_real_mapping() {
        let ctx = egui::Context::default();
        let mut settings = AdjustmentSettings::new(Adjustment::Curves);
        let mut center = Pos2::ZERO;
        let render =
            |events: Vec<egui::Event>, settings: &mut AdjustmentSettings, center: &mut Pos2| {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(420.0, 500.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| curve_editor(ui, settings));
                    },
                );
                let points: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Circle(circle) if circle.radius == 5.0 => Some(circle.center),
                        _ => None,
                    })
                    .collect();
                if points.len() >= 5 {
                    *center = points[2];
                }
            };
        render(vec![], &mut settings, &mut center);
        let start = center;
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        render(
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
            &mut settings,
            &mut center,
        );
        render(
            vec![egui::Event::PointerMoved(start - Vec2::new(20.0, 50.0))],
            &mut settings,
            &mut center,
        );
        render(
            vec![pointer(start - Vec2::new(20.0, 50.0), false)],
            &mut settings,
            &mut center,
        );
        assert_eq!(settings.curve_points.len(), 5);
        assert!(settings.curve_points[2][0] < 0.5);
        assert!(settings.curve_points[2][1] > 0.7);
        assert!(image_ops::curve_value(&settings, 0.5) > 0.7);
    }
}
