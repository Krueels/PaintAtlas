use super::*;
use crate::rulers::{Kind, Ruler, Scope};

pub(super) struct State {
    pub selected: Option<usize>,
    pub kind: Kind,
    drag: Option<Drag>,
    property_edit: bool,
}
struct Drag {
    index: usize,
    handle: usize,
    start: [f32; 2],
    before: Ruler,
}
impl Default for State {
    fn default() -> Self {
        Self {
            selected: None,
            kind: Kind::Symmetry,
            drag: None,
            property_edit: false,
        }
    }
}
impl State {
    pub fn finish_pointer_edit(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| !i.pointer.any_down() || !i.focused) {
            self.property_edit = false;
            self.drag = None;
        }
    }
    pub fn editing(&self) -> bool {
        self.drag.is_some() || self.property_edit
    }
}
fn handles(r: &Ruler, zoom: f32) -> Vec<[f32; 2]> {
    if r.kind.perspective() > 0 {
        r.vanishing[..r.kind.perspective()].to_vec()
    } else {
        vec![
            r.origin,
            [
                r.origin[0] + r.angle.cos() * 85.0 / zoom,
                r.origin[1] + r.angle.sin() * 85.0 / zoom,
            ],
        ]
    }
}
impl PaintAtlas {
    pub(super) fn create_ruler(&mut self, kind: Kind) {
        if self.ruler_ui.editing()
            || self.gesture.is_some()
            || self.selection.floating.is_some()
            || self.image_editor.is_some()
        {
            return;
        }
        if self.doc.layers[self.doc.active].rulers.len() >= 8 {
            self.status = "Limite de 8 réguas por camada.".into();
            return;
        }
        if !self.settings.show_rulers {
            self.settings.show_rulers = true;
            self.settings_pending = true;
            self.settings_edited = std::time::Instant::now();
        }
        self.history.push_layer_properties(&self.doc);
        let ruler = Ruler::new(kind, self.doc.width, self.doc.height);
        let rulers = &mut self.doc.layers[self.doc.active].rulers;
        for r in rulers
            .iter_mut()
            .filter(|r| (r.kind == Kind::Symmetry) == (kind == Kind::Symmetry))
        {
            r.enabled = false;
        }
        rulers.push(ruler);
        self.ruler_ui.selected = Some(rulers.len() - 1);
        self.ruler_ui.kind = kind;
        self.select_tool(Tool::Ruler);
        self.dirty = true;
        self.status = "Régua criada. Arraste as alças; selecione o pincel para desenhar.".into();
    }
    pub(super) fn ruler_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Réguas", |ui| {
            self.ruler_toggles(ui);
            ui.separator();
            for kind in Kind::ALL {
                if ui.button(kind.label()).clicked() {
                    self.create_ruler(kind);
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .button(
                    self.settings
                        .shortcuts
                        .menu(Shortcut::RulerTool, "Editar réguas"),
                )
                .clicked()
            {
                self.select_tool(Tool::Ruler);
                ui.close();
            }
        });
    }
    fn ruler_toggles(&mut self, ui: &mut egui::Ui) {
        let changed = ui
            .checkbox(&mut self.settings.show_rulers, "Mostrar réguas")
            .changed()
            | ui.checkbox(&mut self.settings.snap_rulers, "Encaixar nas réguas")
                .changed();
        if changed {
            self.settings_pending = true;
            self.settings_edited = std::time::Instant::now();
        }
    }
    pub(super) fn ruler_properties(&mut self, ui: &mut egui::Ui) {
        self.ruler_toggles(ui);
        ui.separator();
        egui::ComboBox::from_id_salt("new_ruler_kind")
            .selected_text(self.ruler_ui.kind.label())
            .show_ui(ui, |ui| {
                for kind in Kind::ALL {
                    ui.selectable_value(&mut self.ruler_ui.kind, kind, kind.label());
                }
            });
        if crate::theme::button(ui, "Criar ao centro").clicked() {
            self.create_ruler(self.ruler_ui.kind);
        }
        crate::theme::caption(
            ui,
            format!(
                "Arraste no espaço vazio para criar. Arraste as alças para editar. {}: ângulos de 15°.",
                self.settings.shortcuts.label(Shortcut::RulerAngleSnap)
            ),
        );
        ui.separator();
        let active = self.doc.active;
        for (i, r) in self.doc.layers[active].rulers.iter().enumerate() {
            let label = format!(
                "{} {}{}",
                i + 1,
                r.kind.label(),
                if r.enabled { " · ativa" } else { "" }
            );
            if ui
                .selectable_label(self.ruler_ui.selected == Some(i), label)
                .clicked()
            {
                self.ruler_ui.selected = Some(i);
            }
        }
        let Some(index) = self
            .ruler_ui
            .selected
            .filter(|i| *i < self.doc.layers[active].rulers.len())
        else {
            return;
        };
        let mut ruler = self.doc.layers[active].rulers[index].clone();
        let before = ruler.clone();
        ui.separator();
        ui.checkbox(&mut ruler.visible, "Visível");
        ui.checkbox(&mut ruler.enabled, "Encaixe ativo");
        egui::ComboBox::from_id_salt("ruler_scope")
            .selected_text(ruler.scope.label())
            .show_ui(ui, |ui| {
                for scope in [Scope::Layer, Scope::Folder, Scope::All] {
                    ui.selectable_value(&mut ruler.scope, scope, scope.label());
                }
            });
        if ruler.kind == Kind::Symmetry {
            ui.add(egui::Slider::new(&mut ruler.lines, 2..=16).text("Linhas"));
            if ruler.lines.is_multiple_of(2) {
                ui.checkbox(&mut ruler.mirror, "Simetria por espelhamento");
            } else {
                ruler.mirror = false;
                ui.label("Número ímpar: simetria por rotação");
            }
        }
        if ruler.kind.perspective() == 0 {
            let mut degrees = ruler.angle.to_degrees();
            if ui
                .add(egui::DragValue::new(&mut degrees).speed(0.5).suffix("°"))
                .changed()
            {
                ruler.angle = degrees.to_radians();
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Vertical").clicked() {
                    ruler.angle = std::f32::consts::FRAC_PI_2;
                }
                if ui.button("Horizontal").clicked() {
                    ruler.angle = 0.0;
                }
                if ui.button("Centralizar").clicked() {
                    ruler.origin = [self.doc.width as f32 / 2.0, self.doc.height as f32 / 2.0];
                }
            });
        } else {
            for i in 0..ruler.kind.perspective() {
                ui.checkbox(
                    &mut ruler.directions[i],
                    format!("Encaixar no ponto de fuga {}", i + 1),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut ruler.vanishing[i][0])
                            .speed(1.0)
                            .range(-1_000_000.0..=1_000_000.0)
                            .prefix("X "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut ruler.vanishing[i][1])
                            .speed(1.0)
                            .range(-1_000_000.0..=1_000_000.0)
                            .prefix("Y "),
                    );
                });
            }
            crate::theme::caption(
                ui,
                "Arraste os pontos de fuga. Volte ao início do traço para escolher outra direção.",
            );
        }
        let mut delete = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Duplicar").clicked() && self.doc.layers[active].rulers.len() < 8 {
                self.history.push_layer_properties(&self.doc);
                let mut copy = ruler.clone();
                copy.enabled = false;
                self.doc.layers[active].rulers.push(copy);
                self.dirty = true;
            }
            delete = ui.button("Excluir régua").clicked();
        });
        if delete {
            self.history.push_layer_properties(&self.doc);
            self.doc.layers[active].rulers.remove(index);
            self.ruler_ui.selected = None;
            self.ruler_ui.property_edit = false;
            self.dirty = true;
        } else if ruler != before {
            if !self.ruler_ui.property_edit {
                self.history.push_layer_properties(&self.doc);
            }
            self.ruler_ui.property_edit = ui.ctx().input(|i| i.pointer.any_down());
            if ruler.enabled && !before.enabled {
                for r in &mut self.doc.layers[active].rulers {
                    if (r.kind == Kind::Symmetry) == (ruler.kind == Kind::Symmetry) {
                        r.enabled = false;
                    }
                }
            }
            self.doc.layers[active].rulers[index] = ruler;
            self.dirty = true;
        }
        if !ui.ctx().input(|i| i.pointer.any_down()) {
            self.ruler_ui.property_edit = false;
        }
    }
    pub(super) fn ruler_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        view: CanvasView,
        panning: bool,
    ) -> bool {
        if self.tool != Tool::Ruler
            || panning
            || self.pending.is_some()
            || self.new_dialog
            || self.image_editor.is_some()
            || self.selection.floating.is_some()
        {
            self.ruler_ui.drag = None;
            return false;
        }
        let ctx = ui.ctx();
        let (pointer, pressed, down, angle_snap, focused) = ctx.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                self.settings.shortcuts.held(i, Shortcut::RulerAngleSnap),
                i.focused,
            )
        });
        if !focused {
            self.ruler_ui.drag = None;
            return true;
        }
        if pressed
            && response.hovered()
            && let Some(p) = pointer
        {
            let hit = self.doc.layers[self.doc.active]
                .rulers
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, r)| {
                    handles(r, view.zoom)
                        .iter()
                        .position(|q| view.screen(*q).distance(p) <= 12.0)
                        .map(|handle| (index, handle))
                });
            let (index, handle) = if let Some(hit) = hit {
                self.history.push_layer_properties(&self.doc);
                hit
            } else {
                if self.doc.layers[self.doc.active].rulers.len() >= 8 {
                    return true;
                }
                self.create_ruler(self.ruler_ui.kind);
                let Some(index) = self.ruler_ui.selected else {
                    return true;
                };
                let origin = view.document(p);
                let r = &mut self.doc.layers[self.doc.active].rulers[index];
                let offset = Vec2::from(origin) - Vec2::from(r.origin);
                r.map_geometry(|q| (Vec2::from(q) + offset).into());
                (index, if r.kind.perspective() > 0 { 3 } else { 1 })
            };
            self.ruler_ui.selected = Some(index);
            self.ruler_ui.drag = Some(Drag {
                index,
                handle,
                start: view.document(p),
                before: self.doc.layers[self.doc.active].rulers[index].clone(),
            });
        }
        if let Some(drag) = &self.ruler_ui.drag
            && let Some(p) = pointer
        {
            let q = view.document(p).map(|v| v.clamp(-1_000_000.0, 1_000_000.0));
            if let Some(r) = self.doc.layers[self.doc.active].rulers.get_mut(drag.index) {
                *r = drag.before.clone();
                if r.kind.perspective() > 0 {
                    if drag.handle < 3 {
                        r.vanishing[drag.handle] = q;
                    } else {
                        let delta = Vec2::from(q) - Vec2::from(drag.start);
                        r.map_geometry(|p| (Vec2::from(p) + delta).into());
                    }
                } else if drag.handle == 0 {
                    r.origin = q;
                } else {
                    let delta = Vec2::from(q) - Vec2::from(r.origin);
                    if delta.length_sq() > 0.01 {
                        r.angle = delta.angle();
                        if angle_snap {
                            let step = 15.0_f32.to_radians();
                            r.angle = (r.angle / step).round() * step;
                        }
                    }
                }
                self.dirty = true;
            }
        }
        if !down {
            self.ruler_ui.drag = None;
        }
        if response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        true
    }
    pub(super) fn paint_rulers(&self, ui: &egui::Ui, view: CanvasView) {
        if !self.settings.show_rulers {
            return;
        }
        let painter = ui.painter().with_clip_rect(ui.max_rect());
        let viewport_reach = ui.max_rect().size().length() * 2.0;
        for (owner, layer) in self.doc.layers.iter().enumerate() {
            for (index, r) in layer.rulers.iter().enumerate() {
                if !r.visible || !self.doc.ruler_applies(owner, r) {
                    continue;
                }
                let enabled = r.enabled && self.settings.snap_rulers;
                let color = if enabled {
                    Color32::from_rgba_unmultiplied(172, 128, 255, 155)
                } else {
                    Color32::from_rgba_unmultiplied(97, 194, 147, 110)
                };
                let stroke = Stroke::new(1.0_f32, color);
                let line = |p: [f32; 2], d: Vec2| {
                    let origin = view.screen(p);
                    let end = view.screen([p[0] + d.x, p[1] + d.y]);
                    let axis = (end - origin).normalized();
                    let reach = viewport_reach + origin.distance(ui.max_rect().center());
                    if axis != Vec2::ZERO {
                        painter
                            .line_segment([origin - axis * reach, origin + axis * reach], stroke);
                    }
                };
                match r.kind {
                    Kind::Symmetry => {
                        let count = if r.lines.is_multiple_of(2) {
                            r.lines / 2
                        } else {
                            r.lines
                        };
                        for i in 0..count {
                            line(
                                r.origin,
                                Vec2::angled(
                                    r.angle + i as f32 * std::f32::consts::PI / count as f32,
                                ),
                            );
                        }
                    }
                    Kind::Parallel => {
                        let normal = Vec2::angled(r.angle).rot90();
                        for i in -8..=8 {
                            let p = Vec2::from(r.origin) + normal * (i as f32 * 48.0 / view.zoom);
                            line(p.into(), Vec2::angled(r.angle));
                        }
                    }
                    Kind::Radial => {
                        for i in 0..12 {
                            line(
                                r.origin,
                                Vec2::angled(i as f32 * std::f32::consts::PI / 12.0),
                            );
                        }
                    }
                    Kind::Guide => line(r.origin, Vec2::angled(r.angle)),
                    _ => {
                        for i in 0..r.kind.perspective() {
                            let p = r.vanishing[i];
                            // CSP-style construction lines, leaving the drawing readable.
                            for offset in [-0.2, 0.2] {
                                let target = Vec2::from(r.origin)
                                    + Vec2::new(
                                        self.doc.width as f32 * offset,
                                        self.doc.height as f32 * 0.25,
                                    );
                                line(p, target - Vec2::from(p));
                            }
                        }
                        if r.kind.perspective() >= 2 {
                            line(
                                r.vanishing[0],
                                Vec2::from(r.vanishing[1]) - Vec2::from(r.vanishing[0]),
                            );
                        }
                    }
                }
                if self.tool == Tool::Ruler && owner == self.doc.active {
                    for (n, p) in handles(r, view.zoom).into_iter().enumerate() {
                        let p = view.screen(p);
                        painter.circle_filled(p, 6.0, Color32::from_gray(28));
                        painter.circle_stroke(
                            p,
                            6.0,
                            Stroke::new(
                                1.8_f32,
                                if self.ruler_ui.selected == Some(index) {
                                    Color32::WHITE
                                } else {
                                    color
                                },
                            ),
                        );
                        if r.kind.perspective() > 0 {
                            painter.text(
                                p + Vec2::new(9.0, -9.0),
                                egui::Align2::LEFT_BOTTOM,
                                format!("PF{}", n + 1),
                                egui::FontId::proportional(12.0),
                                color,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(app: &mut PaintAtlas, ctx: &egui::Context, time: f64, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(640.0, 480.0),
                )),
                time: Some(time),
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
    fn setup(ctx: &egui::Context, kind: Kind) -> PaintAtlas {
        let mut app = PaintAtlas::from_context(ctx);
        app.reset(Document::new(512, 512, false), None, false);
        app.fit = false;
        app.zoom = 1.0;
        app.pan = Vec2::ZERO;
        app.create_ruler(kind);
        app.history.commit(&app.doc);
        app
    }
    #[test]
    fn dragging_ruler_handle_changes_only_metadata_and_can_be_undone() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Kind::Symmetry);
        let original = app.doc.layers[0].pixels.clone();
        frame(&mut app, &ctx, 0.0, vec![]);
        let p = Pos2::new(320.0, 240.0);
        frame(
            &mut app,
            &ctx,
            0.1,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        frame(
            &mut app,
            &ctx,
            0.2,
            vec![egui::Event::PointerMoved(p + Vec2::new(40.0, 20.0))],
        );
        frame(
            &mut app,
            &ctx,
            0.3,
            vec![button(p + Vec2::new(40.0, 20.0), false)],
        );
        app.history.commit(&app.doc);
        assert_eq!(app.doc.layers[0].rulers.len(), 1);
        assert_eq!(app.doc.layers[0].rulers[0].origin, [296.0, 276.0]);
        assert_eq!(app.doc.layers[0].pixels, original);
        assert!(app.history.undo(&mut app.doc));
        assert_eq!(app.doc.layers[0].rulers[0].origin, [256.0, 256.0]);
    }
    #[test]
    fn canvas_brush_symmetry_is_one_history_action_and_does_not_modify_ruler() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Kind::Symmetry);
        app.tool = Tool::Brush;
        app.pressure_enabled = false;
        app.stabilization = 0.0;
        app.size = 3.0;
        app.opacity = 1.0;
        app.hardness = 1.0;
        app.active_tip = None;
        let ruler = app.doc.layers[0].rulers[0].clone();
        frame(&mut app, &ctx, 0.0, vec![]);
        let p = Pos2::new(200.0, 180.0);
        let q = Pos2::new(220.0, 250.0);
        frame(
            &mut app,
            &ctx,
            0.1,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        frame(&mut app, &ctx, 0.2, vec![egui::Event::PointerMoved(q)]);
        frame(&mut app, &ctx, 0.3, vec![button(q, false)]);
        app.history.commit(&app.doc);
        assert_eq!(app.history.counts(), (2, 0));
        let pixels = &app.doc.layers[0].pixels;
        assert!(pixels.iter().any(|p| *p != 0));
        for y in 0..512 {
            for x in 0..256 {
                assert!(
                    pixels[(y * 512 + x) * 4 + 3].abs_diff(pixels[(y * 512 + 511 - x) * 4 + 3])
                        <= 1
                );
            }
        }
        assert_eq!(app.doc.layers[0].rulers[0], ruler);
        assert!(app.history.undo(&mut app.doc));
        assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
    }
    #[test]
    fn returning_to_perspective_stroke_start_replaces_the_old_direction() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx, Kind::Perspective1);
        app.tool = Tool::Brush;
        app.pressure_enabled = false;
        app.stabilization = 0.0;
        app.size = 3.0;
        app.opacity = 1.0;
        app.hardness = 1.0;
        app.active_tip = None;
        // Vanishing point is diagonal to the start, leaving horizontal/vertical directions distinct.
        frame(&mut app, &ctx, 0.0, vec![]);
        let p = Pos2::new(200.0, 240.0);
        frame(
            &mut app,
            &ctx,
            0.1,
            vec![egui::Event::PointerMoved(p), button(p, true)],
        );
        frame(
            &mut app,
            &ctx,
            0.2,
            vec![egui::Event::PointerMoved(p + Vec2::new(70.0, 0.0))],
        );
        frame(&mut app, &ctx, 0.3, vec![egui::Event::PointerMoved(p)]);
        frame(
            &mut app,
            &ctx,
            0.4,
            vec![egui::Event::PointerMoved(p + Vec2::new(0.0, 70.0))],
        );
        frame(
            &mut app,
            &ctx,
            0.5,
            vec![button(p + Vec2::new(0.0, 70.0), false)],
        );
        let alpha = |x: usize, y: usize| app.doc.layers[0].pixels[(y * 512 + x) * 4 + 3];
        assert_eq!(alpha(170, 256), 0); // Old horizontal direction (screen x=234).
        assert!(alpha(136, 290) > 0); // New vertical direction.
        app.history.commit(&app.doc);
        assert_eq!(app.history.counts(), (2, 0));
    }
}
