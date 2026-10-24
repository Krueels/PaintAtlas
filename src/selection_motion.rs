//! Shared, visual-only spring for mutually exclusive button groups.
use eframe::egui::{self, Pos2, Rect, Stroke, Vec2};

#[derive(Clone)]
struct Motion {
    center: Pos2,
    size: Vec2,
    velocity: Vec2,
    size_velocity: Vec2,
    time: f64,
}
impl Motion {
    fn new(rect: Rect, time: f64) -> Self {
        Self {
            center: rect.center(),
            size: rect.size(),
            velocity: Vec2::ZERO,
            size_velocity: Vec2::ZERO,
            time,
        }
    }
    fn advance(&mut self, target: Rect, dt: f32) -> bool {
        let dt = dt.clamp(0.0, 0.05);
        let steps = (dt / 0.004).ceil().max(1.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            self.velocity += ((target.center() - self.center) * 420.0 - self.velocity * 27.0) * h;
            self.center += self.velocity * h;
            self.size_velocity +=
                ((target.size() - self.size) * 420.0 - self.size_velocity * 27.0) * h;
            self.size += self.size_velocity * h;
        }
        let moving = self.center.distance(target.center()) > 0.08
            || self.velocity.length() > 0.4
            || (self.size - target.size()).length() > 0.08
            || self.size_velocity.length() > 0.4;
        if !moving {
            *self = Self::new(target, self.time);
        }
        moving
    }
    fn rect(&self) -> Rect {
        // Stretch along the travel direction and gently compress across it.
        let stretch = (self.velocity.abs() * 0.003).min(Vec2::splat(5.0));
        let deform = Vec2::new(stretch.x - stretch.y * 0.25, stretch.y - stretch.x * 0.25);
        Rect::from_center_size(self.center, (self.size + deform).max(Vec2::splat(4.0)))
    }
}

pub fn paint(ui: &egui::Ui, id: egui::Id, target: Option<Rect>) {
    let ctx = ui.ctx();
    let Some(target) = target else {
        ctx.data_mut(|d| d.remove::<Motion>(id));
        return;
    };
    // Keep the animation attached to a moving/docked panel.
    let origin = ui.clip_rect().min.to_vec2();
    let target = target.translate(-origin);
    let now = ctx.input(|i| i.time);
    let mut motion = ctx
        .data_mut(|d| d.get_temp::<Motion>(id))
        .unwrap_or_else(|| Motion::new(target, now));
    if crate::theme::reduced_motion(ctx) || now - motion.time > 0.25 {
        motion = Motion::new(target, now);
    }
    let moving = motion.advance(target, (now - motion.time).max(0.0) as f32);
    motion.time = now;
    let rect = motion.rect().translate(origin);
    ctx.data_mut(|d| d.insert_temp(id, motion));
    let accent = crate::theme::accent(ctx);
    ui.painter().rect(
        rect,
        9.0,
        accent.gamma_multiply(0.11),
        Stroke::new(1.5_f32, accent),
        egui::StrokeKind::Inside,
    );
    if moving {
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spring_glides_overshoots_and_settles_at_different_frame_rates() {
        for fps in [30, 60, 144] {
            let a = Rect::from_min_size(Pos2::ZERO, Vec2::new(36.0, 29.0));
            let b = a.translate(Vec2::new(0.0, 200.0));
            let mut m = Motion::new(a, 0.0);
            m.advance(b, 1.0 / fps as f32);
            assert!(m.center.y > a.center().y && m.center.y < b.center().y);
            let mut overshot = false;
            for _ in 0..fps * 2 {
                m.advance(b, 1.0 / fps as f32);
                overshot |= m.center.y > b.center().y;
                assert!(m.rect().is_finite());
            }
            assert!(overshot);
            assert_eq!(m.center, b.center());
            assert_eq!(m.velocity, Vec2::ZERO);
        }
    }
}
