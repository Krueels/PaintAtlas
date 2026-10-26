use super::*;
use crate::quickshape::{Kind, Shape, VecGeometry};

pub(super) struct Stroke {
    samples: Vec<(Vec2, f32)>,
    drawn: Vec<Vec2>,
    anchor: Vec2,
    moved_at: f64,
    attempted: bool,
    pub disabled: bool,
    pub active: Option<Edit>,
}
impl Stroke {
    pub fn new(point: [f32; 2], size: f32, time: f64) -> Self {
        Self {
            samples: vec![(point.into(), size)],
            drawn: vec![point.into()],
            anchor: point.into(),
            moved_at: time,
            attempted: false,
            disabled: false,
            active: None,
        }
    }
    pub fn record(&mut self, point: [f32; 2]) {
        let p = Vec2::from(point);
        if self
            .drawn
            .last()
            .is_some_and(|last| last.distance_sq(p) < 0.0001)
        {
            return;
        }
        if self.drawn.len() >= 2048 {
            self.drawn = self.drawn.iter().step_by(2).copied().collect();
        }
        self.drawn.push(p);
    }
    pub fn sample(&mut self, p: [f32; 2], size: f32, time: f64, zoom: f32) {
        let p = Vec2::from(p);
        if p.distance(self.anchor) * zoom > 2.5 {
            self.anchor = p;
            self.moved_at = time;
            self.attempted = false;
        }
        if p.distance(self.samples.last().unwrap().0) * zoom < 0.6 {
            return;
        }
        if self.samples.len() >= 2048 {
            self.samples = self.samples.iter().step_by(2).copied().collect();
        }
        self.samples.push((p, size));
    }
}
struct Drag {
    shape: Shape,
    start: Vec2,
    node: Option<usize>,
}
const MORPH_DURATION: f64 = 0.24;
struct Transition {
    start: f64,
    last_frame: f64,
    from: Vec<Vec2>,
}
fn resample_path(points: &[Vec2], count: usize) -> Vec<Vec2> {
    let lengths: Vec<_> = points.windows(2).map(|p| p[0].distance(p[1])).collect();
    let total: f32 = lengths.iter().sum();
    if points.len() < 2 || total < 0.0001 {
        return vec![points[0]; count];
    }
    let (mut edge, mut traveled) = (0, 0.0);
    (0..count)
        .map(|i| {
            let target = total * i as f32 / (count - 1) as f32;
            while edge + 1 < lengths.len() && traveled + lengths[edge] < target {
                traveled += lengths[edge];
                edge += 1;
            }
            points[edge].lerp(
                points[edge + 1],
                ((target - traveled) / lengths[edge].max(0.0001)).clamp(0.0, 1.0),
            )
        })
        .collect()
}
fn morph_progress(elapsed: f64) -> f32 {
    let t = (elapsed / MORPH_DURATION).clamp(0.0, 1.0) as f32;
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
pub(super) struct Edit {
    pub editing: bool,
    shape: Shape,
    base: Shape,
    brush: Brush,
    erase: bool,
    layer: usize,
    samples: Vec<(Vec2, f32)>,
    original: crate::document::StrokeTiles,
    copies: Vec<crate::rulers::Transform>,
    hold: Vec2,
    phase: f32,
    reverse: bool,
    drag: Option<Drag>,
    transition: Option<Transition>,
}
impl Edit {
    fn new(app: &PaintAtlas, shape: Shape, samples: Vec<(Vec2, f32)>, erase: bool) -> Self {
        let path = shape.path();
        let n = path.len() - usize::from(shape.closed);
        let first = (0..n)
            .min_by(|a, b| {
                path[*a]
                    .distance_sq(samples[0].0)
                    .total_cmp(&path[*b].distance_sq(samples[0].0))
            })
            .unwrap_or(0);
        let area = |p: &[Vec2]| {
            p.windows(2)
                .map(|q| q[0].x * q[1].y - q[1].x * q[0].y)
                .sum::<f32>()
        };
        let raw: Vec<_> = samples.iter().map(|p| p.0).collect();
        Self {
            editing: false,
            base: shape.clone(),
            shape,
            brush: app.brush(),
            erase,
            layer: app.doc.active,
            original: app.doc.capture_stroke_tiles(),
            copies: app.doc.ruler_copies.clone(),
            hold: samples.last().unwrap().0,
            samples,
            phase: first as f32 / n.max(1) as f32,
            reverse: area(&raw) * area(&path) < 0.0,
            drag: None,
            transition: None,
        }
    }
    fn hold_shape(&mut self, pointer: Vec2, regular: bool) -> bool {
        let pivot = if self.base.closed {
            self.base.center()
        } else {
            self.base.nodes[0]
        };
        let a = self.hold - pivot;
        let b = pointer - pivot;
        let scale = if a.length() > 1.0 {
            (b.length() / a.length()).clamp(0.05, 8.0)
        } else {
            1.0
        };
        let mut angle = b.angle() - a.angle();
        if regular {
            let step = 15.0_f32.to_radians();
            let orientation = (self.base.nodes[1] - self.base.nodes[0]).angle();
            angle = ((orientation + angle) / step).round() * step - orientation;
        }
        let base = if regular {
            self.base.regular()
        } else {
            self.base.clone()
        };
        let shape = base.transformed(pivot, scale, angle, Vec2::ZERO);
        if shape == self.shape {
            false
        } else {
            self.shape = shape;
            true
        }
    }
    fn restore_original(&self, doc: &mut Document) {
        doc.restore_stroke_tiles();
        doc.write_stroke_tiles(&self.original);
    }
    fn start_transition(&mut self, ctx: &egui::Context, reduced_motion: bool, drawn: &[Vec2]) {
        if reduced_motion {
            return;
        }
        let now = ctx.input(|i| i.time);
        self.transition = Some(Transition {
            start: now,
            last_frame: now,
            from: resample_path(drawn, 512),
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
    fn animate(&mut self, doc: &mut Document, ctx: &egui::Context, reduced_motion: bool) -> bool {
        let Some(mut transition) = self.transition.take() else {
            return false;
        };
        let now = ctx.input(|i| i.time);
        let elapsed = now - transition.start;
        if reduced_motion || elapsed >= MORPH_DURATION {
            self.render(doc);
            return true;
        }
        let wait = (1.0 / 60.0 - (now - transition.last_frame)).max(0.0);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.001)));
        if wait > 0.001 {
            self.transition = Some(transition);
            return false;
        }
        transition.last_frame = now;
        let target = resample_path(&self.ordered_path(), transition.from.len());
        let t = morph_progress(elapsed);
        let path: Vec<_> = transition
            .from
            .iter()
            .zip(target)
            .map(|(a, b)| a.lerp(b, t))
            .collect();
        self.render_path(doc, &path);
        self.transition = Some(transition);
        true
    }
    fn ordered_path(&self) -> Vec<Vec2> {
        let mut path = self.shape.path();
        if self.shape.closed {
            path.pop();
            let n = path.len();
            path.rotate_left((self.phase * n as f32).round() as usize % n);
            if self.reverse {
                path[1..].reverse();
            }
            path.push(path[0]);
        }
        path
    }
    fn render(&self, doc: &mut Document) {
        self.render_path(doc, &self.ordered_path());
    }
    fn render_path(&self, doc: &mut Document, path: &[Vec2]) {
        doc.restore_stroke_tiles();
        let previous = std::mem::replace(&mut doc.ruler_copies, self.copies.clone());
        let total = path
            .windows(2)
            .map(|p| p[0].distance(p[1]))
            .sum::<f32>()
            .max(0.001);
        let mut source = Vec::with_capacity(self.samples.len());
        let mut length = 0.0;
        source.push((0.0, self.samples[0].1));
        for p in self.samples.windows(2) {
            length += p[0].0.distance(p[1].0);
            source.push((length, p[1].1));
        }
        let mut sample_index = 0;
        let mut size_at = |t: f32| {
            let at = t * length;
            while sample_index + 1 < source.len() - 1 && source[sample_index + 1].0 < at {
                sample_index += 1;
            }
            let a = source[sample_index];
            let b = source[(sample_index + 1).min(source.len() - 1)];
            a.1 + (b.1 - a.1) * ((at - a.0) / (b.0 - a.0).max(0.001)).clamp(0.0, 1.0)
        };
        let mut brush = self.brush.clone();
        brush.size = size_at(0.0);
        doc.stamp(path[0].into(), brush.clone(), self.erase);
        let mut next = (brush.size * brush.spacing.clamp(0.05, 2.0)).max(0.5);
        let mut traveled = 0.0;
        for edge in path.windows(2) {
            let distance = edge[0].distance(edge[1]);
            if distance > 0.0001 {
                while next <= traveled + distance {
                    let t = ((next - traveled) / distance).clamp(0.0, 1.0);
                    brush.size = size_at(next / total);
                    doc.stamp(edge[0].lerp(edge[1], t).into(), brush.clone(), self.erase);
                    next += (brush.size * brush.spacing.clamp(0.05, 2.0)).max(0.5);
                }
            }
            traveled += distance;
        }
        doc.ruler_copies = previous;
    }
}
impl PaintAtlas {
    pub(super) fn quickshape_smoke(&mut self) {
        self.doc.layers[self.doc.active].pixels.fill(0);
        self.doc.layers[self.doc.active].alpha_lock = false;
        self.tool = Tool::Brush;
        self.pressure_enabled = false;
        self.size = 6.0;
        self.opacity = 1.0;
        self.hardness = 1.0;
        self.active_tip = None;
        self.color = Color32::from_rgb(70, 150, 235);
        self.history.push_stroke(&mut self.doc);
        let samples: Vec<_> = (0..=140)
            .map(|i| {
                let t = i as f32 / 140.0 * std::f32::consts::TAU;
                let r = 1.0 + (t * 7.0).sin() * 0.035;
                (
                    Vec2::new(320.0 + 170.0 * t.cos() * r, 240.0 + 95.0 * t.sin() * r),
                    6.0,
                )
            })
            .collect();
        let brush = self.brush();
        self.doc.stamp(samples[0].0.into(), brush.clone(), false);
        let mut remaining = 1.0;
        for pair in samples.windows(2) {
            self.doc.stroke_segment(
                pair[0].0.into(),
                pair[1].0.into(),
                6.0,
                brush.clone(),
                false,
                &mut remaining,
            );
        }
        if let Some(shape) =
            crate::quickshape::recognize(&samples.iter().map(|s| s.0).collect::<Vec<_>>(), 1.0)
        {
            let mut edit = Edit::new(self, shape, samples, false);
            edit.editing = true;
            edit.render(&mut self.doc);
            self.quick_shape = Some(edit);
        }
        self.changed();
    }
    pub(super) fn finish_quickshape(&mut self, original: bool) {
        if let Some(edit) = self.quick_shape.take() {
            if original && self.doc.active == edit.layer && self.doc.stroke_tiles.borrow().is_some()
            {
                edit.restore_original(&mut self.doc);
                self.changed();
            }
            if !original && edit.transition.is_some() {
                edit.render(&mut self.doc);
                self.changed();
            }
            self.history.commit(&self.doc);
        }
        self.quick_shape_toolbar = None;
    }
    pub(super) fn quickshape_pre_input(&mut self, ctx: &egui::Context) {
        if self.quick_shape.is_some()
            && !ctx.wants_keyboard_input()
            && !self.preferences.capturing_shortcut()
        {
            let (apply, cancel) = ctx.input_mut(|i| {
                (
                    self.settings
                        .shortcuts
                        .consume(i, Shortcut::QuickShapeApply),
                    self.settings
                        .shortcuts
                        .consume(i, Shortcut::QuickShapeCancel),
                )
            });
            if apply || cancel {
                self.finish_quickshape(cancel);
                return;
            }
        }
        let Some(edit) = &self.quick_shape else {
            return;
        };
        if self.doc.active != edit.layer || self.doc.stroke_tiles.borrow().is_none() {
            self.quick_shape = None;
            self.quick_shape_toolbar = None;
            return;
        }
        if edit.editing {
            return;
        }
        let finish = ctx.input(|i| {
            i.events.iter().any(|e| match e {
                egui::Event::PointerButton {
                    pos, pressed: true, ..
                } => self.quick_shape_toolbar.is_none_or(|r| !r.contains(*pos)),
                egui::Event::Key { pressed: true, .. }
                | egui::Event::Paste(_)
                | egui::Event::Copy
                | egui::Event::Cut => true,
                _ => false,
            })
        });
        if finish {
            self.finish_quickshape(false);
        }
    }
    pub(super) fn try_quickshape(
        &mut self,
        gesture: &mut Gesture,
        ctx: &egui::Context,
        view: CanvasView,
    ) {
        if !ctx.input(|i| i.focused) {
            return;
        }
        let Some(stroke) = &mut gesture.quick else {
            return;
        };
        if stroke.disabled || stroke.attempted || stroke.active.is_some() {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now - stroke.moved_at < self.settings.quickshape.delay as f64 {
            ctx.request_repaint_after(std::time::Duration::from_millis(12));
            return;
        }
        stroke.attempted = true;
        let points: Vec<_> = stroke.samples.iter().map(|p| p.0).collect();
        let Some(shape) = crate::quickshape::recognize(&points, view.zoom) else {
            return;
        };
        // Finish stabilization before capturing the original freehand pixels.
        if gesture.last != gesture.raw_last {
            for p in gesture
                .filter
                .finish(gesture.raw_last, self.stabilization, 0.5)
            {
                self.doc.stroke_segment(
                    gesture.last,
                    p,
                    gesture.last_size,
                    self.brush(),
                    gesture.tool == Tool::Eraser,
                    &mut gesture.distance_to_dab,
                );
                stroke.record(p);
                gesture.last = p;
                gesture.last_size = self.brush().size;
            }
        }
        let mut edit = Edit::new(
            self,
            shape,
            stroke.samples.clone(),
            gesture.tool == Tool::Eraser,
        );
        let regular = ctx.input(|i| self.settings.shortcuts.held(i, Shortcut::QuickShapeRegular));
        edit.hold_shape(edit.hold, regular);
        edit.start_transition(ctx, self.settings.appearance.reduced_motion, &stroke.drawn);
        if edit.transition.is_none() {
            edit.render(&mut self.doc);
        }
        self.status = format!(
            "QuickShape: {} · solte para editar",
            edit.shape.kind.label()
        );
        stroke.active = Some(edit);
        self.changed();
    }
    pub(super) fn quickshape_hold(
        &mut self,
        gesture: &mut Gesture,
        ctx: &egui::Context,
        view: CanvasView,
        down: bool,
    ) {
        let stroke = gesture.quick.as_mut().unwrap();
        if stroke.disabled {
            return;
        }
        if ctx.input_mut(|i| {
            self.settings
                .shortcuts
                .consume(i, Shortcut::QuickShapeCancel)
        }) {
            let edit = stroke.active.take().unwrap();
            edit.restore_original(&mut self.doc);
            stroke.disabled = true;
            self.changed();
            if !down {
                self.history.commit(&self.doc);
            }
            return;
        }
        let edit = stroke.active.as_mut().unwrap();
        let (p, regular) = ctx.input(|i| {
            (
                i.pointer.interact_pos(),
                self.settings.shortcuts.held(i, Shortcut::QuickShapeRegular),
            )
        });
        if let Some(p) = p {
            let p = Vec2::from(view.document(p)).clamp(
                Vec2::splat(-4096.0),
                Vec2::new(
                    self.doc.width as f32 + 4096.0,
                    self.doc.height as f32 + 4096.0,
                ),
            );
            // Tablet jitter and subpixel positions omitted by Stroke::sample
            // still change the target geometry. Keep morphing toward that target:
            // cancelling here made a real pen skip straight to the final shape.
            if edit.hold_shape(p, regular) && edit.transition.is_none() {
                edit.render(&mut self.doc);
                self.changed();
            }
        }
        if edit.animate(&mut self.doc, ctx, self.settings.appearance.reduced_motion) {
            self.changed();
        }
        if !down {
            self.quick_shape = stroke.active.take();
        }
    }
    pub(super) fn quickshape_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        view: CanvasView,
        panning: bool,
    ) -> bool {
        self.quickshape_pre_input(ui.ctx());
        let Some(mut edit) = self.quick_shape.take() else {
            return false;
        };
        let ctx = ui.ctx();
        if edit.animate(&mut self.doc, ctx, self.settings.appearance.reduced_motion) {
            self.changed();
        }
        if !edit.editing {
            self.quick_shape = Some(edit);
            return false;
        }
        if panning {
            edit.drag = None;
            self.quick_shape = Some(edit);
            return true;
        }
        let (apply, cancel, p, pressed, down, regular) = ctx.input_mut(|i| {
            (
                self.settings
                    .shortcuts
                    .consume(i, Shortcut::QuickShapeApply),
                self.settings
                    .shortcuts
                    .consume(i, Shortcut::QuickShapeCancel),
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                self.settings.shortcuts.held(i, Shortcut::QuickShapeRegular),
            )
        });
        if apply || cancel {
            self.quick_shape = Some(edit);
            self.finish_quickshape(cancel);
            return true;
        }
        if pressed
            && response.hovered()
            && let Some(p) = p
        {
            let node = edit
                .shape
                .nodes
                .iter()
                .enumerate()
                .find(|(_, q)| view.screen((**q).into()).distance(p) < 12.0)
                .map(|(i, _)| i);
            let path = edit.shape.path();
            let near = path.windows(2).any(|q| {
                let a = view.screen(q[0].into());
                let b = view.screen(q[1].into());
                let d = b - a;
                (a + d * ((p - a).dot(d) / d.length_sq().max(0.001)).clamp(0.0, 1.0)).distance(p)
                    < 12.0
            });
            if node.is_none() && !near {
                self.quick_shape = Some(edit);
                self.finish_quickshape(false);
                return true;
            }
            edit.drag = Some(Drag {
                shape: edit.shape.clone(),
                start: view.document(p).into(),
                node,
            });
        }
        if let Some(drag) = &edit.drag
            && let Some(p) = p
        {
            let p = Vec2::from(view.document(p)).clamp(
                Vec2::splat(-4096.0),
                Vec2::new(
                    self.doc.width as f32 + 4096.0,
                    self.doc.height as f32 + 4096.0,
                ),
            );
            let mut shape = drag.shape.clone();
            if let Some(node) = drag.node {
                shape.move_node(node, p, regular);
            } else {
                shape = shape.transformed(Vec2::ZERO, 1.0, 0.0, p - drag.start);
            }
            if shape != edit.shape {
                edit.transition = None;
                edit.shape = shape;
                edit.render(&mut self.doc);
                self.changed();
            }
        }
        if !down {
            edit.drag = None;
        }
        self.quick_shape = Some(edit);
        true
    }
    pub(super) fn quickshape_overlay(&mut self, ui: &egui::Ui, view: CanvasView) {
        let Some(mut edit) = self.quick_shape.take() else {
            return;
        };
        if edit.editing {
            let painter = ui.painter();
            for p in &edit.shape.nodes {
                let p = view.screen((*p).into());
                painter.circle_filled(p, 5.0, Color32::from_gray(25));
                painter.circle_stroke(p, 5.0, egui::Stroke::new(1.5_f32, Color32::WHITE));
            }
        }
        let width = ui.max_rect().width().min(620.0);
        let position = Pos2::new(
            ui.max_rect().center().x - width * 0.5,
            ui.max_rect().top() + 10.0,
        );
        let mut finish = None;
        let mut changed = false;
        let area=egui::Area::new(egui::Id::new("quickshape_toolbar")).order(egui::Order::Foreground).fixed_pos(position).show(ui.ctx(),|ui| {
            egui::Frame::popup(ui.style()).corner_radius(12).show(ui,|ui| {
                ui.set_max_width(width-20.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("QuickShape · {}",edit.shape.kind.label()));
                    if !edit.editing {
                        if ui.button("Editar forma").clicked() {
                            edit.editing=true;
                            changed |= edit.transition.take().is_some();
                        }
                    } else {
                        let mut kind=edit.shape.kind;
                        egui::ComboBox::from_id_salt("quickshape_kind").selected_text(kind.label()).show_ui(ui,|ui| {
                            for k in Kind::ALL { ui.selectable_value(&mut kind,k,k.label()); }
                        });
                        if kind!=edit.shape.kind { edit.shape=edit.shape.convert(kind); changed=true; }
                        if ui.button("Regularizar").clicked() { edit.shape=edit.shape.regular(); changed=true; }
                    }
                    if ui.button("Aplicar").on_hover_text(self.settings.shortcuts.label(Shortcut::QuickShapeApply)).clicked() { finish=Some(false); }
                    if ui.button("Traço original").on_hover_text(self.settings.shortcuts.label(Shortcut::QuickShapeCancel)).clicked() { finish=Some(true); }
                });
                if edit.editing { crate::theme::caption(ui,"Arraste as alças para ajustar; arraste o contorno para mover. Clique fora para aplicar."); }
            });
        });
        self.quick_shape_toolbar = Some(area.response.rect);
        if changed {
            edit.render(&mut self.doc);
            self.changed();
        }
        self.quick_shape = Some(edit);
        if let Some(original) = finish {
            self.finish_quickshape(original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        app: &mut PaintAtlas,
        ctx: &egui::Context,
        time: f64,
        modifiers: egui::Modifiers,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(640.0, 480.0),
                )),
                time: Some(time),
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| app.canvas(ctx),
        );
    }
    fn button(p: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn setup(ctx: &egui::Context, erase: bool) -> PaintAtlas {
        let mut app = PaintAtlas::from_context(ctx);
        app.reset(Document::new(512, 512, false), None, false);
        app.fit = false;
        app.zoom = 1.0;
        app.pan = Vec2::ZERO;
        app.settings.quickshape = Default::default();
        app.settings.quickshape.delay = 0.3;
        app.settings.appearance.reduced_motion = true;
        app.tool = if erase { Tool::Eraser } else { Tool::Brush };
        app.pressure_enabled = false;
        app.stabilization = 0.0;
        app.size = 5.0;
        app.opacity = 1.0;
        app.hardness = 1.0;
        app.active_tip = None;
        if erase {
            app.doc.layers[0].pixels = [80, 100, 220, 255].repeat(512 * 512);
        }
        frame(&mut app, ctx, 0.0, egui::Modifiers::NONE, vec![]);
        app
    }
    fn draw(app: &mut PaintAtlas, ctx: &egui::Context) -> Vec<u8> {
        let p = Pos2::new(160.0, 190.0);
        frame(
            app,
            ctx,
            0.1,
            egui::Modifiers::NONE,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        let points = (1..40)
            .map(|i| {
                egui::Event::PointerMoved(Pos2::new(
                    160.0 + i as f32 * 4.0,
                    190.0 + i as f32 * 0.3 + (i as f32).sin() * 3.0,
                ))
            })
            .collect();
        frame(app, ctx, 0.2, egui::Modifiers::NONE, points);
        app.doc.layers[0].pixels.clone()
    }
    #[test]
    fn morph_moves_geometry_continues_after_release_and_finishes_exactly() {
        for erase in [false, true] {
            let ctx = egui::Context::default();
            let mut app = setup(&ctx, erase);
            app.settings.appearance.reduced_motion = false;
            let original = draw(&mut app, &ctx);
            let pointer = ctx.input(|i| i.pointer.interact_pos().unwrap());
            frame(&mut app, &ctx, 0.7, egui::Modifiers::NONE, vec![]);
            assert_eq!(app.doc.layers[0].pixels, original);
            let edit = app
                .gesture
                .as_ref()
                .unwrap()
                .quick
                .as_ref()
                .unwrap()
                .active
                .as_ref()
                .unwrap();
            assert!(edit.transition.is_some());
            let mut target = app.doc.clone();
            edit.render(&mut target);
            let final_pixels = target.layers[0].pixels.clone();
            assert_ne!(final_pixels, original);
            frame(
                &mut app,
                &ctx,
                0.72,
                egui::Modifiers::NONE,
                vec![button(pointer, false)],
            );
            assert!(app.quick_shape.as_ref().unwrap().transition.is_some());
            frame(&mut app, &ctx, 0.82, egui::Modifiers::NONE, vec![]);
            assert!(app.quick_shape.as_ref().unwrap().transition.is_some());
            assert_ne!(app.doc.layers[0].pixels, original);
            assert_ne!(app.doc.layers[0].pixels, final_pixels);
            frame(&mut app, &ctx, 0.96, egui::Modifiers::NONE, vec![]);
            assert!(app.quick_shape.as_ref().unwrap().transition.is_none());
            assert_eq!(app.doc.layers[0].pixels, final_pixels);
        }
    }
    #[test]
    fn subpixel_pen_motion_and_regular_modifier_do_not_interrupt_morph() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, false);
        app.settings.appearance.reduced_motion = false;
        draw(&mut app, &ctx);
        let pointer = ctx.input(|i| i.pointer.interact_pos().unwrap());
        // Real pens keep emitting positions below the recognizer's 0.6 px
        // sample threshold, even while held still. Its last sample is stale.
        let pointer = pointer + Vec2::new(0.25, 0.15);
        frame(
            &mut app,
            &ctx,
            0.25,
            egui::Modifiers::NONE,
            vec![egui::Event::PointerMoved(pointer)],
        );
        frame(&mut app, &ctx, 0.7, egui::Modifiers::NONE, vec![]);
        let original = app.doc.layers[0].pixels.clone();
        for (time, offset, modifiers) in [
            (0.74, Vec2::ZERO, egui::Modifiers::NONE),
            (0.78, Vec2::new(0.1, -0.1), egui::Modifiers::NONE),
            (0.82, Vec2::new(0.2, 0.1), egui::Modifiers::SHIFT),
        ] {
            frame(
                &mut app,
                &ctx,
                time,
                modifiers,
                vec![egui::Event::PointerMoved(pointer + offset)],
            );
            let edit = app
                .gesture
                .as_ref()
                .unwrap()
                .quick
                .as_ref()
                .unwrap()
                .active
                .as_ref()
                .unwrap();
            assert!(
                edit.transition.is_some(),
                "pen jitter interrupted morph at {time}"
            );
        }
        assert_ne!(app.doc.layers[0].pixels, original);
        frame(&mut app, &ctx, 0.96, egui::Modifiers::SHIFT, vec![]);
        let edit = app
            .gesture
            .as_ref()
            .unwrap()
            .quick
            .as_ref()
            .unwrap()
            .active
            .as_ref()
            .unwrap();
        assert!(edit.transition.is_none());
        let mut target = app.doc.clone();
        edit.render(&mut target);
        assert_eq!(app.doc.layers[0].pixels, target.layers[0].pixels);
    }
    #[test]
    fn applying_or_cancelling_during_morph_preserves_exact_pixels_and_history() {
        for cancel in [false, true] {
            let ctx = egui::Context::default();
            let mut app = setup(&ctx, false);
            app.settings.appearance.reduced_motion = false;
            let before = app.doc.layers[0].pixels.clone();
            let original = draw(&mut app, &ctx);
            let pointer = ctx.input(|i| i.pointer.interact_pos().unwrap());
            frame(&mut app, &ctx, 0.7, egui::Modifiers::NONE, vec![]);
            frame(
                &mut app,
                &ctx,
                0.76,
                egui::Modifiers::NONE,
                vec![button(pointer, false)],
            );
            let edit = app.quick_shape.as_ref().unwrap();
            assert!(edit.transition.is_some());
            let mut target = app.doc.clone();
            edit.render(&mut target);
            let expected = if cancel {
                original
            } else {
                target.layers[0].pixels.clone()
            };
            app.finish_quickshape(cancel);
            assert_eq!(app.doc.layers[0].pixels, expected);
            assert_eq!(app.history.counts(), (1, 0));
            assert!(app.history.undo(&mut app.doc));
            assert_eq!(app.doc.layers[0].pixels, before);
            assert!(app.history.redo(&mut app.doc));
            assert_eq!(app.doc.layers[0].pixels, expected);
        }
    }
    #[test]
    fn easing_and_path_resampling_have_stable_endpoints() {
        assert_eq!(morph_progress(-1.0), 0.0);
        assert_eq!(morph_progress(MORPH_DURATION), 1.0);
        assert!((morph_progress(MORPH_DURATION * 0.5) - 0.5).abs() < 0.00001);
        let progress: Vec<_> = (0..=100)
            .map(|i| morph_progress(MORPH_DURATION * i as f64 / 100.0))
            .collect();
        assert!(progress.windows(2).all(|p| p[0] <= p[1]));
        let path = resample_path(&[Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(10.0, 0.0)], 6);
        assert_eq!(
            path,
            (0..6)
                .map(|i| Vec2::new(i as f32 * 2.0, 0.0))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn draw_hold_corrects_and_release_keeps_a_single_undo_action() {
        for erase in [false, true] {
            let ctx = egui::Context::default();
            let mut app = setup(&ctx, erase);
            let before = app.doc.layers[0].pixels.clone();
            let original = draw(&mut app, &ctx);
            frame(&mut app, &ctx, 0.7, egui::Modifiers::NONE, vec![]);
            assert!(
                app.gesture
                    .as_ref()
                    .unwrap()
                    .quick
                    .as_ref()
                    .unwrap()
                    .active
                    .is_some()
            );
            assert_ne!(app.doc.layers[0].pixels, original);
            let p = Pos2::new(316.0, 201.0);
            frame(
                &mut app,
                &ctx,
                0.8,
                egui::Modifiers::NONE,
                vec![button(p, false)],
            );
            assert!(app.quick_shape.is_some());
            assert!(app.gesture.is_none());
            app.finish_quickshape(false);
            let corrected = app.doc.layers[0].pixels.clone();
            assert_eq!(app.history.counts(), (1, 0));
            assert!(app.history.undo(&mut app.doc));
            assert_eq!(app.doc.layers[0].pixels, before);
            assert!(app.history.redo(&mut app.doc));
            assert_eq!(app.doc.layers[0].pixels, corrected);
        }
    }
    #[test]
    fn cancelling_after_node_edits_restores_exact_freehand_and_one_undo() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, false);
        let original = draw(&mut app, &ctx);
        frame(&mut app, &ctx, 0.7, egui::Modifiers::NONE, vec![]);
        frame(
            &mut app,
            &ctx,
            0.8,
            egui::Modifiers::NONE,
            vec![button(Pos2::new(316.0, 201.0), false)],
        );
        let mut edit = app.quick_shape.take().unwrap();
        edit.editing = true;
        edit.shape = edit.shape.convert(Kind::Ellipse);
        edit.shape.move_node(1, Vec2::new(420.0, 280.0), false);
        edit.render(&mut app.doc);
        assert_ne!(app.doc.layers[0].pixels, original);
        app.quick_shape = Some(edit);
        app.finish_quickshape(true);
        assert_eq!(app.doc.layers[0].pixels, original);
        assert_eq!(app.history.counts(), (1, 0));
        assert!(app.history.undo(&mut app.doc));
        assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
    }
    #[test]
    fn disabled_and_short_strokes_remain_freehand_and_hold_can_resize() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, false);
        app.settings.quickshape.enabled = false;
        let original = draw(&mut app, &ctx);
        frame(&mut app, &ctx, 0.8, egui::Modifiers::NONE, vec![]);
        assert!(app.gesture.as_ref().unwrap().quick.is_none());
        assert_eq!(app.doc.layers[0].pixels, original);
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, false);
        draw(&mut app, &ctx);
        frame(&mut app, &ctx, 0.7, egui::Modifiers::NONE, vec![]);
        let initial = app
            .gesture
            .as_ref()
            .unwrap()
            .quick
            .as_ref()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .shape
            .clone();
        frame(
            &mut app,
            &ctx,
            0.8,
            egui::Modifiers::SHIFT,
            vec![egui::Event::PointerMoved(Pos2::new(440.0, 220.0))],
        );
        let shape = &app
            .gesture
            .as_ref()
            .unwrap()
            .quick
            .as_ref()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .shape;
        assert!(
            (shape.nodes[1] - shape.nodes[0]).length()
                > (initial.nodes[1] - initial.nodes[0]).length()
        );
        let angle = (shape.nodes[1] - shape.nodes[0]).angle().to_degrees();
        assert!((angle / 15.0 - (angle / 15.0).round()).abs() < 0.001);
    }
    #[test]
    fn ordinary_pen_release_does_not_trigger_and_cancellation_owns_pointer_until_release() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, false);
        let original = draw(&mut app, &ctx);
        frame(
            &mut app,
            &ctx,
            0.25,
            egui::Modifiers::NONE,
            vec![button(Pos2::new(316.0, 202.0), false)],
        );
        assert!(app.quick_shape.is_none());
        assert!(app.gesture.is_none());
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, false);
        let original2 = draw(&mut app, &ctx);
        frame(&mut app, &ctx, 0.7, egui::Modifiers::NONE, vec![]);
        frame(
            &mut app,
            &ctx,
            0.8,
            egui::Modifiers::NONE,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.doc.layers[0].pixels, original2);
        frame(
            &mut app,
            &ctx,
            0.9,
            egui::Modifiers::NONE,
            vec![egui::Event::PointerMoved(Pos2::new(500.0, 400.0))],
        );
        assert_eq!(app.doc.layers[0].pixels, original2);
        assert!(original.iter().any(|p| *p != 0));
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    #[test]
    fn texture_pressure_symmetry_and_selection_survive_shape_replacement() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(512, 512, false), None, false);
        let mask = std::sync::Arc::new(
            crate::selection::Mask::rectangle(
                512,
                512,
                egui::Rect::from_min_max(Pos2::new(0.0, 208.0), Pos2::new(512.0, 224.0)),
            )
            .unwrap(),
        );
        app.selection.mask = Some(mask.clone());
        app.doc.selection = Some(mask);
        app.doc.layers[0].rulers.push(crate::rulers::Ruler::new(
            crate::rulers::Kind::Symmetry,
            512,
            512,
        ));
        let mut image = image::RgbaImage::new(12, 8);
        for y in 0..8 {
            for x in 0..12 {
                if x < y + 3 {
                    image.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
                }
            }
        }
        app.active_tip = Some(std::sync::Arc::new(
            crate::brush_tip::BrushTip::from_rgba(image).unwrap(),
        ));
        app.pressure_enabled = false;
        app.size = 8.0;
        app.opacity = 0.7;
        app.hardness = 0.6;
        let samples: Vec<_> = (0..50)
            .map(|i| {
                (
                    Vec2::new(
                        96.0 + i as f32 * 3.0,
                        211.0 + i as f32 * 0.1 + (i as f32).sin() * 2.0,
                    ),
                    2.0 + i as f32 * 0.2,
                )
            })
            .collect();
        app.history.push_stroke(&mut app.doc);
        app.doc.begin_ruler_stroke(true, samples[0].0.into());
        let brush = app.brush();
        let mut remaining = 1.0;
        app.doc.stamp(samples[0].0.into(), brush.clone(), false);
        for pair in samples.windows(2) {
            app.doc.stroke_segment(
                pair[0].0.into(),
                pair[1].0.into(),
                8.0,
                brush.clone(),
                false,
                &mut remaining,
            );
        }
        let shape =
            crate::quickshape::recognize(&samples.iter().map(|p| p.0).collect::<Vec<_>>(), 1.0)
                .unwrap();
        let edit = Edit::new(&app, shape, samples, false);
        edit.render(&mut app.doc);
        let pixels = &app.doc.layers[0].pixels;
        assert!(pixels.iter().any(|p| *p != 0));
        for y in 0..512 {
            for x in 0..256 {
                let a = pixels[(y * 512 + x) * 4 + 3];
                let b = pixels[(y * 512 + 511 - x) * 4 + 3];
                assert!(a.abs_diff(b) <= 1, "{x},{y}: {a} vs {b}");
                if !(208..224).contains(&y) {
                    assert_eq!(a, 0);
                    assert_eq!(b, 0);
                }
            }
        }
        app.quick_shape = Some(edit);
        app.finish_quickshape(false);
        assert_eq!(app.history.counts(), (1, 0));
        assert!(app.history.undo(&mut app.doc));
        assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
    }
}
