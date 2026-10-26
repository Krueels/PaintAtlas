//! Visual-only drag feedback. Document changes still happen once, on drop.
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, TextureHandle, Vec2};

pub struct Card {
    pub texture: Option<TextureHandle>,
    pub checker: Option<TextureHandle>,
    pub name: String,
    pub info: String,
    pub aspect: f32,
}

pub struct LayerMotion {
    pub index: usize,
    pub released: bool,
    card: Card,
    center: Pos2,
    velocity: Vec2,
    size: Vec2,
    grab_offset: Vec2,
    age: f32,
    release_age: f32,
    impact_age: Option<f32>,
}

fn spring(center: &mut Pos2, velocity: &mut Vec2, target: Pos2, dt: f32, released: bool) {
    // Small integration steps keep the spring stable on slower displays.
    let dt = dt.clamp(0.0, 0.05);
    let steps = (dt / 0.004).ceil().max(1.0) as usize;
    let h = dt / steps as f32;
    let (stiffness, damping) = if released {
        (480.0, 31.0)
    } else {
        (390.0, 30.0)
    };
    for _ in 0..steps {
        *velocity += ((target - *center) * stiffness - *velocity * damping) * h;
        *center += *velocity * h;
    }
}

impl LayerMotion {
    pub fn new(index: usize, rect: Rect, pointer: Pos2, card: Card) -> Self {
        Self {
            index,
            released: false,
            card,
            center: rect.center(),
            velocity: Vec2::ZERO,
            size: rect.size(),
            grab_offset: rect.center() - pointer,
            age: 0.0,
            release_age: 0.0,
            impact_age: None,
        }
    }
    pub fn release(&mut self, index: usize) {
        if !self.released {
            self.index = index;
            self.released = true;
        }
    }
    pub fn row_opacity(&self) -> f32 {
        if self.released {
            0.25 + 0.75 * ((self.release_age - 0.56) / 0.22).clamp(0.0, 1.0)
        } else {
            0.25
        }
    }
    /// Returns false after landing and the final ripple have finished.
    pub fn paint(&mut self, ui: &egui::Ui, target: Rect) -> bool {
        let ctx = ui.ctx();
        let reduced = crate::theme::reduced_motion(ctx);
        if reduced && self.released {
            return false;
        }
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.05);
        self.age += dt;
        if self.released {
            self.release_age += dt;
        }
        let destination = if self.released {
            target.center()
        } else {
            ctx.pointer_interact_pos()
                .map(|p| p + self.grab_offset + Vec2::new(0.0, -6.0))
                .unwrap_or(self.center)
        };
        if reduced {
            self.center = destination;
            self.velocity = Vec2::ZERO;
        } else {
            spring(
                &mut self.center,
                &mut self.velocity,
                destination,
                dt,
                self.released,
            );
        }
        if self.released && self.impact_age.is_none() && self.center.distance(destination) < 8.0 {
            self.impact_age = Some(self.release_age);
        }
        let impact = self.impact_age.map(|t| self.release_age - t);
        let pulse = if self.released || reduced {
            0.0
        } else {
            (self.age * 5.2).sin() * 0.012
        };
        let lift = if reduced {
            0.0
        } else if self.released {
            (-self.release_age * 12.0).exp()
        } else {
            1.0 - (-self.age * 16.0).exp()
        };
        let stretch = (self.velocity.y / 2200.0).tanh() * 0.09;
        let horizontal = (self.velocity.x / 2200.0).tanh().abs() * 0.07;
        let squash = impact
            .map(|t| (t * 26.0).sin() * (-t * 11.0).exp() * 0.065)
            .unwrap_or(0.0);
        let scale = Vec2::new(
            1.0 + lift * 0.025 + pulse - stretch.abs() * 0.35 + horizontal + squash,
            1.0 + lift * 0.04 - pulse + stretch.abs() - horizontal * 0.35 - squash,
        );
        let rect = Rect::from_center_size(self.center, self.size * scale);
        let layer = egui::LayerId::new(ui.layer_id().order, ui.id().with("liquid_layer_overlay"));
        ctx.move_to_top(layer);
        let mut painter = ctx.layer_painter(layer).with_clip_rect(ctx.content_rect());
        let opacity = if self.released {
            ((0.78 - self.release_age) / 0.14).clamp(0.0, 1.0)
        } else {
            1.0
        };
        painter.multiply_opacity(opacity);
        if let Some(t) = impact
            && t < 0.45
        {
            let alpha = ((1.0 - t / 0.45) * 100.0) as u8;
            let accent = crate::theme::accent(ctx);
            painter.rect_stroke(
                target.expand(t * 24.0),
                12.0 + t * 20.0,
                Stroke::new(
                    1.5_f32,
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha),
                ),
                egui::StrokeKind::Outside,
            );
        }
        for (spread, alpha) in [(9.0, 10), (5.0, 20), (2.0, 36)] {
            painter.rect_filled(
                rect.translate(Vec2::new(0.0, 4.0 + lift * 4.0))
                    .expand(spread * lift),
                14.0,
                Color32::from_black_alpha(alpha),
            );
        }
        painter.rect_filled(
            rect,
            10.0 + lift * 7.0 + pulse * 80.0,
            ctx.style().visuals.window_fill,
        );
        crate::theme::backdrop(ctx, &painter, rect, 10.0 + lift * 7.0);
        painter.rect_stroke(
            rect,
            10.0 + lift * 7.0,
            Stroke::new(1.2_f32, Color32::from_white_alpha(100)),
            egui::StrokeKind::Inside,
        );
        let preview = Rect::from_min_size(
            rect.min + Vec2::new(6.0, 6.0),
            Vec2::new(48.0, rect.height() - 12.0),
        );
        painter.rect_filled(preview, 4.0, Color32::from_gray(24));
        let image_size = if self.card.aspect >= preview.aspect_ratio() {
            Vec2::new(preview.width(), preview.width() / self.card.aspect)
        } else {
            Vec2::new(preview.height() * self.card.aspect, preview.height())
        };
        let image = Rect::from_center_size(preview.center(), image_size);
        if let Some(checker) = &self.card.checker {
            painter.image(
                checker.id(),
                image,
                Rect::from_min_size(Pos2::ZERO, image_size / 8.0),
                Color32::WHITE,
            );
        }
        if let Some(texture) = &self.card.texture {
            painter.image(
                texture.id(),
                image,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let text_rect = Rect::from_min_max(
            rect.min + Vec2::new(63.0, 0.0),
            rect.max - Vec2::new(7.0, 0.0),
        );
        let text = painter.with_clip_rect(text_rect.intersect(ctx.content_rect()));
        text.text(
            text_rect.min + Vec2::new(0.0, 10.0),
            egui::Align2::LEFT_TOP,
            &self.card.name,
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
        text.text(
            text_rect.min + Vec2::new(0.0, 29.0),
            egui::Align2::LEFT_TOP,
            &self.card.info,
            egui::FontId::proportional(10.0),
            Color32::from_gray(190),
        );
        let active = !self.released || self.release_age < 0.8;
        if active && !reduced {
            ctx.request_repaint();
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reduced_motion_follows_pointer_and_finishes_drop_immediately() {
        let ctx = egui::Context::default();
        crate::theme::apply(
            &ctx,
            &crate::theme::Appearance {
                reduced_motion: true,
                ..Default::default()
            },
        );
        let origin = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 52.0));
        let mut motion = LayerMotion::new(
            0,
            origin,
            origin.center(),
            Card {
                texture: None,
                checker: None,
                name: "Layer".into(),
                info: "100%".into(),
                aspect: 1.0,
            },
        );
        let pointer = Pos2::new(240.0, 180.0);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(motion.paint(ui, origin));
                });
            },
        );
        assert_eq!(motion.center, pointer + Vec2::new(0.0, -6.0));
        assert_eq!(motion.velocity, Vec2::ZERO);
        motion.release(1);
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!motion.paint(ui, origin));
            });
        });
    }
    #[test]
    fn drop_finishes_and_restores_the_row_opacity() {
        let ctx = egui::Context::default();
        let origin = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(300.0, 52.0));
        let target = origin.translate(Vec2::new(0.0, 180.0));
        let mut motion = LayerMotion::new(
            0,
            origin,
            origin.center(),
            Card {
                texture: None,
                checker: None,
                name: "Layer".into(),
                info: "100%".into(),
                aspect: 1.0,
            },
        );
        assert_eq!(motion.row_opacity(), 0.25);
        motion.release(2);
        let mut finished = false;
        for frame in 0..90 {
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(frame as f64 / 60.0),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if !motion.paint(ui, target) {
                            finished = true;
                        }
                    });
                },
            );
            if finished {
                break;
            }
        }
        assert!(finished);
        assert_eq!(motion.index, 2);
        assert_eq!(motion.row_opacity(), 1.0);
        assert!(motion.center.distance(target.center()) < 1.0);
    }
    #[test]
    fn springs_follow_and_land_at_different_refresh_rates() {
        for hz in [30, 60, 144] {
            for released in [false, true] {
                let mut p = Pos2::ZERO;
                let mut velocity = Vec2::ZERO;
                let target = Pos2::new(250.0, 400.0);
                for _ in 0..hz {
                    spring(&mut p, &mut velocity, target, 1.0 / hz as f32, released);
                }
                assert!(p.distance(target) < 0.1);
                assert!(velocity.length() < 1.0);
            }
        }
    }
}
