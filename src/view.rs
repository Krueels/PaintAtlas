use eframe::egui::{self, Color32, Pos2, Vec2};
#[derive(Clone, Copy)]
pub struct CanvasView {
    pub center: Pos2,
    pub size: Vec2,
    pub zoom: f32,
    pub angle: f32,
    pub mirrored: bool,
}
impl CanvasView {
    pub fn screen(self, p: [f32; 2]) -> Pos2 {
        let mut v = Vec2::new(p[0], p[1]) - self.size * 0.5;
        if self.mirrored {
            v.x = -v.x;
        }
        self.center + egui::emath::Rot2::from_angle(self.angle) * (v * self.zoom)
    }
    pub fn document(self, p: Pos2) -> [f32; 2] {
        let mut v = egui::emath::Rot2::from_angle(-self.angle) * (p - self.center) / self.zoom;
        if self.mirrored {
            v.x = -v.x;
        }
        v += self.size * 0.5;
        [v.x, v.y]
    }
    pub fn corners(self) -> [Pos2; 4] {
        [
            self.screen([0.0, 0.0]),
            self.screen([self.size.x, 0.0]),
            self.screen([self.size.x, self.size.y]),
            self.screen([0.0, self.size.y]),
        ]
    }
    pub fn mesh(self, texture: egui::TextureId, uv_size: Vec2, color: Color32) -> egui::Mesh {
        let mut mesh = egui::Mesh::with_texture(texture);
        let uv = [
            Pos2::ZERO,
            Pos2::new(uv_size.x, 0.0),
            Pos2::new(uv_size.x, uv_size.y),
            Pos2::new(0.0, uv_size.y),
        ];
        for (pos, uv) in self.corners().into_iter().zip(uv) {
            mesh.vertices.push(egui::epaint::Vertex { pos, uv, color });
        }
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        mesh
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transformed_input_roundtrips_for_rotated_mirrored_canvas() {
        for angle in [0.0, 0.4, 1.57, -2.2] {
            for mirrored in [true, false] {
                let view = CanvasView {
                    center: Pos2::new(220.0, 173.0),
                    size: Vec2::new(1280.0, 800.0),
                    zoom: 0.73,
                    angle,
                    mirrored,
                };
                for p in [[0.0, 0.0], [120.5, 79.0], [1280.0, 800.0]] {
                    let q = view.document(view.screen(p));
                    assert!((q[0] - p[0]).abs() < 0.001 && (q[1] - p[1]).abs() < 0.001);
                }
            }
        }
    }
}
