use super::*;

const SIDE: usize = 19;
const RADIUS: f32 = 55.0;

#[derive(Default)]
pub(super) struct State {
    pub active: bool,
    pixels: Vec<u8>,
    size: [u32; 2],
    position: Option<Pos2>,
    pixel: Option<[i32; 2]>,
    texture: Option<egui::TextureHandle>,
    color: Color32,
    scale: f32,
    velocity: f32,
}

impl State {
    pub fn begin(&mut self, doc: &Document) {
        self.active = true;
        self.pixels = doc.composite();
        self.size = [doc.width, doc.height];
        self.pixel = None;
    }

    fn sample(&mut self, ctx: &egui::Context, point: [f32; 2]) -> Option<Color32> {
        if point[0] < 0.0
            || point[1] < 0.0
            || point[0] >= self.size[0] as f32
            || point[1] >= self.size[1] as f32
        {
            return None;
        }
        let pixel = [point[0] as i32, point[1] as i32];
        if self.pixel != Some(pixel) {
            self.pixel = Some(pixel);
            let mut image = egui::ColorImage::filled([SIDE, SIDE], Color32::TRANSPARENT);
            for y in 0..SIDE {
                for x in 0..SIDE {
                    let px = pixel[0] + x as i32 - SIDE as i32 / 2;
                    let py = pixel[1] + y as i32 - SIDE as i32 / 2;
                    // Transparency and the area outside the canvas retain a checkerboard.
                    let checker = if (px.div_euclid(2) + py.div_euclid(2)).rem_euclid(2) == 0 {
                        190
                    } else {
                        230
                    };
                    let background = Color32::from_gray(checker);
                    let color = if px >= 0
                        && py >= 0
                        && px < self.size[0] as i32
                        && py < self.size[1] as i32
                    {
                        let i = (py as usize * self.size[0] as usize + px as usize) * 4;
                        let rgba = &self.pixels[i..i + 4];
                        let c = Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
                        if x == SIDE / 2 && y == SIDE / 2 {
                            self.color = Color32::from_rgb(rgba[0], rgba[1], rgba[2]);
                        }
                        background.blend(c)
                    } else {
                        background
                    };
                    image[(x, y)] = color;
                }
            }
            if let Some(texture) = &mut self.texture {
                texture.set(image, egui::TextureOptions::NEAREST);
            } else {
                self.texture = Some(ctx.load_texture(
                    "eyedropper-magnifier",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
        }
        Some(self.color)
    }

    fn animate(&mut self, dt: f32, reduced: bool) {
        let target = if self.active { 1.0 } else { 0.0 };
        if reduced {
            self.scale = target;
            self.velocity = 0.0;
            return;
        }
        // Substeps keep the damped spring stable even when a frame takes longer.
        let dt = dt.clamp(0.0, 0.05) / 8.0;
        for _ in 0..8 {
            self.velocity += ((target - self.scale) * 420.0 - self.velocity * 27.0) * dt;
            self.scale += self.velocity * dt;
        }
        if (self.scale - target).abs() < 0.001 && self.velocity.abs() < 0.01 {
            self.scale = target;
            self.velocity = 0.0;
        }
    }

    pub fn update(
        &mut self,
        ctx: &egui::Context,
        pointer: Option<Pos2>,
        down: bool,
        view: CanvasView,
    ) -> Option<Color32> {
        let mut color = None;
        if self.active {
            if let Some(pointer) = pointer {
                self.position = Some(pointer);
                color = self.sample(ctx, view.document(pointer));
            }
            if !down || !ctx.input(|i| i.focused) {
                self.active = false;
                self.pixels = Vec::new();
            }
        }
        self.animate(
            ctx.input(|i| i.stable_dt),
            crate::theme::reduced_motion(ctx),
        );
        if self.active || self.scale != 0.0 || self.velocity != 0.0 {
            ctx.request_repaint();
        } else {
            self.texture = None;
            self.position = None;
        }
        color
    }

    pub fn paint(&self, ctx: &egui::Context) {
        let (Some(pointer), Some(texture)) = (self.position, &self.texture) else {
            return;
        };
        let scale = self.scale.max(0.0);
        if scale < 0.001 {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("eyedropper-loupe"),
        ));
        let radius = RADIUS * scale;
        // The circle grows out of the pointer and travels above it with the same spring.
        let mut destination = pointer - Vec2::new(0.0, RADIUS + 30.0);
        let screen = ctx.content_rect();
        destination.x = destination.x.clamp(
            screen.left() + RADIUS + 10.0,
            (screen.right() - RADIUS - 10.0).max(screen.left() + RADIUS + 10.0),
        );
        if destination.y - RADIUS < screen.top() + 8.0 {
            destination.y = pointer.y + RADIUS + 30.0;
        }
        let center = pointer + (destination - pointer) * scale;
        let opacity = scale.min(1.0);
        // Layered translucent rings form a soft, blurred halo, without blurring the sampled pixel.
        for i in (1..=12).rev() {
            painter.circle_filled(
                center + Vec2::new(0.0, 3.0 * scale),
                radius + i as f32 * scale,
                Color32::from_black_alpha((3.0 * opacity) as u8),
            );
        }
        let mut mesh = egui::Mesh::with_texture(texture.id());
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center,
            uv: Pos2::new(0.5, 0.5),
            color: Color32::WHITE.gamma_multiply(opacity),
        });
        const SEGMENTS: usize = 96;
        // Two rings feather the actual image edge to transparent.
        for (r, alpha) in [(radius - 3.0 * scale, opacity), (radius + 2.0 * scale, 0.0)] {
            for i in 0..SEGMENTS {
                let direction = Vec2::angled(i as f32 * std::f32::consts::TAU / SEGMENTS as f32);
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: center + direction * r,
                    uv: Pos2::new(0.5, 0.5) + direction * (r / radius * 0.5),
                    color: Color32::WHITE.gamma_multiply(alpha),
                });
            }
        }
        for i in 0..SEGMENTS as u32 {
            let next = (i + 1) % SEGMENTS as u32;
            mesh.add_triangle(0, 1 + i, 1 + next);
            mesh.add_triangle(1 + i, 1 + SEGMENTS as u32 + i, 1 + next);
            mesh.add_triangle(
                1 + next,
                1 + SEGMENTS as u32 + i,
                1 + SEGMENTS as u32 + next,
            );
        }
        painter.add(mesh);
        painter.circle_stroke(
            center,
            radius - 2.0 * scale,
            Stroke::new(2.0 * scale, self.color.gamma_multiply(opacity)),
        );
        let pixel = egui::Rect::from_center_size(center, Vec2::splat(radius * 2.0 / SIDE as f32));
        painter.rect_stroke(
            pixel,
            0.0,
            Stroke::new(2.5 * scale, Color32::BLACK.gamma_multiply(opacity)),
            egui::StrokeKind::Outside,
        );
        painter.rect_stroke(
            pixel,
            0.0,
            Stroke::new(scale, Color32::WHITE.gamma_multiply(opacity)),
            egui::StrokeKind::Outside,
        );
        let label = format!(
            "#{:02X}{:02X}{:02X}",
            self.color.r(),
            self.color.g(),
            self.color.b()
        );
        let label_rect = egui::Rect::from_center_size(
            center + Vec2::new(0.0, radius * 0.65),
            Vec2::new(76.0, 23.0) * scale,
        );
        painter.rect_filled(
            label_rect,
            8.0 * scale,
            Color32::from_black_alpha((190.0 * opacity) as u8),
        );
        painter.text(
            label_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(12.0 * scale),
            Color32::WHITE.gamma_multiply(opacity),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_composited_pixels_exactly_and_reuses_the_magnifier_texture() {
        let ctx = egui::Context::default();
        let mut doc = Document::new(2, 2, false);
        doc.layers[0].pixels[..8].copy_from_slice(&[12, 34, 56, 255, 210, 150, 70, 255]);
        let mut state = State::default();
        state.begin(&doc);
        assert_eq!(
            state.sample(&ctx, [0.99, 0.25]),
            Some(Color32::from_rgb(12, 34, 56))
        );
        let texture = state.texture.as_ref().unwrap().id();
        assert_eq!(
            state.sample(&ctx, [1.01, 0.25]),
            Some(Color32::from_rgb(210, 150, 70))
        );
        assert_eq!(state.texture.as_ref().unwrap().id(), texture);
        assert_eq!(state.sample(&ctx, [-0.01, 0.0]), None);
        assert_eq!(state.sample(&ctx, [2.0, 0.0]), None);
        assert_eq!(state.color, Color32::from_rgb(210, 150, 70));
        // Sampling is read-only, and hidden layers must not affect the chosen color.
        doc.add_layer();
        doc.layers[1].pixels[..4].copy_from_slice(&[255, 0, 0, 255]);
        doc.layers[1].visible = false;
        state.begin(&doc);
        assert_eq!(
            state.sample(&ctx, [0.0, 0.0]),
            Some(Color32::from_rgb(12, 34, 56))
        );
    }

    #[test]
    fn spring_overshoots_then_settles_and_reduced_motion_is_immediate() {
        let mut state = State {
            active: true,
            ..Default::default()
        };
        let mut peak: f32 = 0.0;
        for _ in 0..180 {
            state.animate(1.0 / 60.0, false);
            peak = peak.max(state.scale);
        }
        assert!(peak > 1.01 && peak < 1.2);
        assert_eq!(state.scale, 1.0);
        state.active = false;
        for _ in 0..180 {
            state.animate(1.0 / 60.0, false);
        }
        assert_eq!(state.scale, 0.0);
        state.active = true;
        state.animate(0.01, true);
        assert_eq!(state.scale, 1.0);
        state.active = false;
        state.animate(0.01, true);
        assert_eq!(state.scale, 0.0);
    }
}
