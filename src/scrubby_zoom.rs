//! Temporary, cursor-anchored zoom. Does not change the selected drawing tool.
use super::*;

#[derive(Default)]
pub(super) struct State {
    drag: Option<Drag>,
    captured: bool,
}
struct Drag {
    origin: Pos2,
    anchor: Vec2,
    zoom: f32,
    pan: Vec2,
    moved: bool,
    scrubby: bool,
}
impl State {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
impl PaintAtlas {
    pub(super) fn zoom_cursor(&self, ctx: &egui::Context) -> egui::CursorIcon {
        if ctx.input(|i| self.settings.shortcuts.held(i, Shortcut::ZoomOutModifier)) {
            egui::CursorIcon::ZoomOut
        } else {
            egui::CursorIcon::ZoomIn
        }
    }

    pub(super) fn scrubby_zoom_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        bounds: egui::Rect,
    ) -> bool {
        let ctx = ui.ctx();
        let (held, pointer, origin, pressed, down, released, focused, middle) =
            ctx.input(|input| {
                (
                    self.settings.shortcuts.held(input, Shortcut::TemporaryZoom),
                    input.pointer.interact_pos(),
                    input.pointer.press_origin(),
                    input.pointer.primary_pressed(),
                    input.pointer.primary_down(),
                    input.pointer.primary_released(),
                    input.focused,
                    input.pointer.middle_down(),
                )
            });
        let available = !self.preferences.capturing_shortcut()
            && !ctx.wants_keyboard_input()
            && self.pending.is_none()
            && !self.new_dialog
            && self.image_editor.is_none()
            && self.gesture.is_none()
            && !self.retouch.busy()
            && !middle
            && focused;
        let pan_held = ctx.input(|i| self.settings.shortcuts.held(i, Shortcut::Pan));
        let mode = available && (held || self.tool == Tool::Zoom && !pan_held);
        // Keep ownership until release, even if the modifier is released first.
        // This prevents a zoom drag from becoming a brush stroke or a pan.
        let was_captured = self.scrubby_zoom.captured;
        if !available {
            self.scrubby_zoom.clear();
            return was_captured;
        }
        if pressed
            && mode
            && response.hovered()
            && let Some(origin) = origin.or(pointer)
        {
            self.scrubby_zoom.captured = true;
            self.scrubby_zoom.drag = Some(Drag {
                origin,
                anchor: origin - bounds.center(),
                zoom: self.zoom,
                pan: self.pan,
                moved: false,
                scrubby: self.settings.scrubby_zoom,
            });
            self.zoom_animation = None;
            self.fit_animation = None;
            self.rotation_target = None;
        }
        let consumed = was_captured || self.scrubby_zoom.captured || mode && response.hovered();
        if !mode {
            self.scrubby_zoom.drag = None;
        }
        let mut click_anchor = None;
        if let Some(drag) = &mut self.scrubby_zoom.drag {
            if let Some(pointer) = pointer {
                let delta = pointer - drag.origin;
                drag.moved |= delta.length_sq() >= 9.0;
                if drag.moved && drag.scrubby {
                    let zoom =
                        (drag.zoom * (delta.x * 0.008).clamp(-20.0, 20.0).exp()).clamp(0.05, 16.0);
                    self.zoom = zoom;
                    self.pan = (drag.pan - drag.anchor) * (zoom / drag.zoom) + drag.anchor;
                    ctx.request_repaint();
                }
            }
            if released && !drag.moved {
                click_anchor = Some(drag.anchor);
            }
        }
        if released || !down {
            self.scrubby_zoom.clear();
        }
        if let Some(anchor) = click_anchor {
            if self.tool == Tool::Zoom
                && ctx.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                })
            {
                self.zoom_animation = None;
                self.fit_animation = None;
                self.set_rotation_target(0.0);
                self.mirrored = false;
                self.fit = true;
                self.smooth_fit = true;
            } else {
                let out = ctx.input(|i| self.settings.shortcuts.held(i, Shortcut::ZoomOutModifier));
                self.request_zoom(if out { 1.0 / 1.25 } else { 1.25 }, false, anchor);
            }
        }
        if mode && (response.hovered() || consumed) {
            ctx.set_cursor_icon(self.zoom_cursor(ctx));
        }
        consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(ctx: &egui::Context) -> PaintAtlas {
        let mut app = PaintAtlas::from_context(ctx);
        app.reset(Document::new(512, 512, false), None, false);
        app.fit = false;
        app.zoom = 1.0;
        app.pan = Vec2::new(25.0, -10.0);
        app.angle = 0.6;
        app.mirrored = true;
        app
    }
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
    fn key(pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Space,
            physical_key: Some(egui::Key::Space),
            pressed,
            repeat: false,
            modifiers,
        }
    }
    fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        }
    }
    #[test]
    fn zoom_tool_clicks_and_alt_clicks_work_with_scrubby_enabled_or_disabled() {
        for (scrubby, tool) in [
            (false, Tool::Zoom),
            (true, Tool::Zoom),
            (false, Tool::Brush),
            (true, Tool::Brush),
        ] {
            let ctx = egui::Context::default();
            let mut app = setup(&ctx);
            app.tool = tool;
            app.settings.scrubby_zoom = scrubby;
            app.settings.appearance.reduced_motion = true;
            frame(&mut app, &ctx, 0.0, egui::Modifiers::NONE, vec![]);
            let p = Pos2::new(320.0, 240.0);
            for (i, mut modifiers, expected) in [
                (0, egui::Modifiers::NONE, 1.25),
                (1, egui::Modifiers::ALT, 1.0),
            ] {
                if tool == Tool::Brush {
                    modifiers |= egui::Modifiers::CTRL;
                }
                let time = 0.1 + i as f64;
                frame(&mut app, &ctx, time, modifiers, {
                    let mut events = vec![egui::Event::PointerMoved(p), button(p, true, modifiers)];
                    if tool == Tool::Brush && i == 0 {
                        events.insert(0, key(true, modifiers));
                    }
                    events
                });
                frame(
                    &mut app,
                    &ctx,
                    time + 0.1,
                    modifiers,
                    vec![button(p, false, modifiers)],
                );
                frame(&mut app, &ctx, time + 0.2, modifiers, vec![]);
                assert!((app.zoom - expected).abs() < 0.001);
                assert!(app.gesture.is_none());
                assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
            }
            assert!(matches!(app.zoom_cursor(&ctx), egui::CursorIcon::ZoomOut));
        }
    }
    #[test]
    fn double_click_zoom_restores_view_without_changing_pixels() {
        for scrubby in [false, true] {
            let ctx = egui::Context::default();
            let mut app = setup(&ctx);
            app.tool = Tool::Zoom;
            app.settings.scrubby_zoom = scrubby;
            app.settings.appearance.reduced_motion = true;
            let original = app.doc.layers[0].pixels.clone();
            frame(&mut app, &ctx, 0.0, egui::Modifiers::NONE, vec![]);
            let p = Pos2::new(320.0, 240.0);
            for (time, pressed) in [(0.1, true), (0.15, false), (0.2, true), (0.25, false)] {
                frame(
                    &mut app,
                    &ctx,
                    time,
                    egui::Modifiers::NONE,
                    vec![
                        egui::Event::PointerMoved(p),
                        button(p, pressed, egui::Modifiers::NONE),
                    ],
                );
            }
            frame(&mut app, &ctx, 0.3, egui::Modifiers::NONE, vec![]);
            assert!(!app.mirrored);
            assert!(app.angle.sin().abs() < 0.001 && app.angle.cos() > 0.999);
            assert_eq!(app.pan, Vec2::ZERO);
            assert!(app.zoom_animation.is_none());
            assert_eq!(app.doc.layers[0].pixels, original);
            assert_eq!(app.history.counts(), (0, 0));
        }
    }
    #[test]
    fn disabling_scrubby_disables_drag_zoom_for_tool_and_temporary_shortcut() {
        for tool in [Tool::Zoom, Tool::Brush] {
            let ctx = egui::Context::default();
            let mut app = setup(&ctx);
            app.tool = tool;
            app.settings.scrubby_zoom = false;
            frame(&mut app, &ctx, 0.0, egui::Modifiers::NONE, vec![]);
            let p = Pos2::new(320.0, 240.0);
            let modifiers = if tool == Tool::Brush {
                egui::Modifiers::CTRL
            } else {
                egui::Modifiers::NONE
            };
            let mut events = vec![egui::Event::PointerMoved(p), button(p, true, modifiers)];
            if tool == Tool::Brush {
                events.insert(0, key(true, modifiers));
            }
            frame(&mut app, &ctx, 0.1, modifiers, events);
            frame(
                &mut app,
                &ctx,
                0.2,
                modifiers,
                vec![egui::Event::PointerMoved(p + Vec2::new(100.0, 0.0))],
            );
            assert_eq!(app.zoom, 1.0);
            frame(
                &mut app,
                &ctx,
                0.3,
                modifiers,
                vec![button(p + Vec2::new(100.0, 0.0), false, modifiers)],
            );
            assert!(app.zoom_animation.is_none());
            assert!(app.gesture.is_none());
            assert_eq!(app.history.counts(), (0, 0));
        }
    }
    #[test]
    fn zoom_limits_are_reversible_and_custom_binding_replaces_ctrl_space() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        let mut binding = crate::shortcuts::Binding::from_modifiers(egui::Modifiers::CTRL);
        binding.key = Some("J".into());
        assert!(
            app.settings
                .shortcuts
                .assign(Shortcut::TemporaryZoom, 0, binding, false)
        );
        frame(&mut app, &ctx, 0.0, egui::Modifiers::NONE, vec![]);
        let start = Pos2::new(320.0, 240.0);
        frame(
            &mut app,
            &ctx,
            0.1,
            egui::Modifiers::CTRL,
            vec![
                key(true, egui::Modifiers::CTRL),
                egui::Event::PointerMoved(start),
                button(start, true, egui::Modifiers::CTRL),
            ],
        );
        assert!(!app.scrubby_zoom.captured);
        frame(
            &mut app,
            &ctx,
            0.2,
            egui::Modifiers::CTRL,
            vec![
                key(false, egui::Modifiers::CTRL),
                button(start, false, egui::Modifiers::CTRL),
            ],
        );
        let key_j = egui::Event::Key {
            key: egui::Key::J,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        frame(
            &mut app,
            &ctx,
            0.3,
            egui::Modifiers::CTRL,
            vec![key_j, button(start, true, egui::Modifiers::CTRL)],
        );
        assert!(app.scrubby_zoom.captured);
        let initial = app.zoom;
        for (i, offset, expected) in [(0, 10000.0, 16.0), (1, -10000.0, 0.05), (2, 0.0, initial)] {
            frame(
                &mut app,
                &ctx,
                0.4 + i as f64 * 0.1,
                egui::Modifiers::CTRL,
                vec![egui::Event::PointerMoved(start + Vec2::new(offset, 0.0))],
            );
            assert_eq!(app.zoom, expected);
        }
    }
    #[test]
    fn drag_zooms_both_directions_keeps_anchor_and_never_paints_or_pans() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        frame(&mut app, &ctx, 0.0, egui::Modifiers::NONE, vec![]);
        let original = app.doc.layers[0].pixels.clone();
        let tool = app.tool;
        let start = Pos2::new(420.0, 280.0);
        let document = |app: &PaintAtlas| {
            CanvasView {
                center: Pos2::new(320.0, 240.0) + app.pan,
                size: Vec2::splat(512.0),
                zoom: app.zoom,
                angle: app.angle,
                mirrored: app.mirrored,
            }
            .document(start)
        };
        let anchor = document(&app);
        frame(
            &mut app,
            &ctx,
            0.1,
            egui::Modifiers::CTRL,
            vec![
                key(true, egui::Modifiers::CTRL),
                egui::Event::PointerMoved(start),
                button(start, true, egui::Modifiers::CTRL),
            ],
        );
        frame(
            &mut app,
            &ctx,
            0.2,
            egui::Modifiers::CTRL,
            vec![egui::Event::PointerMoved(start + Vec2::new(100.0, 0.0))],
        );
        assert!(app.zoom > 2.0);
        for (a, b) in document(&app).into_iter().zip(anchor) {
            assert!((a - b).abs() < 0.001);
        }
        frame(
            &mut app,
            &ctx,
            0.3,
            egui::Modifiers::CTRL,
            vec![egui::Event::PointerMoved(start - Vec2::new(100.0, 0.0))],
        );
        assert!(app.zoom < 0.5);
        for (a, b) in document(&app).into_iter().zip(anchor) {
            assert!((a - b).abs() < 0.001);
        }
        frame(
            &mut app,
            &ctx,
            0.4,
            egui::Modifiers::CTRL,
            vec![button(start, false, egui::Modifiers::CTRL)],
        );
        assert!(!app.scrubby_zoom.captured);
        assert_eq!(app.doc.layers[0].pixels, original);
        assert!(app.tool == tool);
        assert_eq!(app.history.counts(), (0, 0));
    }
    #[test]
    fn modifier_release_cannot_turn_zoom_into_a_stroke_and_click_zooms_in() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        frame(&mut app, &ctx, 0.0, egui::Modifiers::NONE, vec![]);
        let start = Pos2::new(320.0, 240.0);
        frame(
            &mut app,
            &ctx,
            0.1,
            egui::Modifiers::CTRL,
            vec![
                key(true, egui::Modifiers::CTRL),
                egui::Event::PointerMoved(start),
                button(start, true, egui::Modifiers::CTRL),
            ],
        );
        frame(
            &mut app,
            &ctx,
            0.2,
            egui::Modifiers::CTRL,
            vec![button(start, false, egui::Modifiers::CTRL)],
        );
        assert!(app.zoom_animation.is_some());
        frame(
            &mut app,
            &ctx,
            0.3,
            egui::Modifiers::CTRL,
            vec![button(start, true, egui::Modifiers::CTRL)],
        );
        frame(
            &mut app,
            &ctx,
            0.4,
            egui::Modifiers::NONE,
            vec![
                key(false, egui::Modifiers::NONE),
                egui::Event::PointerMoved(start + Vec2::splat(40.0)),
            ],
        );
        assert!(app.scrubby_zoom.captured);
        assert!(app.gesture.is_none());
        assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
        frame(
            &mut app,
            &ctx,
            0.5,
            egui::Modifiers::NONE,
            vec![button(start, false, egui::Modifiers::NONE)],
        );
        assert!(!app.scrubby_zoom.captured);
    }
}
