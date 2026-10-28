//! Canvas-native Liquify and Clone controls, sharing the tiled stroke history.
use super::*;
use crate::retouch::{self, Mode};

/// Keep the actual path to within a quarter document pixel, while avoiding a
/// full brush-area warp for every redundant tablet packet in the same frame.
/// Always retains the last point, including movements smaller than one pixel.
fn liquify_samples(start: [f32; 2], samples: impl IntoIterator<Item = [f32; 2]>) -> Vec<[f32; 2]> {
    let mut path = vec![start];
    for p in samples {
        if path.last() != Some(&p) {
            path.push(p);
        }
    }
    if path.len() <= 2 {
        return path.into_iter().skip(1).collect();
    }
    let mut keep = vec![false; path.len()];
    keep[0] = true;
    keep[path.len() - 1] = true;
    let mut pending = vec![(0, path.len() - 1)];
    while let Some((a, b)) = pending.pop() {
        let dx = path[b][0] - path[a][0];
        let dy = path[b][1] - path[a][1];
        let length2 = dx * dx + dy * dy;
        let mut largest = 0.25 * 0.25;
        let mut split = None;
        for i in a + 1..b {
            let px = path[i][0] - path[a][0];
            let py = path[i][1] - path[a][1];
            let t = if length2 > 0.0 {
                ((px * dx + py * dy) / length2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let error = (px - dx * t).powi(2) + (py - dy * t).powi(2);
            if error > largest {
                largest = error;
                split = Some(i);
            }
        }
        if let Some(i) = split {
            keep[i] = true;
            pending.push((a, i));
            pending.push((i, b));
        }
    }
    path.into_iter()
        .zip(keep)
        .skip(1)
        .filter_map(|(p, keep)| keep.then_some(p))
        .collect()
}

struct StrokeState {
    last: [f32; 2],
    source: Vec<u8>,
    offset: [f32; 2],
    velocity: Vec2,
    pressure: f32,
    tail: f32,
    idle_time: f32,
}
struct SourceDrag {
    start: Pos2,
    time: f64,
    moved: bool,
    toggled: bool,
}
pub(super) struct State {
    pub mode: Mode,
    pub size: f32,
    pub strength: f32,
    pub distortion: f32,
    pub momentum: f32,
    pub pressure: bool,
    pub locked: bool,
    pub source: Option<[f32; 2]>,
    key: Option<(u64, usize, u32, u32)>,
    original: Option<Vec<u8>>,
    scratch: retouch::Scratch,
    stroke: Option<StrokeState>,
    dragging: Option<SourceDrag>,
    region: Option<[usize; 4]>,
    amount: f32,
    amount_source: Option<Vec<u8>>,
    amount_edit: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::Push,
            size: 180.0,
            strength: 0.5,
            distortion: 0.0,
            momentum: 0.0,
            pressure: true,
            locked: false,
            source: None,
            key: None,
            original: None,
            scratch: Default::default(),
            stroke: None,
            dragging: None,
            region: None,
            amount: 1.0,
            amount_source: None,
            amount_edit: false,
        }
    }
}
impl State {
    pub fn busy(&self) -> bool {
        self.stroke.is_some() || self.dragging.is_some() || self.amount_edit
    }
    pub fn clear(&mut self) {
        self.key = None;
        self.original = None;
        self.stroke = None;
        self.dragging = None;
        self.source = None;
        self.region = None;
        self.amount_source = None;
        self.amount_edit = false;
        self.amount = 1.0;
        self.scratch = Default::default();
    }
    pub fn memory_bytes(&self) -> usize {
        self.original.as_ref().map_or(0, Vec::len)
            + self.stroke.as_ref().map_or(0, |s| s.source.len())
            + self.amount_source.as_ref().map_or(0, Vec::len)
            + self.scratch.memory_bytes()
    }
}
impl PaintAtlas {
    fn retouch_key(&mut self) {
        let key = (
            self.documents[self.active_document].id,
            self.doc.active,
            self.doc.width,
            self.doc.height,
        );
        if self.retouch.key != Some(key) {
            self.retouch.clear();
            self.retouch.key = Some(key);
        }
    }
    pub(super) fn retouch_properties(&mut self, ui: &mut egui::Ui) {
        self.retouch_key();
        let liquify = self.tool == Tool::Liquify;
        let busy = self.retouch.stroke.is_some() || self.retouch.dragging.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            if liquify {
                egui::ComboBox::from_id_salt("liquify_mode")
                    .selected_text(crate::i18n::ui(ui.ctx(), self.retouch.mode.label()))
                    .width(ui.available_width().min(180.0))
                    .show_ui(ui, |ui| {
                        for mode in Mode::ALL {
                            let label = crate::i18n::ui(ui.ctx(),mode.label());
                            ui.selectable_value(&mut self.retouch.mode, mode, label);
                        }
                    });
                if ui.checkbox(&mut self.settings.liquify_smoothing,crate::i18n::ui(ui.ctx(),"Smooth deformation (interpolation)"))
                    .on_hover_text(crate::i18n::ui(ui.ctx(),"Disable to preserve crisp pixels. Slow movements still accumulate; edges may appear pixelated."))
                    .changed() {
                    self.settings_pending = true;
                    self.settings_edited = std::time::Instant::now();
                }
                let size_label = crate::i18n::ui(ui.ctx(), "Size");
                ui.add(egui::Slider::new(&mut self.retouch.size,1.0..=1000.0).text(size_label).suffix(" px"));
            } else {
                let label = crate::i18n::ui(ui.ctx(), "Size");
                ui.add(egui::Slider::new(&mut self.size,1.0..=MAX_BRUSH_SIZE).text(label).suffix(" px"));
            }
            let label = crate::i18n::ui(ui.ctx(), "Strength");
            ui.add(egui::Slider::new(&mut self.retouch.strength,0.0..=1.0).text(label));
            ui.checkbox(&mut self.retouch.pressure,crate::i18n::ui(ui.ctx(),"Pen pressure"));
            if liquify {
                let label = crate::i18n::ui(ui.ctx(), "Distortion");
                ui.add(egui::Slider::new(&mut self.retouch.distortion,0.0..=1.0).text(label));
                let label = crate::i18n::ui(ui.ctx(), "Momentum");
                ui.add(egui::Slider::new(&mut self.retouch.momentum,0.0..=1.0).text(label));
                if self.retouch.original.is_some() && self.retouch.region.is_some() {
                    let label = crate::i18n::ui(ui.ctx(), "Amount");
                    let previous = self.retouch.amount;
                    let response = ui.add(egui::Slider::new(&mut self.retouch.amount, 0.0..=1.0).text(label));
                    if response.changed() {
                        if let Err(error) = self.adjust_liquify_amount() {
                            self.retouch.amount = previous;
                            self.status = error;
                        }
                    }
                    if self.retouch.amount_edit && !response.dragged() {
                        self.retouch.amount_edit = false;
                        self.history.commit(&self.doc);
                    }
                }
                if ui.add_enabled(!self.retouch.amount_edit && self.retouch.original.is_some(),egui::Button::new(crate::i18n::ui(ui.ctx(),"Reset Liquify"))).clicked() {
                    self.reset_liquify();
                }
                crate::theme::caption(ui,crate::i18n::ui(ui.ctx(),"Paint to deform the active layer. Reconstruct restores the original locally. Each stroke can be undone."));
            } else {
                ui.checkbox(&mut self.retouch.locked,crate::i18n::ui(ui.ctx(),"Lock source"));
                let label = crate::i18n::ui(ui.ctx(), "Hardness");
                ui.add(egui::Slider::new(&mut self.hardness,0.0..=1.0).text(label));
                crate::theme::caption(ui,crate::i18n::ui(ui.ctx(),format!("Drag the source disc, or {} + click to set the source. Hold the disc to lock or unlock it. Samples the active layer using the current brush tip.",self.settings.shortcuts.label(Shortcut::CloneSource))));
            }
        });
    }
    fn adjust_liquify_amount(&mut self) -> Result<(), String> {
        if self.retouch.amount_source.is_none() {
            self.retouch.amount_source = Some(retouch::copy_pixels(
                &self.doc.layers[self.doc.active].pixels,
            )?);
        }
        if !self.retouch.amount_edit {
            self.doc.damage = None;
            self.history.push_stroke(&mut self.doc);
            self.retouch.amount_edit = true;
        }
        let region = self.retouch.region.unwrap();
        self.doc.prepare_pixel_edit(region);
        let original = self.retouch.original.as_ref().unwrap();
        let warped = self.retouch.amount_source.as_ref().unwrap();
        for y in region[1]..region[3] {
            for x in region[0]..region[2] {
                let i = (y * self.doc.width as usize + x) * 4;
                let a = original[i..i + 4].try_into().unwrap();
                let b = warped[i..i + 4].try_into().unwrap();
                let out = retouch::mix(a, b, self.retouch.amount, false);
                self.doc.layers[self.doc.active].pixels[i..i + 4].copy_from_slice(&out);
            }
        }
        self.mark_document_changed();
        Ok(())
    }
    fn reset_liquify(&mut self) {
        let Some(original) = self.retouch.original.as_ref() else {
            return;
        };
        let Some(region) = self.retouch.region else {
            return;
        };
        self.doc.damage = None;
        self.history.push_stroke(&mut self.doc);
        self.doc.prepare_pixel_edit(region);
        for y in region[1]..region[3] {
            let i = (y * self.doc.width as usize + region[0]) * 4;
            let len = (region[2] - region[0]) * 4;
            self.doc.layers[self.doc.active].pixels[i..i + len]
                .copy_from_slice(&original[i..i + len]);
        }
        self.history.commit(&self.doc);
        self.changed();
        self.retouch.original = None;
        self.retouch.region = None;
    }
    fn retouch_dab(&mut self, stroke: &mut StrokeState, point: [f32; 2], dt: f32) -> bool {
        let delta = [point[0] - stroke.last[0], point[1] - stroke.last[1]];
        let pressure = if self.retouch.pressure {
            stroke.pressure.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let strength = self.retouch.strength * pressure;
        let result = if self.tool == Tool::Liquify {
            let strength = if self.retouch.mode == Mode::Push {
                strength
            } else {
                strength * dt * 30.0
            };
            self.retouch.scratch.liquify(
                &mut self.doc,
                point,
                delta,
                self.retouch.size,
                strength,
                self.retouch.distortion,
                self.retouch.mode,
                self.retouch.original.as_deref().unwrap_or(&[]),
                self.settings.liquify_smoothing,
            )
        } else {
            let source = if self.retouch.locked {
                self.retouch.source.unwrap()
            } else {
                [point[0] + stroke.offset[0], point[1] + stroke.offset[1]]
            };
            let mut brush = self.brush();
            brush.opacity = strength;
            if !self.retouch.pressure {
                brush.size = self.size;
            }
            Ok(retouch::clone_dab(
                &mut self.doc,
                &stroke.source,
                point,
                source,
                &brush,
            ))
        };
        stroke.last = point;
        match result {
            Ok(changed) => changed,
            Err(error) => {
                self.status = error;
                false
            }
        }
    }
    fn retouch_segment(&mut self, stroke: &mut StrokeState, point: [f32; 2], dt: f32) -> bool {
        let a = stroke.last;
        let distance = (point[0] - a[0]).hypot(point[1] - a[1]);
        let diameter = if self.tool == Tool::Liquify {
            self.retouch.size
        } else {
            self.brush().size
        };
        let spacing = if self.tool == Tool::Liquify {
            0.15
        } else {
            self.spacing.clamp(0.05, 1.0)
        };
        let steps = (distance / (diameter * spacing).max(1.0))
            .ceil()
            .clamp(1.0, 512.0) as usize;
        let mut changed = false;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            changed |= self.retouch_dab(
                stroke,
                [a[0] + (point[0] - a[0]) * t, a[1] + (point[1] - a[1]) * t],
                dt / steps as f32,
            );
        }
        changed
    }
    pub(super) fn retouch_input(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        view: CanvasView,
        panning: bool,
    ) -> bool {
        if !matches!(self.tool, Tool::Liquify | Tool::Clone) {
            return false;
        }
        self.retouch_key();
        let ctx = ui.ctx();
        let (pointer, down, pressed, dt, time, focused, set_source) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.primary_down(),
                i.pointer.primary_pressed(),
                i.stable_dt.clamp(0.001, 0.05),
                i.time,
                i.focused,
                !self.preferences.capturing_shortcut()
                    && self.settings.shortcuts.held(i, Shortcut::CloneSource),
            )
        });
        if self.tool == Tool::Clone && self.retouch.source.is_none() {
            let p = view.document(response.rect.center());
            self.retouch.source = Some([
                p[0].clamp(0.5, self.doc.width as f32 - 0.5),
                p[1].clamp(0.5, self.doc.height as f32 - 0.5),
            ]);
        }
        if self.retouch.amount_edit {
            if !down {
                self.retouch.amount_edit = false;
                self.history.commit(&self.doc);
            }
            return true;
        }
        if panning && !self.retouch.busy() {
            return true;
        }
        if let Some(drag) = &mut self.retouch.dragging {
            if down
                && focused
                && let Some(pointer) = pointer
            {
                if pointer.distance(drag.start) > 3.0 {
                    drag.moved = true;
                }
                if drag.moved {
                    let p = view.document(pointer);
                    self.retouch.source = Some([
                        p[0].clamp(0.5, self.doc.width as f32 - 0.5),
                        p[1].clamp(0.5, self.doc.height as f32 - 0.5),
                    ]);
                } else if !drag.toggled && time - drag.time >= 0.55 {
                    self.retouch.locked = !self.retouch.locked;
                    drag.toggled = true;
                }
                ctx.request_repaint();
            } else {
                self.retouch.dragging = None;
            }
            return true;
        }
        // A fresh press starts a new undo transaction, including during an inertial tail.
        if pressed && response.hovered() && self.retouch.stroke.is_some() {
            self.history.commit(&self.doc);
            self.retouch.stroke = None;
        }
        if pressed
            && response.hovered()
            && self.retouch.stroke.is_none()
            && let Some(pointer) = pointer
        {
            let pointer = ctx
                .input(|i| {
                    i.events.iter().find_map(|event| match event {
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            ..
                        } => Some(*pos),
                        _ => None,
                    })
                })
                .unwrap_or(pointer);
            let point = view.document(pointer);
            if self.tool == Tool::Clone && set_source && self.doc.contains(point) {
                self.retouch.source = Some(point);
                return true;
            }
            if self.tool == Tool::Clone
                && self
                    .retouch
                    .source
                    .is_some_and(|s| view.screen(s).distance(pointer) <= 16.0)
            {
                self.retouch.dragging = Some(SourceDrag {
                    start: pointer,
                    time,
                    moved: false,
                    toggled: false,
                });
                return true;
            }
            if !self.doc.contains(point) {
                return true;
            }
            let layer = &self.doc.layers[self.doc.active];
            if layer.group.is_some() || !layer.visible {
                self.status = "Select a visible raster layer to use Liquify or Clone.".into();
                return true;
            }
            let snapshot = if self.tool == Tool::Clone || self.retouch.original.is_none() {
                match retouch::copy_pixels(&layer.pixels) {
                    Ok(pixels) => pixels,
                    Err(error) => {
                        self.status = error;
                        return true;
                    }
                }
            } else {
                Vec::new()
            };
            let source = if self.tool == Tool::Liquify {
                if self.retouch.original.is_none() {
                    self.retouch.original = Some(snapshot);
                }
                Vec::new()
            } else {
                snapshot
            };
            self.retouch.amount_source = None;
            self.retouch.amount = 1.0;
            self.finish_quickshape(false);
            self.doc.damage = None;
            self.history.push_stroke(&mut self.doc);
            self.retouch.scratch.begin_stroke();
            let origin = self.retouch.source.unwrap_or(point);
            let mut stroke = StrokeState {
                last: point,
                source,
                offset: [origin[0] - point[0], origin[1] - point[1]],
                velocity: Vec2::ZERO,
                pressure: self.pressure,
                tail: 0.0,
                idle_time: 0.0,
            };
            if self.retouch_dab(&mut stroke, point, dt) {
                self.retouch_changed();
            }
            self.retouch.stroke = Some(stroke);
        }
        if let Some(mut stroke) = self.retouch.stroke.take() {
            let mut changed = false;
            if focused && (down || pressed || ctx.input(|i| i.pointer.primary_released())) {
                if down || pressed {
                    stroke.pressure = self.pressure;
                }
                let samples = ctx.input(|i| {
                    let mut started = !pressed;
                    let mut samples = Vec::new();
                    for event in &i.events {
                        match event {
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                ..
                            } => {
                                if !pressed && started {
                                    samples.push(*pos);
                                }
                                started = *pressed;
                            }
                            egui::Event::PointerMoved(p) if started => samples.push(*p),
                            _ => {}
                        }
                    }
                    samples
                });
                let start = stroke.last;
                let mut samples: Vec<_> = samples.into_iter().chain(pointer).collect();
                samples.dedup();
                let points: Vec<_> = samples.into_iter().map(|p| view.document(p)).collect();
                let points = if self.tool == Tool::Liquify {
                    liquify_samples(stroke.last, points)
                } else {
                    points
                };
                let count = points.len().max(1) as f32;
                for point in points {
                    if (point[0] - stroke.last[0]).hypot(point[1] - stroke.last[1]) > 0.01 {
                        changed |= self.retouch_segment(&mut stroke, point, dt / count);
                    }
                }
                let movement = Vec2::new(stroke.last[0] - start[0], stroke.last[1] - start[1]);
                // Clamp velocity so a dropped frame cannot throw the deformation across the canvas.
                if movement.length() > 0.01 || down {
                    stroke.velocity = movement / dt;
                }
                let speed = stroke.velocity.length();
                if speed > self.retouch.size * 15.0 {
                    stroke.velocity *= self.retouch.size * 15.0 / speed;
                }
                if movement.length() < 0.01 && self.tool == Tool::Liquify && !pressed && down {
                    stroke.idle_time += dt;
                    if stroke.idle_time >= 1.0 / 60.0 {
                        let point = stroke.last;
                        let elapsed = stroke.idle_time;
                        stroke.idle_time = 0.0;
                        changed |= self.retouch_dab(&mut stroke, point, elapsed);
                    }
                } else {
                    stroke.idle_time = 0.0;
                }
                if !down
                    && !(self.tool == Tool::Liquify
                        && self.retouch.momentum > 0.0
                        && stroke.velocity.length() > 1.0)
                {
                    if self.tool == Tool::Clone && !self.retouch.locked {
                        self.retouch.source = Some([
                            stroke.last[0] + stroke.offset[0],
                            stroke.last[1] + stroke.offset[1],
                        ]);
                    }
                    self.history.commit(&self.doc);
                    self.retouch.scratch.end_stroke();
                } else {
                    self.retouch.stroke = Some(stroke);
                }
            } else if self.tool == Tool::Liquify
                && focused
                && self.retouch.momentum > 0.0
                && stroke.tail < 0.45 * self.retouch.momentum
                && stroke.velocity.length() > 1.0
            {
                stroke.tail += dt;
                stroke.velocity *= (-dt * 14.0).exp();
                let point = [
                    stroke.last[0] + stroke.velocity.x * dt,
                    stroke.last[1] + stroke.velocity.y * dt,
                ];
                changed |= self.retouch_segment(&mut stroke, point, dt);
                self.retouch.stroke = Some(stroke);
            } else {
                if self.tool == Tool::Clone && !self.retouch.locked {
                    self.retouch.source = Some([
                        stroke.last[0] + stroke.offset[0],
                        stroke.last[1] + stroke.offset[1],
                    ]);
                }
                self.history.commit(&self.doc);
                self.retouch.scratch.end_stroke();
            }
            if changed {
                self.retouch_changed();
            }
            if self.retouch.stroke.is_some() {
                ctx.request_repaint();
            }
        }
        true
    }
    fn retouch_changed(&mut self) {
        if self.tool == Tool::Liquify
            && let Some(r) = self.doc.damage
        {
            self.retouch.region = Some(self.retouch.region.map_or(r, |a| {
                [
                    a[0].min(r[0]),
                    a[1].min(r[1]),
                    a[2].max(r[2]),
                    a[3].max(r[3]),
                ]
            }));
        }
        self.mark_document_changed();
    }
    pub(super) fn retouch_overlay(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        view: CanvasView,
    ) {
        if !matches!(self.tool, Tool::Liquify | Tool::Clone) {
            return;
        }
        let painter = ui.painter().with_clip_rect(response.rect);
        if self.tool == Tool::Clone
            && let Some(mut source) = self.retouch.source
        {
            if !self.retouch.locked
                && let Some(stroke) = &self.retouch.stroke
            {
                source = [
                    stroke.last[0] + stroke.offset[0],
                    stroke.last[1] + stroke.offset[1],
                ];
            }
            let p = view.screen(source);
            let pulse = if self.retouch.dragging.as_ref().is_some_and(|d| d.toggled) {
                1.0 + (ui.input(|i| i.time) as f32 * 16.0).sin() * 0.10
            } else {
                1.0
            };
            let r = 13.0 * pulse;
            painter.circle_filled(p, r, Color32::from_black_alpha(80));
            painter.circle_stroke(
                p,
                r + 1.0,
                Stroke::new(3.0_f32, Color32::from_black_alpha(180)),
            );
            let color = if self.retouch.locked {
                crate::theme::accent(ui.ctx())
            } else {
                Color32::WHITE
            };
            painter.circle_stroke(p, r, Stroke::new(1.5_f32, color));
            for direction in [Vec2::X, Vec2::Y] {
                painter.line_segment(
                    [p - direction * 6.0, p + direction * 6.0],
                    Stroke::new(1.0_f32, color),
                );
            }
        }
        if response.hovered()
            && self.retouch.dragging.is_none()
            && let Some(p) = ui.input(|i| i.pointer.hover_pos())
        {
            if !self.doc.contains(view.document(p)) {
                return;
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            if self.tool == Tool::Clone {
                let mut brush = self.brush();
                if !self.retouch.pressure {
                    brush.size = self.size;
                }
                self.paint_brush_cursor(&painter, view, p, &brush);
            } else {
                let r = self.retouch.size * view.zoom * 0.5;
                painter.circle_stroke(p, r, Stroke::new(2.5_f32, Color32::from_black_alpha(160)));
                painter.circle_stroke(p, r, Stroke::new(1.0_f32, Color32::WHITE));
                painter.circle_filled(p, 1.5, Color32::WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Rect;
    fn setup(ctx: &egui::Context, tool: Tool) -> PaintAtlas {
        let mut app = PaintAtlas::from_context(ctx);
        app.reset(Document::new(96, 96, true), None, false);
        for y in 0..96 {
            for x in 0..96 {
                let i = (y * 96 + x) * 4;
                app.doc.layers[app.doc.active].pixels[i..i + 4].copy_from_slice(&[
                    x as u8 * 2,
                    y as u8 * 2,
                    100,
                    255,
                ]);
            }
        }
        app.select_tool(tool);
        app.retouch.strength = 1.0;
        app.retouch.pressure = false;
        app.retouch.size = 30.0;
        app.size = 10.0;
        app.hardness = 1.0;
        app.active_tip = None;
        app.pressure_enabled = false;
        app
    }
    fn frame(app: &mut PaintAtlas, ctx: &egui::Context, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                focused: true,
                time: Some(ctx.cumulative_frame_nr() as f64 / 60.0),
                events,
                ..Default::default()
            },
            |ctx| app.canvas(ctx),
        );
    }
    fn position(app: &PaintAtlas, p: [f32; 2]) -> Pos2 {
        CanvasView {
            center: app.last_canvas_bounds.unwrap().center() + app.pan,
            size: Vec2::splat(96.0),
            zoom: app.zoom,
            angle: app.angle,
            mirrored: app.mirrored,
        }
        .screen(p)
    }
    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    #[test]
    fn clone_canvas_input_paints_from_disc_and_roundtrips_history() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Tool::Clone);
        frame(&mut app, &ctx, vec![]);
        app.retouch.source = Some([20.5, 20.5]);
        let before = app.doc.layers[app.doc.active].pixels.clone();
        let p = position(&app, [60.5, 60.5]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        let q = position(&app, [65.5, 60.5]);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(q)]);
        frame(&mut app, &ctx, vec![button(q, false)]);
        frame(&mut app, &ctx, vec![]);
        assert!(!app.retouch.busy());
        assert!(app.gesture.is_none());
        let after = app.doc.layers[app.doc.active].pixels.clone();
        assert_eq!(
            &after[(60 * 96 + 60) * 4..(60 * 96 + 60) * 4 + 4],
            &[40, 40, 100, 255]
        );
        assert_ne!(after, before);
        assert!(app.retouch.source.unwrap()[0] > 24.0);
        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, before);
        app.redo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, after);
    }
    #[test]
    fn liquify_canvas_input_amount_and_reset_are_undoable() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Tool::Liquify);
        frame(&mut app, &ctx, vec![]);
        let before = app.doc.layers[app.doc.active].pixels.clone();
        let p = position(&app, [45.5, 45.5]);
        let q = position(&app, [53.5, 45.5]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(q)]);
        frame(&mut app, &ctx, vec![button(q, false)]);
        frame(&mut app, &ctx, vec![]);
        assert!(!app.retouch.busy());
        let after = app.doc.layers[app.doc.active].pixels.clone();
        assert_ne!(after, before);
        assert!(app.retouch.original.is_some());
        app.retouch.amount = 0.0;
        app.adjust_liquify_amount().unwrap();
        app.retouch.amount_edit = false;
        app.history.commit(&app.doc);
        assert_eq!(app.doc.layers[app.doc.active].pixels, before);
        app.retouch.amount = 1.0;
        app.adjust_liquify_amount().unwrap();
        app.retouch.amount_edit = false;
        app.history.commit(&app.doc);
        assert_eq!(app.doc.layers[app.doc.active].pixels, after);
        app.reset_liquify();
        assert_eq!(app.doc.layers[app.doc.active].pixels, before);
        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, after);
        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, before);
    }
    #[test]
    fn fast_consecutive_clone_strokes_remain_separate_history_entries() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Tool::Clone);
        frame(&mut app, &ctx, vec![]);
        app.retouch.source = Some([20.5, 20.5]);
        app.retouch.locked = true;
        let before = app.doc.layers[app.doc.active].pixels.clone();
        let mut first = Vec::new();
        for x in [60.5, 75.5] {
            let p = position(&app, [x, 60.5]);
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(p), button(p, true)],
            );
            frame(&mut app, &ctx, vec![button(p, false)]);
            assert!(!app.retouch.busy());
            if first.is_empty() {
                first = app.doc.layers[app.doc.active].pixels.clone();
            }
        }
        assert_eq!(app.history.counts().0, 2);
        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, first);
        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, before);
    }
    #[test]
    fn liquify_momentum_finishes_as_a_single_undo_transaction() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Tool::Liquify);
        app.retouch.momentum = 1.0;
        frame(&mut app, &ctx, vec![]);
        let before = app.doc.layers[app.doc.active].pixels.clone();
        let p = position(&app, [40.5, 45.5]);
        let q = position(&app, [50.5, 45.5]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(q)]);
        frame(&mut app, &ctx, vec![button(q, false)]);
        assert!(app.retouch.busy());
        for _ in 0..60 {
            frame(&mut app, &ctx, vec![]);
        }
        assert!(!app.retouch.busy());
        assert_eq!(app.history.counts().0, 1);
        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, before);
    }
    #[test]
    fn choosing_a_brush_preset_keeps_clone_active_and_preserves_its_source() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Tool::Clone);
        app.retouch.source = Some([12.0, 18.0]);
        app.select_preset(0);
        assert!(app.tool == Tool::Clone);
        assert_eq!(app.retouch.source, Some([12.0, 18.0]));
        app.select_tool(Tool::Brush);
        assert!(app.tool == Tool::Brush);
        assert!(app.retouch.source.is_none());
    }
    #[test]
    fn slow_low_strength_liquify_drag_works_without_smoothing() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Tool::Liquify);
        app.settings.liquify_smoothing = false;
        app.retouch.strength = 0.2;
        frame(&mut app, &ctx, vec![]);
        let before = app.doc.layers[app.doc.active].pixels.clone();
        let p = position(&app, [40.5, 45.5]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        for i in 1..=40 {
            let p = position(&app, [40.5 + i as f32 * 0.2, 45.5]);
            frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
        }
        let p = position(&app, [48.5, 45.5]);
        frame(&mut app, &ctx, vec![button(p, false)]);
        assert!(!app.retouch.busy());
        let i = (45 * 96 + 44) * 4;
        assert!(app.doc.layers[app.doc.active].pixels[i] < before[i]);
        assert_eq!(app.history.counts().0, 1);
        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].pixels, before);
    }
    #[test]
    fn tablet_packet_coalescing_keeps_small_motion_corners_and_reversals() {
        let line = liquify_samples([0.0, 0.0], (1..=100).map(|i| [i as f32 * 0.1, 0.0]));
        assert_eq!(line, vec![[10.0, 0.0]]);
        assert_eq!(
            liquify_samples([0.0, 0.0], [[0.02, 0.0]]),
            vec![[0.02, 0.0]]
        );
        assert_eq!(
            liquify_samples([0.0, 0.0], [[5.0, 0.0], [5.0, 5.0]]),
            vec![[5.0, 0.0], [5.0, 5.0]]
        );
        assert_eq!(
            liquify_samples([0.0, 0.0], [[5.0, 0.0], [0.0, 0.0]]),
            vec![[5.0, 0.0], [0.0, 0.0]]
        );
        let curved: Vec<_> = (1..=40)
            .map(|i| {
                let t = i as f32 / 40.0;
                [t * 20.0, t * t * 20.0]
            })
            .collect();
        let simplified = liquify_samples([0.0, 0.0], curved.clone());
        assert!(simplified.len() > 2 && simplified.len() < curved.len());
        assert_eq!(simplified.last(), curved.last());
        let mut segments = vec![[0.0, 0.0]];
        segments.extend(simplified);
        for p in curved {
            let distance2 = segments
                .windows(2)
                .map(|pair| {
                    let dx = pair[1][0] - pair[0][0];
                    let dy = pair[1][1] - pair[0][1];
                    let t = (((p[0] - pair[0][0]) * dx + (p[1] - pair[0][1]) * dy)
                        / (dx * dx + dy * dy))
                        .clamp(0.0, 1.0);
                    (p[0] - pair[0][0] - dx * t).powi(2) + (p[1] - pair[0][1] - dy * t).powi(2)
                })
                .fold(f32::INFINITY, f32::min);
            assert!(distance2 <= 0.25 * 0.25 + 0.00001);
        }
    }
    #[test]
    fn dense_tablet_packets_match_the_same_direct_liquify_drag() {
        let ctx_a = egui::Context::default();
        let ctx_b = egui::Context::default();
        let mut a = setup(&ctx_a, Tool::Liquify);
        let mut b = setup(&ctx_b, Tool::Liquify);
        a.retouch.size = 80.0;
        b.retouch.size = 80.0;
        frame(&mut a, &ctx_a, vec![]);
        frame(&mut b, &ctx_b, vec![]);
        let start = position(&a, [40.5, 45.5]);
        frame(
            &mut a,
            &ctx_a,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame(
            &mut b,
            &ctx_b,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        let packets = (1..=32)
            .map(|i| egui::Event::PointerMoved(position(&a, [40.5 + i as f32 * 0.25, 45.5])))
            .collect();
        let end = position(&a, [48.5, 45.5]);
        frame(&mut a, &ctx_a, packets);
        frame(&mut b, &ctx_b, vec![egui::Event::PointerMoved(end)]);
        frame(&mut a, &ctx_a, vec![button(end, false)]);
        frame(&mut b, &ctx_b, vec![button(end, false)]);
        assert_eq!(
            a.doc.layers[a.doc.active].pixels,
            b.doc.layers[b.doc.active].pixels
        );
        assert_eq!(a.history.counts().0, 1);
        assert_eq!(b.history.counts().0, 1);
    }
    #[test]
    fn zero_pressure_has_no_effect_and_source_is_cleared_when_canvas_changes() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Tool::Liquify);
        app.retouch_key();
        let before = app.doc.layers[app.doc.active].pixels.clone();
        app.retouch.original = Some(before.clone());
        app.retouch.pressure = true;
        let mut stroke = StrokeState {
            last: [40.0, 40.0],
            source: Vec::new(),
            offset: [0.0; 2],
            velocity: Vec2::ZERO,
            pressure: 0.0,
            tail: 0.0,
            idle_time: 0.0,
        };
        assert!(!app.retouch_dab(&mut stroke, [50.0, 40.0], 1.0 / 60.0));
        assert_eq!(app.doc.layers[app.doc.active].pixels, before);
        app.retouch.source = Some([12.0, 12.0]);
        app.reset(Document::new(128, 128, true), None, false);
        assert!(app.retouch.source.is_none());
        assert!(app.retouch.original.is_none());
    }
}
