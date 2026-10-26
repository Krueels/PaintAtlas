//! Cursor-local brush adjustment. Owns its pointer gesture until release.
use super::*;

#[derive(Default)]
pub(super) struct State {
    drag: Option<(Pos2, f32, f32)>,
    captured: bool,
    preview: Option<Pos2>,
    texture: Option<(Option<String>, f32, egui::TextureHandle)>,
}
impl State {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
fn adjusted(size: f32, opacity: f32, delta: Vec2) -> (f32, f32) {
    (
        (size * (delta.x * 0.008).clamp(-20.0, 20.0).exp()).clamp(1.0, MAX_BRUSH_SIZE),
        (opacity - delta.y / 200.0).clamp(0.0, 1.0),
    )
}
impl PaintAtlas {
    pub(super) fn brush_hud_input(&mut self, ui: &egui::Ui, response: &egui::Response) -> bool {
        let ctx = ui.ctx();
        let (held, pointer, pressed, down, focused) = ctx.input(|i| {
            (
                self.settings.shortcuts.held(i, Shortcut::BrushHud),
                i.pointer.interact_pos(),
                i.pointer.primary_pressed() || i.pointer.secondary_pressed(),
                i.pointer.primary_down() || i.pointer.secondary_down(),
                i.focused,
            )
        });
        let available = !ctx.input(|i| self.settings.shortcuts.held(i, Shortcut::TemporaryZoom))
            && focused
            && !ctx.wants_keyboard_input()
            && !self.preferences.capturing_shortcut()
            && self.pending.is_none()
            && !self.new_dialog
            && self.image_editor.is_none()
            && self.gesture.is_none()
            && self.selection.floating.is_none()
            && matches!(self.tool, Tool::Brush | Tool::Eraser);
        let was_captured = self.brush_hud.captured;
        if !available {
            self.brush_hud.clear();
            return was_captured;
        }
        let active = held && (response.hovered() || was_captured);
        self.brush_hud.preview = if active {
            self.brush_hud.drag.map(|d| d.0).or(pointer)
        } else {
            None
        };
        if active
            && pressed
            && let Some(p) = pointer
        {
            self.brush_hud.captured = true;
            self.brush_hud.drag = Some((p, self.size, self.opacity));
        }
        if !held {
            self.brush_hud.drag = None;
        }
        if let Some((origin, size, opacity)) = self.brush_hud.drag
            && let Some(p) = pointer
        {
            let (size, opacity) = adjusted(size, opacity, p - origin);
            if self.size != size || self.opacity != opacity {
                self.size = size;
                self.opacity = opacity;
                self.remember_preset();
            }
        }
        if active
            && self
                .brush_hud
                .texture
                .as_ref()
                .is_none_or(|(id, hardness, _)| *id != self.tip_id || *hardness != self.hardness)
        {
            let side = 128;
            let mut pixels = Vec::with_capacity(side * side);
            for y in 0..side {
                for x in 0..side {
                    let u = (x as f32 + 0.5) / side as f32;
                    let v = (y as f32 + 0.5) / side as f32;
                    let alpha = if let Some(tip) = &self.active_tip {
                        tip.levels[0].sample(u, v)
                    } else {
                        let radius = Vec2::new(u - 0.5, v - 0.5).length() * 2.0;
                        ((1.0 - radius) / (1.0 - self.hardness).max(0.001)).clamp(0.0, 1.0)
                    };
                    pixels.push(Color32::from_rgba_unmultiplied(
                        255,
                        70,
                        85,
                        (alpha * 220.0) as u8,
                    ));
                }
            }
            let texture = ctx.load_texture(
                "brush-hud-tip",
                egui::ColorImage::new([side, side], pixels),
                egui::TextureOptions::LINEAR,
            );
            self.brush_hud.texture = Some((self.tip_id.clone(), self.hardness, texture));
        }
        let consumed = active || was_captured || self.brush_hud.captured;
        if !down {
            self.brush_hud.drag = None;
            self.brush_hud.captured = false;
        }
        consumed
    }
    pub(super) fn paint_brush_hud(&self, ui: &egui::Ui, view: CanvasView) {
        let Some(p) = self.brush_hud.preview else {
            return;
        };
        let painter = ui.painter();
        let radius = self.size * view.zoom * 0.5;
        if let Some((_, _, texture)) = &self.brush_hud.texture {
            let mut size = Vec2::splat(radius * 2.0);
            if let Some(tip) = &self.active_tip {
                let mask = &tip.levels[0];
                size = size * Vec2::new(mask.width as f32, mask.height as f32)
                    / mask.width.max(mask.height) as f32;
            }
            painter.image(
                texture.id(),
                egui::Rect::from_center_size(p, size),
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::from_white_alpha((self.opacity * 255.0) as u8),
            );
        }
        let mut brush = self.brush();
        brush.size = self.size; // HUD is independent of pen pressure.
        self.paint_brush_cursor(painter, view, p, &brush);
        let text = format!(
            "{:.0} px   ·   Opacidade {:.0}%\n↔ tamanho   ↕ opacidade",
            self.size,
            self.opacity * 100.0
        );
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::WHITE);
        let size = galley.size() + Vec2::new(20.0, 16.0);
        let bounds = ui.max_rect();
        let pos = p + Vec2::new(24.0, 24.0);
        let pos = Pos2::new(
            pos.x
                .clamp(bounds.left(), (bounds.right() - size.x).max(bounds.left())),
            pos.y
                .clamp(bounds.top(), (bounds.bottom() - size.y).max(bounds.top())),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(pos, size),
            10.0,
            Color32::from_black_alpha(225),
        );
        painter.galley(pos + Vec2::new(10.0, 8.0), galley, Color32::WHITE);
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hud_consumes_pen_drag_and_modifier_release_without_painting() {
        for tool in [Tool::Brush, Tool::Eraser] {
            let ctx = egui::Context::default();
            let mut app = PaintAtlas::from_context(&ctx);
            app.reset(Document::new(512, 512, false), None, false);
            app.tool = tool;
            app.size = 20.0;
            app.opacity = 0.5;
            let original = app.doc.layers[0].pixels.clone();
            let mods = egui::Modifiers::CTRL | egui::Modifiers::ALT;
            let p = Pos2::new(320.0, 240.0);
            let mut frame = |time, modifiers, events| {
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
            };
            let button = |pressed, modifiers| egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            };
            frame(0.0, egui::Modifiers::NONE, vec![]);
            frame(
                0.1,
                mods,
                vec![egui::Event::PointerMoved(p), button(true, mods)],
            );
            frame(
                0.2,
                mods,
                vec![egui::Event::PointerMoved(p + Vec2::new(100.0, -50.0))],
            );
            frame(
                0.3,
                egui::Modifiers::NONE,
                vec![egui::Event::PointerMoved(p + Vec2::splat(120.0))],
            );
            frame(
                0.4,
                egui::Modifiers::NONE,
                vec![button(false, egui::Modifiers::NONE)],
            );
            assert!(app.size > 20.0);
            assert_eq!(app.opacity, 0.75);
            assert!(app.gesture.is_none());
            assert!(!app.picker.active);
            assert_eq!(app.doc.layers[0].pixels, original);
            assert_eq!(app.history.counts(), (0, 0));
        }
    }
    #[test]
    fn brush_adjustment_axes_limits_and_reversibility() {
        let (size, opacity) = adjusted(20.0, 0.5, Vec2::new(100.0, -50.0));
        assert!(size > 20.0);
        assert_eq!(opacity, 0.75);
        assert_eq!(adjusted(20.0, 0.5, Vec2::ZERO), (20.0, 0.5));
        assert_eq!(adjusted(20.0, 0.5, Vec2::splat(-10000.0)), (1.0, 1.0));
        assert_eq!(
            adjusted(20.0, 0.5, Vec2::splat(10000.0)),
            (MAX_BRUSH_SIZE, 0.0)
        );
    }
}
