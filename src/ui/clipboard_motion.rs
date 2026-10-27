//! Lightweight visual clipboard feedback; never moves or edits document pixels.
use super::*;

const DURATION: f32 = 0.62;
#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Copy,
    Paste,
}
struct Flight {
    texture: egui::TextureHandle,
    source: [Pos2; 4],
    target: [Pos2; 4],
    direction: Direction,
    elapsed: f32,
}
#[derive(Default)]
pub(super) struct State {
    thumbnail: Option<egui::TextureHandle>,
    badge: Option<egui::Rect>,
    flight: Option<Flight>,
    shake: Option<f32>,
}
fn quad(rect: egui::Rect) -> [Pos2; 4] {
    [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
}
fn center(points: [Pos2; 4]) -> Pos2 {
    ((points[0].to_vec2() + points[1].to_vec2() + points[2].to_vec2() + points[3].to_vec2()) * 0.25)
        .to_pos2()
}

pub(super) fn thumbnail(
    ctx: &egui::Context,
    image: &crate::clipboard::Image,
) -> egui::TextureHandle {
    // Bound CPU/GPU preview storage even for 4096px clipboard images. Nearest
    // sampling keeps the exact mask/alpha and avoids cloning the full image.
    let factor = (384.0 / image.size[0].max(image.size[1]) as f32).min(1.0);
    let width = (image.size[0] as f32 * factor).round().max(1.0) as usize;
    let height = (image.size[1] as f32 * factor).round().max(1.0) as usize;
    let mut colors = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let sx = x * image.size[0] / width;
            let sy = y * image.size[1] / height;
            let i = (sy * image.size[0] + sx) * 4;
            let p = &image.pixels[i..i + 4];
            colors.push(Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]));
        }
    }
    ctx.load_texture(
        "clipboard-thumbnail",
        egui::ColorImage::new([width, height], colors),
        egui::TextureOptions::LINEAR,
    )
}
fn fit(rect: egui::Rect, size: Vec2) -> egui::Rect {
    let factor = (rect.width() / size.x).min(rect.height() / size.y);
    egui::Rect::from_center_size(rect.center(), size * factor)
}
fn trajectory(source: [Pos2; 4], target: [Pos2; 4], progress: f32) -> [Pos2; 4] {
    let t = progress.clamp(0.0, 1.0);
    if t == 0.0 {
        return source;
    }
    if t == 1.0 {
        return target;
    }
    let t = t * t * (3.0 - 2.0 * t);
    let a = center(source);
    let b = center(target);
    let arc = Vec2::new(
        0.0,
        -(a.distance(b) * 0.12).min(90.0) * (std::f32::consts::PI * t).sin(),
    );
    let c = a + (b - a) * t + arc;
    std::array::from_fn(|i| c + (source[i] - a) * (1.0 - t) + (target[i] - b) * t)
}
impl State {
    pub fn visible(&self) -> bool {
        self.thumbnail.is_some()
    }
    pub fn hides_paste(&self) -> bool {
        self.flight
            .as_ref()
            .is_some_and(|f| f.direction == Direction::Paste)
    }
    pub fn cancel_flight(&mut self) {
        self.flight = None;
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    fn icon_quad(&self, ctx: &egui::Context) -> [Pos2; 4] {
        let rect = self.badge.unwrap_or_else(|| {
            egui::Rect::from_min_size(
                Pos2::new(
                    ctx.content_rect().right() - 45.0,
                    ctx.content_rect().top() + 5.0,
                ),
                Vec2::splat(32.0),
            )
        });
        let size = self
            .thumbnail
            .as_ref()
            .map_or(Vec2::splat(1.0), |t| t.size_vec2());
        quad(fit(rect.shrink(5.0), size))
    }
    pub fn copied(
        &mut self,
        ctx: &egui::Context,
        image: &crate::clipboard::Image,
        source: Option<[Pos2; 4]>,
    ) {
        self.thumbnail = Some(thumbnail(ctx, image));
        self.flight = None;
        self.shake = None;
        if !crate::theme::reduced_motion(ctx)
            && let Some(source) = source
        {
            self.flight = Some(Flight {
                texture: self.thumbnail.as_ref().unwrap().clone(),
                source,
                target: self.icon_quad(ctx),
                direction: Direction::Copy,
                elapsed: 0.0,
            });
        }
        ctx.request_repaint();
    }
    pub fn pasted(
        &mut self,
        ctx: &egui::Context,
        texture: egui::TextureHandle,
        target: Option<[Pos2; 4]>,
    ) {
        // Only a copy made inside the app creates the indicator. Reading an
        // external image on an explicit paste refreshes an existing indicator.
        if !self.visible() {
            return;
        }
        self.thumbnail = Some(texture);
        self.flight = None;
        if !crate::theme::reduced_motion(ctx) {
            self.shake = Some(0.0);
            if let Some(target) = target {
                self.flight = Some(Flight {
                    texture: self.thumbnail.as_ref().unwrap().clone(),
                    source: self.icon_quad(ctx),
                    target,
                    direction: Direction::Paste,
                    elapsed: 0.0,
                });
            }
        }
        ctx.request_repaint();
    }
    pub fn badge_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(texture) = &self.thumbnail else {
            return false;
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(36.0, 30.0), Sense::click());
        self.badge = Some(rect);
        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 1.0 / 30.0);
        let angle = if crate::theme::reduced_motion(ui.ctx()) {
            self.shake = None;
            0.0
        } else if let Some(time) = &mut self.shake {
            *time += dt;
            let angle = (*time * 38.0).sin() * (-*time * 7.0).exp() * 0.27;
            if *time >= 0.7 {
                self.shake = None;
            } else {
                ui.ctx().request_repaint();
            }
            angle
        } else {
            0.0
        };
        let accent = crate::theme::accent(ui.ctx());
        ui.painter().rect_filled(
            rect,
            8.0,
            accent.gamma_multiply(if response.hovered() { 0.26 } else { 0.13 }),
        );
        let rotation = egui::emath::Rot2::from_angle(angle);
        let rotate = |p: Pos2| rect.center() + rotation * (p - rect.center());
        let frame = egui::Rect::from_center_size(rect.center(), Vec2::new(22.0, 24.0));
        let mut outline: Vec<Pos2> = quad(frame).into_iter().map(rotate).collect();
        outline.push(outline[0]);
        ui.painter()
            .add(egui::Shape::line(outline, Stroke::new(1.4_f32, accent)));
        let preview = fit(frame.shrink2(Vec2::new(3.0, 5.0)), texture.size_vec2());
        let mut mesh = egui::Mesh::with_texture(texture.id());
        for (pos, uv) in quad(preview).into_iter().zip(quad(egui::Rect::from_min_max(
            Pos2::ZERO,
            Pos2::new(1.0, 1.0),
        ))) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: rotate(pos),
                uv,
                color: Color32::WHITE,
            });
        }
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        ui.painter().add(mesh);
        ui.painter().line_segment(
            [
                rotate(rect.center() + Vec2::new(-5.0, -10.0)),
                rotate(rect.center() + Vec2::new(5.0, -10.0)),
            ],
            Stroke::new(3.0_f32, accent),
        );
        response
            .on_hover_text("Imagem copiada para a área de transferência. Clique para colar.")
            .clicked()
    }
    pub fn paint(&mut self, ctx: &egui::Context, paste_target: Option<[Pos2; 4]>) {
        let reduced = crate::theme::reduced_motion(ctx);
        let icon = self.icon_quad(ctx);
        let Some(flight) = &mut self.flight else {
            return;
        };
        let progress = if reduced {
            1.0
        } else {
            (flight.elapsed / DURATION).min(1.0)
        };
        if flight.direction == Direction::Copy {
            flight.target = icon;
        } else if let Some(target) = paste_target {
            flight.target = target;
        }
        let points = trajectory(flight.source, flight.target, progress);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("clipboard-flight"),
        ));
        let mut mesh = egui::Mesh::with_texture(flight.texture.id());
        let opacity = if flight.direction == Direction::Copy {
            1.0 - 0.15 * progress
        } else {
            1.0
        };
        for (pos, uv) in points.into_iter().zip(quad(egui::Rect::from_min_max(
            Pos2::ZERO,
            Pos2::new(1.0, 1.0),
        ))) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv,
                color: Color32::WHITE.gamma_multiply(opacity),
            });
        }
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        painter.add(mesh);
        if progress >= 1.0 {
            if flight.direction == Direction::Copy && !reduced {
                self.shake = Some(0.0);
            }
            self.flight = None;
        } else {
            flight.elapsed += ctx.input(|i| i.stable_dt).clamp(0.0, 1.0 / 30.0);
        }
        ctx.request_repaint();
    }
}
impl PaintAtlas {
    pub(super) fn clipboard_quad(&self, rect: egui::Rect) -> Option<[Pos2; 4]> {
        self.last_canvas_bounds.map(|bounds| {
            let view = CanvasView {
                center: bounds.center() + self.pan,
                size: Vec2::new(self.doc.width as f32, self.doc.height as f32),
                zoom: self.zoom,
                angle: self.angle,
                mirrored: self.mirrored,
            };
            quad(rect).map(|p| view.screen([p.x, p.y]))
        })
    }
    pub(super) fn clipboard_feedback_ui(&mut self, ctx: &egui::Context) {
        if self.startup_canvas_required && self.clipboard_motion.visible() {
            let mut paste = false;
            egui::Area::new(egui::Id::new("startup-clipboard-badge"))
                .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-12.0, 12.0))
                .show(ctx, |ui| {
                    paste = self.clipboard_motion.badge_ui(ui);
                });
            if paste {
                self.paste_from_clipboard(ctx);
            }
        }
        let target = self
            .selection
            .floating
            .as_ref()
            .filter(|f| f.new_layer)
            .and_then(|f| self.clipboard_quad(f.bounds));
        self.clipboard_motion.paint(ctx, target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(ctx: &egui::Context) -> PaintAtlas {
        let mut app = PaintAtlas::from_context(ctx);
        app.reset(Document::new(32, 32, false), None, false);
        for p in app.doc.layers[0].pixels.as_chunks_mut::<4>().0 {
            *p = [120, 60, 210, 128];
        }
        app.selection.mask = Some(std::sync::Arc::new(
            crate::selection::Mask::rectangle(
                32,
                32,
                egui::Rect::from_min_size(Pos2::new(4.0, 6.0), Vec2::new(8.0, 10.0)),
            )
            .unwrap(),
        ));
        app.doc.selection = app.selection.mask.clone();
        app.last_canvas_bounds = Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(640.0, 480.0),
        ));
        app.fit = false;
        app.zoom = 3.0;
        app.angle = 0.4;
        app.mirrored = true;
        app
    }
    #[test]
    fn copy_and_paste_fly_without_changing_pixels_or_adding_history() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        let original = app.doc.layers[0].pixels.clone();
        assert!(!app.clipboard_motion.visible());
        app.copy_selection_to_clipboard(&ctx, false);
        assert!(app.clipboard_motion.visible());
        let flight = app.clipboard_motion.flight.as_ref().unwrap();
        assert!(flight.direction == Direction::Copy);
        assert_eq!(
            flight.source,
            app.clipboard_quad(app.selection.mask.as_ref().unwrap().bounds)
                .unwrap()
        );
        assert_eq!(app.doc.layers[0].pixels, original);
        assert_eq!(app.history.counts(), (0, 0));
        // Copying can be followed by paste immediately, even while the copy is flying.
        app.paste_from_clipboard(&ctx);
        assert!(app.clipboard_motion.hides_paste());
        assert_eq!(app.selection.floating.as_ref().unwrap().size, [8, 10]);
        assert_eq!(app.fallback_blend_composite().unwrap(), app.doc.composite());
        for i in 0..90 {
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(i as f64 / 60.0),
                    ..Default::default()
                },
                |ctx| app.clipboard_feedback_ui(ctx),
            );
        }
        assert!(!app.clipboard_motion.hides_paste());
        assert!(app.clipboard_motion.visible());
        assert_eq!(app.doc.layers[0].pixels, original);
        assert_eq!(app.history.counts(), (0, 0));
        app.apply_transform();
        assert_eq!(app.doc.layers.len(), 2);
        assert_eq!(app.doc.layers[0].pixels, original);
    }
    #[test]
    fn cancel_and_reduced_motion_leave_normal_paste_available() {
        let ctx = egui::Context::default();
        let mut app = setup(&ctx);
        app.copy_selection_to_clipboard(&ctx, false);
        app.paste_from_clipboard(&ctx);
        app.cancel_transform();
        assert!(!app.clipboard_motion.hides_paste());
        assert!(app.clipboard_motion.visible());
        assert_eq!(app.doc.layers.len(), 1);
        crate::theme::apply(
            &ctx,
            &crate::theme::Appearance {
                reduced_motion: true,
                ..Default::default()
            },
        );
        app.copy_selection_to_clipboard(&ctx, false);
        assert!(app.clipboard_motion.flight.is_none());
        app.paste_from_clipboard(&ctx);
        assert!(app.selection.floating.is_some());
        assert!(!app.clipboard_motion.hides_paste());
        app.cancel_transform();
        app.clipboard = Default::default();
        app.paste_from_clipboard(&ctx);
        assert!(!app.clipboard_motion.visible());
    }
    #[test]
    fn curved_flight_has_exact_endpoints_and_a_smooth_middle() {
        let a = quad(egui::Rect::from_min_size(
            Pos2::new(100.0, 300.0),
            Vec2::new(200.0, 100.0),
        ));
        let b = quad(egui::Rect::from_min_size(
            Pos2::new(900.0, 10.0),
            Vec2::splat(24.0),
        ));
        assert_eq!(trajectory(a, b, 0.0), a);
        assert_eq!(trajectory(a, b, 1.0), b);
        let middle = trajectory(a, b, 0.5);
        assert!(center(middle).y < (center(a).y + center(b).y) / 2.0);
        assert!(middle.iter().all(|p| p.is_finite()));
    }
}
