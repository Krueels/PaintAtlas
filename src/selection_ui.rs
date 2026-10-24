use super::*;
use crate::selection::{Combine, Floating, Mask};
use std::sync::Arc;

pub(super) struct Creation {
    points: Vec<Pos2>,
    lasso: bool,
    mode: Combine,
}
struct Drag {
    point: Pos2,
    bounds: egui::Rect,
    handle: Option<usize>,
    angle: f32,
    rotating: bool,
}
pub(super) struct State {
    pub mask: Option<Arc<Mask>>,
    pub floating: Option<Floating>,
    creation: Option<Creation>,
    drag: Option<Drag>,
    pub(super) textures: Option<(egui::TextureHandle, egui::TextureHandle)>,
    toolbar: Option<egui::Rect>,
    pub mode: Combine,
    pub keep_ratio: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            mask: None,
            floating: None,
            creation: None,
            drag: None,
            textures: None,
            toolbar: None,
            mode: Combine::Replace,
            keep_ratio: true,
        }
    }
}
fn corners(r: egui::Rect) -> [Pos2; 4] {
    [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
    ]
}
fn handles(r: egui::Rect) -> [Pos2; 8] {
    [
        r.left_top(),
        r.center_top(),
        r.right_top(),
        r.right_center(),
        r.right_bottom(),
        r.center_bottom(),
        r.left_bottom(),
        r.left_center(),
    ]
}
fn screen(view: CanvasView, p: Pos2) -> Pos2 {
    view.screen([p.x, p.y])
}
fn hit_handle(view: CanvasView, f: &Floating, p: Pos2) -> Option<usize> {
    handles(f.bounds)
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            screen(view, f.to_world(**a))
                .distance_sq(p)
                .total_cmp(&screen(view, f.to_world(**b)).distance_sq(p))
        })
        .filter(|(_, q)| screen(view, f.to_world(**q)).distance(p) <= 10.0)
        .map(|(i, _)| i)
}
fn hit_rotation(view: CanvasView, f: &Floating, p: Pos2) -> bool {
    !f.bounds.contains(f.to_local(Pos2::from(view.document(p))))
        && handles(f.bounds)
            .iter()
            .any(|q| screen(view, f.to_world(*q)).distance(p) <= 26.0)
        && hit_handle(view, f, p).is_none()
}
fn resized(initial: egui::Rect, handle: usize, delta: Vec2, keep_ratio: bool) -> egui::Rect {
    let mut min = initial.min;
    let mut max = initial.max;
    if [0, 6, 7].contains(&handle) {
        min.x = (min.x + delta.x).min(max.x - 1.0);
    }
    if [2, 3, 4].contains(&handle) {
        max.x = (max.x + delta.x).max(min.x + 1.0);
    }
    if [0, 1, 2].contains(&handle) {
        min.y = (min.y + delta.y).min(max.y - 1.0);
    }
    if [4, 5, 6].contains(&handle) {
        max.y = (max.y + delta.y).max(min.y + 1.0);
    }
    if keep_ratio && handle.is_multiple_of(2) {
        let sx = (max.x - min.x) / initial.width();
        let sy = (max.y - min.y) / initial.height();
        let scale = if (sx - 1.0).abs() > (sy - 1.0).abs() {
            sx
        } else {
            sy
        };
        let scale = scale.max(1.0 / initial.width().min(initial.height()));
        if [0, 6].contains(&handle) {
            min.x = max.x - initial.width() * scale;
        } else {
            max.x = min.x + initial.width() * scale;
        }
        if [0, 2].contains(&handle) {
            min.y = max.y - initial.height() * scale;
        } else {
            max.y = min.y + initial.height() * scale;
        }
    }
    egui::Rect::from_min_max(min, max)
}
fn clipped(a: Pos2, b: Pos2, clip: egui::Rect) -> Option<(Pos2, Pos2)> {
    let delta = b - a;
    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    for (origin, direction, min, max) in [
        (a.x, delta.x, clip.min.x, clip.max.x),
        (a.y, delta.y, clip.min.y, clip.max.y),
    ] {
        if direction.abs() < 0.00001 {
            if origin < min || origin > max {
                return None;
            }
        } else {
            let x = (min - origin) / direction;
            let y = (max - origin) / direction;
            lo = lo.max(x.min(y));
            hi = hi.min(x.max(y));
            if lo > hi {
                return None;
            }
        }
    }
    Some((a + delta * lo, b - delta * (1.0 - hi)))
}
fn quad(
    view: CanvasView,
    f: &Floating,
    texture: egui::TextureId,
    color: Color32,
    flip_x: bool,
    flip_y: bool,
) -> egui::Mesh {
    let mut mesh = egui::Mesh::with_texture(texture);
    for (p, mut uv) in corners(f.bounds).map(|p| f.to_world(p)).into_iter().zip([
        Pos2::ZERO,
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ]) {
        if flip_x {
            uv.x = 1.0 - uv.x;
        }
        if flip_y {
            uv.y = 1.0 - uv.y;
        }
        mesh.vertices.push(egui::epaint::Vertex {
            pos: screen(view, p),
            uv,
            color,
        });
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    mesh
}
impl PaintAtlas {
    pub(super) fn clear_selection(&mut self) {
        self.selection = State::default();
        self.doc.selection = None;
    }
    pub(super) fn begin_transform(&mut self, ctx: &egui::Context, duplicate: bool) {
        if self.selection.floating.is_some() {
            return;
        }
        let Some(mask) = &self.selection.mask else {
            self.status = "Crie uma seleção primeiro (S ou A).".into();
            return;
        };
        match Floating::new(&self.doc, mask, duplicate) {
            Ok(floating) => {
                self.start_floating(ctx, floating);
                self.status = format!(
                    "Arraste para mover; use as alças para redimensionar ou girar. {} aplica; {} cancela.",
                    self.settings.shortcuts.label(Shortcut::Apply),
                    self.settings.shortcuts.label(Shortcut::Cancel)
                );
            }
            Err(e) => self.status = e,
        }
    }
    pub(super) fn start_floating(&mut self, ctx: &egui::Context, floating: Floating) {
        self.layer_motion = None;
        self.layer_row_slides.clear();
        let base = ctx.load_texture(
            "selection_base",
            egui::ColorImage::from_rgba_unmultiplied(
                [self.doc.width as usize, self.doc.height as usize],
                &floating.base,
            ),
            egui::TextureOptions::LINEAR,
        );
        let cut = ctx.load_texture(
            "selection_cut",
            egui::ColorImage::from_rgba_unmultiplied(floating.size, &floating.pixels),
            egui::TextureOptions::LINEAR,
        );
        self.selection.textures = Some((base, cut));
        self.selection.floating = Some(floating);
        self.selection.drag = None;
        self.selection.toolbar = None;
    }
    pub(super) fn creating_selection(&self) -> bool {
        self.selection.creation.is_some()
    }
    pub(super) fn cancel_transform(&mut self) {
        self.selection.floating = None;
        self.selection.textures = None;
        self.selection.drag = None;
        self.status = "Transformação cancelada; o desenho original foi preservado.".into();
    }
    pub(super) fn apply_transform(&mut self) {
        if self
            .selection
            .floating
            .as_ref()
            .is_some_and(|f| f.new_layer)
            && !self.doc.can_add()
        {
            self.status = "Limite de camadas ou de memória atingido.".into();
            return;
        }
        let Some(floating) = self.selection.floating.take() else {
            return;
        };
        if floating.unchanged() {
            self.selection.textures = None;
            self.selection.drag = None;
            self.status = "Transformação sem alterações.".into();
            return;
        }
        let (pixels, mask) = floating.rasterize(self.doc.width, self.doc.height);
        if floating.new_layer
            && (mask.is_none() || !pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0))
        {
            self.selection.floating = Some(floating);
            self.status = format!(
                "A imagem está fora do canvas. Mova-a para dentro ou use {} para cancelar.",
                self.settings.shortcuts.label(Shortcut::Cancel)
            );
            return;
        }
        self.selection.textures = None;
        self.selection.drag = None;
        self.checkpoint();
        if floating.new_layer {
            self.doc.add_layer();
            self.doc.layers[self.doc.active].name = floating
                .name
                .clone()
                .unwrap_or_else(|| "Imagem colada".into());
            self.rename = self.doc.layers[self.doc.active].name.clone();
        }
        self.doc.layers[floating.layer].pixels = pixels;
        self.selection.mask = mask;
        self.doc.selection = self.selection.mask.clone();
        self.changed();
        self.status = format!(
            "{} · {} desfaz.",
            if floating.new_layer {
                "Imagem colada em nova camada"
            } else {
                "Transformação aplicada"
            },
            self.settings.shortcuts.label(Shortcut::Undo)
        );
    }
    fn delete_selected(&mut self) {
        let Some(mask) = self.selection.mask.clone() else {
            return;
        };
        if self.doc.layers[self.doc.active].alpha_lock {
            self.status = "Desative Bloquear transparência para apagar a seleção.".into();
            return;
        }
        if !self.doc.layers[self.doc.active].visible {
            self.status = "Ative a visibilidade da camada para editar.".into();
            return;
        }
        self.checkpoint();
        for (pixel, &coverage) in self.doc.layers[self.doc.active]
            .pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&mask.values)
        {
            pixel[3] = (pixel[3] as f32 * (1.0 - coverage as f32 / 255.0)).round() as u8;
            if pixel[3] == 0 {
                pixel[..3].fill(0);
            }
        }
        self.changed();
    }
    pub(super) fn selection_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if self.preferences.capturing_shortcut() {
            return true;
        }
        if self.selection.creation.is_some() {
            if ctx.input_mut(|i| self.settings.shortcuts.consume(i, Shortcut::Cancel)) {
                self.selection.creation = None;
            }
            return true;
        }
        if self.selection.floating.is_some() {
            let (apply, cancel, delta) = ctx.input_mut(|i| {
                let apply = self.settings.shortcuts.consume(i, Shortcut::Apply);
                let cancel = self.settings.shortcuts.consume(i, Shortcut::Cancel)
                    || self.settings.shortcuts.consume(i, Shortcut::Undo);
                let mut delta = Vec2::ZERO;
                for (action, d) in [
                    (Shortcut::MoveLeft, Vec2::new(-1.0, 0.0)),
                    (Shortcut::MoveRight, Vec2::new(1.0, 0.0)),
                    (Shortcut::MoveUp, Vec2::new(0.0, -1.0)),
                    (Shortcut::MoveDown, Vec2::new(0.0, 1.0)),
                    (Shortcut::FastLeft, Vec2::new(-10.0, 0.0)),
                    (Shortcut::FastRight, Vec2::new(10.0, 0.0)),
                    (Shortcut::FastUp, Vec2::new(0.0, -10.0)),
                    (Shortcut::FastDown, Vec2::new(0.0, 10.0)),
                ] {
                    if self.settings.shortcuts.consume(i, action) {
                        delta += d;
                    }
                }
                (apply, cancel, delta)
            });
            if apply {
                self.apply_transform();
            } else if cancel {
                self.cancel_transform();
            } else if let Some(f) = &mut self.selection.floating {
                f.bounds = f.bounds.translate(delta);
            }
            return true;
        }
        let action = ctx.input_mut(|i| {
            if self.settings.shortcuts.consume(i, Shortcut::Cancel) {
                1
            } else if self.settings.shortcuts.consume(i, Shortcut::Deselect) {
                2
            } else if self.settings.shortcuts.consume(i, Shortcut::SelectAll) {
                3
            } else if self.settings.shortcuts.consume(i, Shortcut::Transform) {
                4
            } else if self.selection.mask.is_some()
                && self
                    .settings
                    .shortcuts
                    .consume(i, Shortcut::DeleteSelection)
            {
                5
            } else {
                0
            }
        });
        match action {
            1 => {
                if self.selection.creation.is_some() {
                    self.selection.creation = None;
                } else {
                    self.clear_selection();
                }
            }
            2 => self.clear_selection(),
            3 => {
                self.selection.mask = Mask::rectangle(
                    self.doc.width,
                    self.doc.height,
                    egui::Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(self.doc.width as f32, self.doc.height as f32),
                    ),
                )
                .map(Arc::new);
                self.doc.selection = self.selection.mask.clone();
            }
            4 => self.begin_transform(ctx, false),
            5 => self.delete_selected(),
            _ => {}
        }
        action != 0
    }
    pub(super) fn selection_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        view: CanvasView,
        panning: bool,
    ) -> bool {
        let ctx = ui.ctx();
        if self.preferences.capturing_shortcut() {
            return true;
        }
        let (pointer, pressed, down, released, proportion, snap, add, subtract) = ctx.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                self.settings.shortcuts.held(i, Shortcut::Proportion),
                self.settings.shortcuts.held(i, Shortcut::RotationSnap),
                self.settings.shortcuts.held(i, Shortcut::SelectionAdd),
                self.settings.shortcuts.held(i, Shortcut::SelectionSubtract),
            )
        });
        let in_toolbar =
            pointer.is_some_and(|p| self.selection.toolbar.is_some_and(|r| r.contains(p)));
        if let Some(f) = &mut self.selection.floating {
            if panning {
                self.selection.drag = None;
                return true;
            }
            if let Some(pointer) = pointer {
                let p = Pos2::from(view.document(pointer));
                if pressed && !in_toolbar && response.hovered() {
                    let handle = hit_handle(view, f, pointer);
                    let rotating = hit_rotation(view, f, pointer);
                    if handle.is_some() || rotating || f.bounds.contains(f.to_local(p)) {
                        self.selection.drag = Some(Drag {
                            point: p,
                            bounds: f.bounds,
                            handle,
                            angle: f.angle,
                            rotating,
                        });
                    }
                }
                if let Some(drag) = &self.selection.drag
                    && (down || released)
                {
                    let delta = p - drag.point;
                    let rotation = egui::emath::Rot2::from_angle(drag.angle);
                    let r = if drag.rotating {
                        let a = drag.point - drag.bounds.center();
                        let b = p - drag.bounds.center();
                        let delta = (a.x * b.y - a.y * b.x).atan2(a.dot(b));
                        let angle = drag.angle + delta;
                        f.angle = if snap {
                            (angle / (std::f32::consts::PI / 12.0)).round()
                                * (std::f32::consts::PI / 12.0)
                        } else {
                            angle
                        };
                        drag.bounds
                    } else if let Some(handle) = drag.handle {
                        let local = resized(
                            drag.bounds,
                            handle,
                            rotation.inverse() * delta,
                            self.selection.keep_ratio ^ proportion,
                        );
                        let center = drag.bounds.center()
                            + rotation * (local.center() - drag.bounds.center());
                        local.translate(center - local.center())
                    } else {
                        drag.bounds.translate(delta.round())
                    };
                    // Prevent unbounded geometry, without clipping the preview to the canvas.
                    if r.is_finite() && r.width() <= 32768.0 && r.height() <= 32768.0 {
                        f.bounds = r;
                    }
                }
            }
            if !down {
                self.selection.drag = None;
            }
            return true;
        }
        let selecting = matches!(self.tool, Tool::SelectRect | Tool::Lasso);
        if !selecting && self.selection.creation.is_none() {
            return in_toolbar && self.gesture.is_none();
        }
        if panning {
            return true;
        }
        if pressed
            && response.hovered()
            && !in_toolbar
            && let Some(pointer) = pointer
        {
            let p = Pos2::from(view.document(pointer));
            if self.doc.contains([p.x, p.y]) {
                let mode = if add {
                    Combine::Add
                } else if subtract {
                    Combine::Subtract
                } else {
                    self.selection.mode
                };
                self.selection.creation = Some(Creation {
                    points: vec![p, p],
                    lasso: self.tool == Tool::Lasso,
                    mode,
                });
            }
        }
        if let Some(creation) = &mut self.selection.creation {
            let points = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::PointerMoved(p) = e {
                            Some(*p)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });
            for pointer in points.into_iter().chain(pointer) {
                let p = Pos2::from(view.document(pointer));
                let p = Pos2::new(
                    p.x.clamp(0.0, self.doc.width as f32),
                    p.y.clamp(0.0, self.doc.height as f32),
                );
                if creation.lasso {
                    if creation.points.last().unwrap().distance(p) * view.zoom >= 1.5 {
                        if creation.points.len() >= 8192 {
                            let mut index = 0_usize;
                            creation.points.retain(|_| {
                                let keep = index.is_multiple_of(2);
                                index += 1;
                                keep
                            });
                        }
                        creation.points.push(p);
                    }
                } else {
                    creation.points[1] = p;
                }
            }
        }
        if !down && let Some(creation) = self.selection.creation.take() {
            let mask = if creation.lasso {
                Mask::polygon(self.doc.width, self.doc.height, &creation.points)
            } else {
                let r = egui::Rect::from_two_pos(creation.points[0], creation.points[1]);
                if r.width() * view.zoom < 2.0 || r.height() * view.zoom < 2.0 {
                    None
                } else {
                    Mask::rectangle(self.doc.width, self.doc.height, r)
                }
            };
            self.selection.mask =
                Mask::combine(self.selection.mask.as_deref(), mask, creation.mode);
            self.doc.selection = self.selection.mask.clone();
            self.status = format!(
                "Seleção pronta · {} transforma · {} desmarca · {} apaga.",
                self.settings.shortcuts.label(Shortcut::Transform),
                self.settings.shortcuts.label(Shortcut::Deselect),
                self.settings.shortcuts.label(Shortcut::DeleteSelection)
            );
        }
        true
    }
    pub(super) fn selection_preview_shapes(
        &self,
        clip: egui::Rect,
        view: CanvasView,
        index: usize,
        opacity: Color32,
        new_layer: bool,
    ) -> Option<Vec<egui::Shape>> {
        if let (Some(f), Some((base, cut))) = (&self.selection.floating, &self.selection.textures)
            && f.layer == index
            && f.new_layer == new_layer
        {
            if let Some(shader) = &self.selection_shader {
                return Some(vec![
                    shader
                        .callback(
                            clip,
                            crate::selection_gpu::Preview {
                                view,
                                bounds: f.bounds,
                                angle: f.angle,
                                region: f.rotated_bounds(),
                                textures: [base.id(), cut.id()],
                                opacity: opacity.a() as f32 / 255.0,
                                flip: [f.flip_x, f.flip_y],
                            },
                        )
                        .into(),
                ]);
            }
            Some(vec![
                egui::Shape::mesh(view.mesh(base.id(), Vec2::splat(1.0), opacity)),
                egui::Shape::mesh(quad(view, f, cut.id(), opacity, f.flip_x, f.flip_y)),
            ])
        } else {
            None
        }
    }
    pub(super) fn selection_properties(&mut self, ui: &mut egui::Ui) {
        ui.label("Modo de seleção");
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.selection.mode, Combine::Replace, "Nova");
            ui.selectable_value(&mut self.selection.mode, Combine::Add, "Adicionar");
            ui.selectable_value(&mut self.selection.mode, Combine::Subtract, "Subtrair");
        });
        crate::theme::caption(
            ui,
            format!(
                "{}: retângulo · {}: laço\n{} adiciona · {} subtrai\n{} transforma · {} desmarca\n{}: mover vista · Roda: zoom",
                self.settings.shortcuts.label(Shortcut::SelectRect),
                self.settings.shortcuts.label(Shortcut::Lasso),
                self.settings.shortcuts.label(Shortcut::SelectionAdd),
                self.settings.shortcuts.label(Shortcut::SelectionSubtract),
                self.settings.shortcuts.label(Shortcut::Transform),
                self.settings.shortcuts.label(Shortcut::Deselect),
                self.settings.shortcuts.label(Shortcut::Pan)
            ),
        );
        ui.checkbox(
            &mut self.selection.keep_ratio,
            "Manter proporção nos cantos",
        );
        crate::theme::caption(
            ui,
            format!(
                "Arraste fora das alças para girar. {} encaixa a rotação em 15°. Nas alças, {} alterna a proporção; as laterais permitem esticar.",
                self.settings.shortcuts.label(Shortcut::RotationSnap),
                self.settings.shortcuts.label(Shortcut::Proportion)
            ),
        );
    }
    pub(super) fn selection_overlay(&mut self, ui: &mut egui::Ui, view: CanvasView) {
        let ctx = ui.ctx().clone();
        let painter = ui.painter().with_clip_rect(ui.max_rect());
        let mut bounds = None;
        if let Some(f) = &self.selection.floating {
            bounds = Some(f.rotated_bounds());
            let mut points = corners(f.bounds)
                .map(|p| screen(view, f.to_world(p)))
                .to_vec();
            points.push(points[0]);
            painter.add(egui::Shape::line(
                points.clone(),
                Stroke::new(3.0_f32, Color32::from_black_alpha(180)),
            ));
            painter.add(egui::Shape::line(
                points,
                Stroke::new(1.2_f32, crate::theme::accent(&ctx)),
            ));
            for p in handles(f.bounds) {
                let rect =
                    egui::Rect::from_center_size(screen(view, f.to_world(p)), Vec2::splat(8.0));
                painter.rect_filled(rect.expand(1.0), 2.0, Color32::from_gray(15));
                painter.rect_filled(rect, 1.0, Color32::WHITE);
            }
            if let Some(p) = ctx.pointer_interact_pos()
                && ui.max_rect().contains(p)
                && !self.selection.toolbar.is_some_and(|r| r.contains(p))
            {
                let cursor = if let Some(index) = hit_handle(view, f, p) {
                    let v = screen(view, f.to_world(handles(f.bounds)[index]))
                        - screen(view, f.bounds.center());
                    let a = (v.y.atan2(v.x) / std::f32::consts::FRAC_PI_4).round() as i32;
                    match a.rem_euclid(4) {
                        0 => egui::CursorIcon::ResizeHorizontal,
                        1 => egui::CursorIcon::ResizeNwSe,
                        2 => egui::CursorIcon::ResizeVertical,
                        _ => egui::CursorIcon::ResizeNeSw,
                    }
                } else if hit_rotation(view, f, p)
                    || self.selection.drag.as_ref().is_some_and(|d| d.rotating)
                {
                    // A curved arrow stays recognizable even on platforms without a rotate cursor.
                    let center = p + Vec2::splat(9.0);
                    let points: Vec<_> = (0..20)
                        .map(|i| {
                            let a = -2.4 + i as f32 * 4.8 / 19.0;
                            center + Vec2::angled(a) * 9.0
                        })
                        .collect();
                    painter.add(egui::Shape::line(
                        points.clone(),
                        Stroke::new(4.0_f32, Color32::BLACK),
                    ));
                    painter.add(egui::Shape::line(
                        points,
                        Stroke::new(2.0_f32, Color32::WHITE),
                    ));
                    let tip = center + Vec2::angled(2.4) * 9.0;
                    painter.arrow(
                        tip - Vec2::new(-5.0, -4.0),
                        Vec2::new(-5.0, -4.0),
                        Stroke::new(2.0_f32, Color32::WHITE),
                    );
                    egui::CursorIcon::None
                } else if f.bounds.contains(f.to_local(Pos2::from(view.document(p)))) {
                    egui::CursorIcon::Move
                } else {
                    egui::CursorIcon::Default
                };
                ctx.set_cursor_icon(cursor);
            }
        } else if let Some(mask) = &self.selection.mask {
            bounds = Some(mask.bounds);
            let reduced = crate::theme::reduced_motion(&ctx);
            let phase = if reduced {
                0.0
            } else {
                ctx.input(|i| i.time as f32 * 22.0) % 12.0
            };
            // Batched line meshes avoid allocating one egui widget per boundary pixel.
            let mut black = Vec::new();
            let mut white = Vec::new();
            for (a, b) in &mask.edges {
                let Some((a, b)) = clipped(view.screen(*a), view.screen(*b), ui.clip_rect()) else {
                    continue;
                };
                let length = a.distance(b);
                if length < 0.01 {
                    continue;
                }
                let direction = (b - a) / length;
                let mut t = -phase;
                while t < length {
                    let left = t.max(0.0);
                    let right = (t + 6.0).min(length);
                    if right > left {
                        white.push(egui::Shape::line_segment(
                            [a + direction * left, a + direction * right],
                            Stroke::new(1.2_f32, Color32::WHITE),
                        ));
                    }
                    t += 12.0;
                }
                black.push(egui::Shape::line_segment(
                    [a, b],
                    Stroke::new(1.2_f32, Color32::BLACK),
                ));
            }
            painter.extend(black);
            painter.extend(white);
            if !reduced {
                ctx.request_repaint_after(std::time::Duration::from_millis(33));
            }
        }
        if let Some(c) = &self.selection.creation {
            let mut points = if c.lasso {
                c.points
                    .iter()
                    .map(|p| screen(view, *p))
                    .collect::<Vec<_>>()
            } else {
                corners(egui::Rect::from_two_pos(c.points[0], c.points[1]))
                    .map(|p| screen(view, p))
                    .to_vec()
            };
            if points.len() > 1 {
                points.push(points[0]);
                painter.add(egui::Shape::line(
                    points.clone(),
                    Stroke::new(3.0_f32, Color32::BLACK),
                ));
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(1.2_f32, Color32::WHITE),
                ));
            }
            self.selection.toolbar = None;
            return;
        }
        if self.pending.is_some() || self.new_dialog || self.gesture.is_some() {
            self.selection.toolbar = None;
            return;
        }
        let Some(bounds) = bounds else {
            self.selection.toolbar = None;
            return;
        };
        let screen_bounds = egui::Rect::from_points(&corners(bounds).map(|p| screen(view, p)));
        let viewport = ui.max_rect().shrink(8.0);
        let size = self
            .selection
            .toolbar
            .map_or(Vec2::new(490.0, 76.0), |r| r.size());
        let width = viewport.width().clamp(120.0, 490.0);
        let x = (screen_bounds.center().x - width * 0.5).clamp(
            viewport.left(),
            (viewport.right() - width).max(viewport.left()),
        );
        let y = if screen_bounds.bottom() + 12.0 + size.y <= viewport.bottom() {
            screen_bounds.bottom() + 12.0
        } else {
            screen_bounds.top() - size.y - 12.0
        };
        let y = y.clamp(
            viewport.top(),
            (viewport.bottom() - size.y).max(viewport.top()),
        );
        let transforming = self.selection.floating.is_some();
        let mut action = 0;
        let area = egui::Area::new(ui.id().with("selection_toolbar"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(x, y))
            .movable(false)
            .constrain_to(ui.max_rect())
            .show(&ctx, |ui| {
                crate::theme::glass_frame(ui).show(ui, |ui| {
                    ui.set_width((width - 20.0).max(100.0));
                    ui.horizontal_wrapped(|ui| {
                        if transforming {
                            if crate::theme::button(ui, "Aplicar").clicked() {
                                action = 1;
                            }
                            if crate::theme::button(ui, "Cancelar").clicked() {
                                action = 2;
                            }
                            if crate::theme::button(ui, "↔ Espelhar").clicked() {
                                action = 3;
                            }
                            if crate::theme::button(ui, "↕ Espelhar").clicked() {
                                action = 4;
                            }
                            if crate::theme::button(ui, "Restaurar").clicked() {
                                action = 5;
                            }
                        } else {
                            if crate::theme::button(ui, "Mover / transformar").clicked() {
                                action = 6;
                            }
                            if ui
                                .button("Duplicar recorte")
                                .on_hover_text(
                                    "Mantém o original e transforma uma cópia na mesma camada",
                                )
                                .clicked()
                            {
                                action = 7;
                            }
                            if crate::theme::button(ui, "Inverter").clicked() {
                                action = 8;
                            }
                            if crate::theme::button(ui, "Apagar").clicked() {
                                action = 9;
                            }
                            if crate::theme::button(ui, "Desmarcar").clicked() {
                                action = 10;
                            }
                        }
                    });
                    if let Some(f) = &self.selection.floating {
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.selection.keep_ratio, "Proporção nos cantos");
                            crate::theme::caption(
                                ui,
                                format!(
                                    "{:.0} × {:.0} px · {:.1}° · {} aplica · {} cancela",
                                    f.bounds.width(),
                                    f.bounds.height(),
                                    f.angle.to_degrees(),
                                    self.settings.shortcuts.label(Shortcut::Apply),
                                    self.settings.shortcuts.label(Shortcut::Cancel)
                                ),
                            );
                        });
                    } else {
                        crate::theme::caption(
                            ui,
                            format!(
                                "Camada ativa · {} adiciona · {} subtrai · {} transforma",
                                self.settings.shortcuts.label(Shortcut::SelectionAdd),
                                self.settings.shortcuts.label(Shortcut::SelectionSubtract),
                                self.settings.shortcuts.label(Shortcut::Transform)
                            ),
                        );
                    }
                });
            });
        self.selection.toolbar = Some(area.response.rect);
        match action {
            1 => self.apply_transform(),
            2 => self.cancel_transform(),
            3 => {
                if let Some(f) = &mut self.selection.floating {
                    f.flip_x = !f.flip_x;
                }
            }
            4 => {
                if let Some(f) = &mut self.selection.floating {
                    f.flip_y = !f.flip_y;
                }
            }
            5 => {
                if let Some(f) = &mut self.selection.floating {
                    f.bounds = f.original;
                    f.angle = 0.0;
                    f.flip_x = false;
                    f.flip_y = false;
                }
            }
            6 => self.begin_transform(&ctx, false),
            7 => self.begin_transform(&ctx, true),
            8 => {
                self.selection.mask = self.selection.mask.as_ref().and_then(|m| m.inverted());
                self.doc.selection = self.selection.mask.clone();
            }
            9 => self.delete_selected(),
            10 => self.clear_selection(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_handles_and_rotation_zone_follow_mirrored_canvas() {
        let doc = Document::new(200, 200, false);
        let mut f = Floating::pasted(
            &doc,
            [2, 2],
            vec![255; 16],
            egui::Rect::from_min_size(Pos2::new(50.0, 60.0), Vec2::new(80.0, 40.0)),
        )
        .unwrap();
        f.angle = 0.7;
        for mirrored in [false, true] {
            let view = CanvasView {
                center: Pos2::new(300.0, 300.0),
                size: Vec2::splat(200.0),
                zoom: 1.8,
                angle: -0.4,
                mirrored,
            };
            let corner = screen(view, f.to_world(f.bounds.left_top()));
            assert_eq!(hit_handle(view, &f, corner), Some(0));
            let outward = (corner - screen(view, f.bounds.center())).normalized();
            assert!(hit_rotation(view, &f, corner + outward * 18.0));
            assert!(!hit_rotation(view, &f, corner));
            assert!(!hit_rotation(view, &f, screen(view, f.bounds.center())));
        }
        let original_anchor = f.to_world(f.bounds.left_top());
        let local = resized(f.bounds, 4, Vec2::new(20.0, 10.0), false);
        let center = f.bounds.center()
            + egui::emath::Rot2::from_angle(f.angle) * (local.center() - f.bounds.center());
        f.bounds = local.translate(center - local.center());
        assert!(f.to_world(f.bounds.left_top()).distance(original_anchor) < 0.001);
    }
    #[test]
    fn corners_keep_ratio_and_sides_stretch() {
        let r = egui::Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(80.0, 40.0));
        for handle in [0, 2, 4, 6] {
            let out = resized(r, handle, Vec2::new(25.0, 14.0), true);
            assert!((out.aspect_ratio() - 2.0).abs() < 0.001);
            assert!(out.width() >= 1.0 && out.height() >= 1.0);
        }
        let out = resized(r, 3, Vec2::new(25.0, 14.0), true);
        assert_eq!(out.size(), Vec2::new(105.0, 40.0));
    }
}
