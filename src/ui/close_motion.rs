//! Closing feedback retains GPU handles briefly, never the closed document's pixels.
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Dot,
    Trash,
    Slide,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(ctx: &egui::Context) -> PaintAtlas {
        let mut app = PaintAtlas::from_context(ctx);
        app.reset(Document::new(32, 24, true), None, false);
        app.settings.appearance.reduced_motion = false;
        crate::theme::apply(ctx, &app.settings.appearance);
        app
    }
    fn frame(
        app: &mut PaintAtlas,
        ctx: &egui::Context,
        f: impl FnOnce(&mut PaintAtlas, &egui::Context),
    ) {
        let mut f = Some(f);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(1000.0, 700.0),
                )),
                ..Default::default()
            },
            |ctx| {
                app.close_motion.ctx = Some(ctx.clone());
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Editor");
                });
                if let Some(f) = f.take() {
                    f(app, ctx);
                }
                app.close_motion.ctx = None;
            },
        );
    }
    fn complete(app: &mut PaintAtlas, ctx: &egui::Context) {
        frame(app, ctx, |app, ctx| {
            app.close_motion.flight.as_mut().unwrap().elapsed = 2.0;
            app.close_motion_ui(ctx);
            app.finish_close_frame(ctx);
        });
    }

    #[test]
    fn clean_last_canvas_reveals_splash_and_badge_expires() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        let layer_count = app.doc.layers.len();
        frame(&mut app, &ctx, |app, ctx| {
            app.request(Action::CloseTab(app.documents[0].id));
            assert_eq!(app.documents.len(), 1); // Render the outgoing HUD before closing.
            app.finish_close_frame(ctx);
        });
        assert!(app.documents.is_empty() && app.startup_canvas_required);
        assert_eq!(app.close_motion.flight.as_ref().unwrap().kind, Kind::Dot);
        let flight = app.close_motion.flight.as_ref().unwrap();
        assert_eq!(flight.snapshot.layers.len(), layer_count);
        let texture = flight.snapshot._textures[0].id();
        assert!(ctx.tex_manager().read().meta(texture).is_some());
        complete(&mut app, &ctx);
        assert!(ctx.tex_manager().read().meta(texture).is_none());
        assert!(!app.close_motion.active());
        assert_eq!(app.close_motion.badge.as_ref().unwrap().kind, Kind::Dot);
        frame(&mut app, &ctx, |app, ctx| {
            app.close_motion.badge.as_mut().unwrap().elapsed = 3.1;
            app.close_motion_ui(ctx);
        });
        assert!(app.close_motion.badge.is_none());
    }
    #[test]
    fn closing_a_clean_tab_reveals_the_remaining_document_without_mutation() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        let original = app.doc.layers[0].pixels.clone();
        app.add_document(Document::new(16, 12, false), None, false);
        frame(&mut app, &ctx, |app, ctx| {
            app.request(Action::CloseTab(app.documents[1].id));
            app.finish_close_frame(ctx);
        });
        assert_eq!(app.close_motion.flight.as_ref().unwrap().kind, Kind::Slide);
        assert_eq!(app.documents.len(), 1);
        assert_eq!(app.doc.layers[0].pixels, original);
        complete(&mut app, &ctx);
        assert!(app.close_motion.badge.is_none());
        assert!(!app.startup_canvas_required);
    }
    #[test]
    fn dirty_canvas_requires_confirmation_then_discard_uses_trash() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        app.dirty = true;
        let action = Action::CloseTab(app.documents[0].id);
        frame(&mut app, &ctx, |app, _| {
            app.request(action);
        });
        assert!(app.pending.is_some() && !app.close_motion.active());
        assert_eq!(app.documents.len(), 1);
        frame(&mut app, &ctx, |app, ctx| {
            app.pending = None;
            app.perform(action); // Explicit confirmation of discard.
            app.finish_close_frame(ctx);
        });
        assert_eq!(app.close_motion.flight.as_ref().unwrap().kind, Kind::Trash);
        complete(&mut app, &ctx);
        assert_eq!(app.close_motion.badge.as_ref().unwrap().kind, Kind::Trash);
    }
    #[test]
    fn close_all_sequences_each_canvas_and_reduced_motion_closes_immediately() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        app.add_document(Document::new(16, 12, false), None, false);
        frame(&mut app, &ctx, |app, ctx| {
            app.request(Action::CloseAll);
            app.finish_close_frame(ctx);
        });
        assert_eq!(app.documents.len(), 1);
        complete(&mut app, &ctx);
        assert!(app.documents.is_empty() && app.startup_canvas_required);
        complete(&mut app, &ctx);
        assert!(!app.allow_close);

        let mut app = setup(&ctx);
        app.settings.appearance.reduced_motion = true;
        crate::theme::apply(&ctx, &app.settings.appearance);
        frame(&mut app, &ctx, |app, _| {
            app.request(Action::CloseAll);
        });
        assert!(app.documents.is_empty() && !app.close_motion.active());
    }
}

struct Snapshot {
    view: CanvasView,
    bounds: egui::Rect,
    _textures: Vec<egui::TextureHandle>,
    layers: Vec<crate::blend_gpu::Layer>,
    checker: Option<egui::TextureHandle>,
    fallback: Option<egui::TextureHandle>,
    shader: Option<std::sync::Arc<crate::blend_gpu::Shader>>,
    hud: Vec<egui::epaint::ClippedShape>,
    _hud_textures: Vec<egui::TextureHandle>,
}
struct Flight {
    snapshot: Snapshot,
    kind: Kind,
    elapsed: f32,
    followup: Option<bool>,
}
struct Badge {
    kind: Kind,
    elapsed: f32,
}

#[derive(Default)]
pub(super) struct State {
    pub ctx: Option<egui::Context>,
    pub pending: Option<bool>, // true only when closing with unsaved changes discarded
    pub followup: Option<bool>, // close remaining documents; true also exits the app
    flight: Option<Flight>,
    badge: Option<Badge>,
}
impl State {
    pub fn active(&self) -> bool {
        self.pending.is_some() || self.flight.is_some()
    }
    pub fn defer(&mut self, discarded: bool) -> bool {
        if let Some(ctx) = &self.ctx
            && !crate::theme::reduced_motion(ctx)
        {
            self.pending.get_or_insert(discarded);
            ctx.request_repaint();
            return true;
        }
        false
    }
}

fn hud_shape(shape: &egui::Shape) -> Option<egui::Shape> {
    match shape {
        // Canvas/glass GPU callbacks depend on live frame resources; keep only HUD geometry.
        egui::Shape::Callback(_) => None,
        egui::Shape::Vec(shapes) => Some(egui::Shape::Vec(
            shapes.iter().filter_map(hud_shape).collect(),
        )),
        shape => Some(shape.clone()),
    }
}
fn capture_hud(ctx: &egui::Context) -> (Vec<egui::epaint::ClippedShape>, Vec<egui::TextureHandle>) {
    let mut layers: Vec<_> = ctx.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| layer.order <= egui::Order::Middle)
            .collect()
    });
    layers.push(egui::LayerId::background());
    layers.sort_by_key(|layer| (layer.order, layer.id.value()));
    layers.dedup();
    let mut shapes = Vec::new();
    for layer in layers {
        let transform = ctx.layer_transform_to_global(layer);
        let entries = ctx.graphics(|g| {
            g.get(layer)
                .map(|list| list.all_entries().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        });
        for entry in entries {
            if let Some(shape) = hud_shape(&entry.shape) {
                let mut entry = egui::epaint::ClippedShape {
                    clip_rect: entry.clip_rect,
                    shape,
                };
                if let Some(transform) = transform {
                    entry.transform(transform);
                }
                shapes.push(entry);
            }
        }
    }
    // Retain image/thumbnail handles referenced by the captured HUD until its fade completes.
    let manager = ctx.tex_manager();
    let ids: Vec<_> = manager.read().allocated().map(|(id, _)| *id).collect();
    let handles = ids
        .into_iter()
        .map(|id| {
            manager.write().retain(id);
            egui::TextureHandle::new(manager.clone(), id)
        })
        .collect();
    (shapes, handles)
}

fn badge_center(screen: egui::Rect) -> Pos2 {
    screen.right_bottom() - Vec2::splat(42.0)
}
fn paint_badge(ctx: &egui::Context, kind: Kind, elapsed: f32, opacity: f32) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("closed-canvas-badge"),
    ));
    let center = badge_center(ctx.content_rect());
    let pulse = (elapsed * 7.0).sin() * 0.5 + 0.5;
    let accent = crate::theme::accent(ctx);
    painter.circle_stroke(
        center,
        20.0 + pulse * 6.0,
        Stroke::new(
            1.5_f32,
            accent.gamma_multiply(opacity * (0.4 - pulse * 0.2)),
        ),
    );
    painter.circle_filled(
        center,
        18.0 + pulse * 1.2,
        Color32::from_rgb(35, 39, 47).gamma_multiply(opacity),
    );
    let stroke = Stroke::new(1.8_f32, accent.gamma_multiply(opacity));
    if kind == Kind::Trash {
        let body =
            egui::Rect::from_center_size(center + Vec2::new(0.0, 2.0), Vec2::new(14.0, 16.0));
        painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
        painter.line_segment(
            [
                center + Vec2::new(-10.0, -8.0),
                center + Vec2::new(10.0, -8.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                center + Vec2::new(-4.0, -11.0),
                center + Vec2::new(4.0, -11.0),
            ],
            stroke,
        );
        for x in [-3.0, 3.0] {
            painter.line_segment(
                [center + Vec2::new(x, -2.0), center + Vec2::new(x, 8.0)],
                stroke,
            );
        }
    } else {
        painter.circle_filled(center, 6.0 + pulse * 1.5, accent.gamma_multiply(opacity));
    }
}

impl PaintAtlas {
    /// Run after all editor panels have painted, then close the model exactly once.
    pub(super) fn finish_close_frame(&mut self, ctx: &egui::Context) {
        let Some(discarded) = self.close_motion.pending.take() else {
            return;
        };
        if self.documents.is_empty() {
            return;
        }
        // An inactive tab can have been selected for closure after this frame's
        // backdrop upload. Ensure the retained textures belong to that document.
        self.sync_layer_textures(ctx);
        let last = self.documents.len() == 1;
        let kind = if discarded {
            Kind::Trash
        } else if last {
            Kind::Dot
        } else {
            Kind::Slide
        };
        let (hud, hud_textures) = if last {
            capture_hud(ctx)
        } else {
            (Vec::new(), Vec::new())
        };
        let bounds = self.last_canvas_bounds.unwrap_or(ctx.content_rect());
        let layers = self
            .doc
            .layers
            .iter()
            .zip(&self.textures)
            .filter(|(layer, _)| layer.visible && layer.parent.is_none())
            .map(|(layer, texture)| crate::blend_gpu::Layer {
                texture: texture.id(),
                opacity: layer.opacity,
                mode: layer.blend_mode,
                floating: None,
                reveal: None,
            })
            .collect();
        let snapshot = Snapshot {
            view: CanvasView {
                center: bounds.center() + self.pan,
                size: Vec2::new(self.doc.width as f32, self.doc.height as f32),
                zoom: self.zoom,
                angle: self.angle,
                mirrored: self.mirrored,
            },
            bounds,
            _textures: self.textures.clone(),
            layers,
            checker: self.checker.clone(),
            fallback: self.blend_fallback.take(),
            shader: self.blend_shader.clone(),
            hud,
            _hud_textures: hud_textures,
        };
        let followup = self.close_motion.followup.take();
        // Bypass visual deferral while performing the actual close.
        self.close_motion.ctx = None;
        self.close_active_document();
        self.close_motion.ctx = Some(ctx.clone());
        self.close_motion.flight = Some(Flight {
            snapshot,
            kind,
            elapsed: 0.0,
            followup,
        });
        ctx.request_repaint();
    }

    pub(super) fn close_motion_ui(&mut self, ctx: &egui::Context) {
        if let Some(badge) = &mut self.close_motion.badge {
            badge.elapsed += ctx.input(|i| i.stable_dt).min(0.05);
            paint_badge(
                ctx,
                badge.kind,
                badge.elapsed,
                ((3.0 - badge.elapsed) / 0.5).clamp(0.0, 1.0),
            );
            if badge.elapsed >= 3.0 {
                self.close_motion.badge = None;
            }
            ctx.request_repaint();
        }
        let Some(flight) = &mut self.close_motion.flight else {
            return;
        };
        let duration = if flight.kind == Kind::Slide {
            0.44
        } else {
            0.64
        };
        let t = if crate::theme::reduced_motion(ctx) {
            1.0
        } else {
            (flight.elapsed / duration).min(1.0)
        };
        let ease = 1.0 - (1.0 - t).powi(3);
        let screen = ctx.content_rect();
        // Shield the new document/splash only while the closing canvas is moving.
        egui::Area::new(egui::Id::new("closing-canvas-input"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.allocate_rect(screen, Sense::click_and_drag());
            });
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("closing-canvas-flight"),
        ));
        let snap = &flight.snapshot;
        // Reveal the splash under the old HUD without retaining/reopening the old document.
        let hud_opacity = (1.0 - t / 0.65).max(0.0);
        if hud_opacity > 0.0 {
            if !snap.hud.is_empty() {
                painter.rect_filled(
                    screen,
                    0.0,
                    Color32::from_gray(18).gamma_multiply(hud_opacity),
                );
            }
            let b = snap.bounds;
            let outside = [
                egui::Rect::from_min_max(screen.min, Pos2::new(screen.right(), b.top())),
                egui::Rect::from_min_max(Pos2::new(screen.left(), b.bottom()), screen.max),
                egui::Rect::from_min_max(Pos2::new(screen.left(), b.top()), b.left_bottom()),
                egui::Rect::from_min_max(b.right_top(), Pos2::new(screen.right(), b.bottom())),
            ];
            for entry in &snap.hud {
                for region in outside {
                    let clip = entry.clip_rect.intersect(region);
                    if clip.is_positive() {
                        let mut p = painter.with_clip_rect(clip);
                        p.multiply_opacity(hud_opacity);
                        p.add(entry.shape.clone());
                    }
                }
            }
        }
        let mut view = snap.view;
        if flight.kind == Kind::Slide {
            let left = snap
                .view
                .corners()
                .iter()
                .map(|p| p.x)
                .fold(f32::INFINITY, f32::min);
            let distance = (snap.bounds.right() - left).max(screen.width()) + 32.0;
            view.center.x += distance * ease;
        } else {
            let target = badge_center(screen);
            view.center = view.center.lerp(target, ease)
                + Vec2::new(0.0, -40.0 * (std::f32::consts::PI * t).sin());
            view.zoom = egui::lerp(view.zoom..=22.0 / view.size.max_elem(), ease);
            view.angle *= 1.0 - ease;
            paint_badge(ctx, flight.kind, flight.elapsed, (t / 0.2).min(1.0));
        }
        let clip = if flight.kind == Kind::Slide {
            snap.bounds
        } else {
            egui::Rect::from_min_max(
                snap.bounds.min.lerp(screen.min, ease),
                snap.bounds.max.lerp(screen.max, ease),
            )
        };
        let p = painter.with_clip_rect(clip);
        if let Some(checker) = &snap.checker {
            p.add(view.mesh(checker.id(), view.size / 16.0, Color32::WHITE));
        }
        if let Some(fallback) = &snap.fallback {
            p.add(view.mesh(fallback.id(), Vec2::splat(1.0), Color32::WHITE));
        } else if let Some(shader) = &snap.shader {
            p.add(shader.callback(clip, view, snap.layers.clone()));
        } else {
            for layer in &snap.layers {
                p.add(view.mesh(
                    layer.texture,
                    Vec2::splat(1.0),
                    Color32::from_white_alpha((layer.opacity * 255.0) as u8),
                ));
            }
        }
        flight.elapsed += ctx.input(|i| i.stable_dt).min(0.05);
        if t >= 1.0 {
            let flight = self.close_motion.flight.take().unwrap();
            if flight.kind != Kind::Slide {
                self.close_motion.badge = Some(Badge {
                    kind: flight.kind,
                    elapsed: 0.0,
                });
            }
            if let Some(exit) = flight.followup {
                self.continue_closing_documents(exit);
            }
        }
        ctx.request_repaint();
    }
}
