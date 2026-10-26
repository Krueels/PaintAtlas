//! HSV color wheel with a rotating saturation/value triangle.
use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Vec2};
use std::f32::consts::TAU;

#[derive(Clone, Copy)]
enum Drag {
    Hue,
    Triangle,
}
#[derive(Default)]
pub struct TriangleSelector {
    h: f32,
    s: f32,
    v: f32,
    last: Option<Color32>,
    drag: Option<Drag>,
}
struct Geometry {
    center: Pos2,
    outer: f32,
    inner: f32,
    vertices: [Pos2; 3], // pure hue, white, black
}
impl Geometry {
    fn new(rect: Rect, hue: f32) -> Self {
        let center = rect.center();
        let outer = (rect.width().min(rect.height()) * 0.5 - 4.0).max(1.0);
        let inner = (outer - (outer * 0.18).clamp(4.0, 20.0)).max(0.5);
        let radius = (inner - 5.0).max(1.0);
        let vertices = std::array::from_fn(|i| center + direction(hue + i as f32 / 3.0) * radius);
        Self {
            center,
            outer,
            inner,
            vertices,
        }
    }
    fn weights(&self, p: Pos2) -> [f32; 3] {
        let [a, b, c] = self.vertices;
        let ab = b - a;
        let ac = c - a;
        let ap = p - a;
        let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
        let d = cross(ab, ac);
        let w = cross(ap, ac) / d;
        let k = cross(ab, ap) / d;
        [1.0 - w - k, w, k]
    }
    fn project(&self, p: Pos2) -> [f32; 3] {
        let weights = self.weights(p);
        if weights.iter().all(|w| *w >= 0.0) {
            return weights;
        }
        let mut best = [1.0, 0.0, 0.0];
        let mut distance = f32::INFINITY;
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let a = self.vertices[i];
            let edge = self.vertices[j] - a;
            let t = ((p - a).dot(edge) / edge.length_sq()).clamp(0.0, 1.0);
            let q = a + edge * t;
            if q.distance_sq(p) < distance {
                distance = q.distance_sq(p);
                best = [0.0; 3];
                best[i] = 1.0 - t;
                best[j] = t;
            }
        }
        best
    }
    fn point(&self, weights: [f32; 3]) -> Pos2 {
        let [a, b, c] = self.vertices;
        a + (b - a) * weights[1] + (c - a) * weights[2]
    }
}
fn direction(h: f32) -> Vec2 {
    let angle = h * TAU - std::f32::consts::FRAC_PI_2;
    Vec2::new(angle.cos(), angle.sin())
}
fn hue_at(p: Vec2) -> f32 {
    ((p.y.atan2(p.x) + std::f32::consts::FRAC_PI_2) / TAU).rem_euclid(1.0)
}
fn hsv(color: Color32) -> [f32; 3] {
    let [r, g, b] = [
        color.r() as f32 / 255.0,
        color.g() as f32 / 255.0,
        color.b() as f32 / 255.0,
    ];
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= f32::EPSILON {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if max > 0.0 { d / max } else { 0.0 }, max]
}
fn color(h: f32, s: f32, v: f32) -> Color32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let [r, g, b] = match h as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    let channel = |a: f32| ((a + v - c) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(channel(r), channel(g), channel(b))
}
impl TriangleSelector {
    fn sync(&mut self, selected: Color32) {
        if self.last == Some(selected) {
            return;
        }
        let [h, s, v] = hsv(selected);
        if v > 0.0 {
            self.s = s;
        }
        if s > 0.0001 && v > 0.0 {
            self.h = h;
        }
        self.v = v;
        self.last = Some(selected);
    }
    pub fn show(&mut self, ui: &mut egui::Ui, selected: &mut Color32) -> Response {
        self.sync(*selected);
        // The containing panel scrolls vertically. Size from its visible width,
        // rather than limiting the wheel to a fixed height or pixel maximum.
        let side = ui.available_width().min(ui.clip_rect().width()).max(1.0);
        let (rect, mut response) =
            ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::ColorButton,
                ui.is_enabled(),
                "Seletor triangular de cor",
            )
        });
        let geometry = Geometry::new(rect, self.h);
        let mut edited = false;
        if (response.is_pointer_button_down_on() || response.dragged() || response.clicked())
            && let Some(p) = response.interact_pointer_pos()
        {
            if self.drag.is_none() {
                let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
                let distance = origin.distance(geometry.center);
                let sv_marker =
                    geometry.point([self.v * self.s, self.v * (1.0 - self.s), 1.0 - self.v]);
                self.drag = if origin.distance(sv_marker) <= (side * 0.02).clamp(2.5, 5.0) + 2.0 {
                    Some(Drag::Triangle)
                } else if distance >= geometry.inner - 2.0 && distance <= geometry.outer + 3.0 {
                    Some(Drag::Hue)
                } else if geometry.weights(origin).iter().all(|w| *w >= -0.001) {
                    Some(Drag::Triangle)
                } else {
                    None
                };
            }
            match self.drag {
                Some(Drag::Hue) if p.distance(geometry.center) > 0.5 => {
                    self.h = hue_at(p - geometry.center);
                    edited = true;
                }
                Some(Drag::Triangle) => {
                    let [h, w, _] = geometry.project(p);
                    let value = (h + w).clamp(0.0, 1.0);
                    if value > 0.0001 {
                        self.s = (h / value).clamp(0.0, 1.0);
                    }
                    self.v = value;
                    edited = true;
                }
                _ => {}
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.drag = None;
        }
        let geometry = Geometry::new(rect, self.h);
        let painter = ui.painter();
        let mut ring = egui::Mesh::default();
        for i in 0..=120 {
            let hue = i as f32 / 120.0;
            let dir = direction(hue);
            let col = color(hue, 1.0, 1.0);
            ring.colored_vertex(geometry.center + dir * geometry.inner, col);
            ring.colored_vertex(geometry.center + dir * geometry.outer, col);
            if i < 120 {
                let k = i * 2;
                ring.indices
                    .extend_from_slice(&[k, k + 1, k + 2, k + 1, k + 3, k + 2]);
            }
        }
        painter.add(egui::Shape::mesh(ring));
        let n = 24;
        let mut triangle = egui::Mesh::default();
        let mut indices = Vec::new();
        for i in 0..=n {
            let mut row = Vec::new();
            for j in 0..=n - i {
                let h = i as f32 / n as f32;
                let w = j as f32 / n as f32;
                let v = h + w;
                row.push(triangle.vertices.len() as u32);
                triangle.colored_vertex(
                    geometry.point([h, w, 1.0 - v]),
                    color(self.h, if v > 0.0 { h / v } else { 0.0 }, v),
                );
            }
            indices.push(row);
        }
        for i in 0..n {
            for j in 0..n - i {
                triangle.indices.extend_from_slice(&[
                    indices[i][j],
                    indices[i + 1][j],
                    indices[i][j + 1],
                ]);
                if i + j < n - 1 {
                    triangle.indices.extend_from_slice(&[
                        indices[i + 1][j],
                        indices[i + 1][j + 1],
                        indices[i][j + 1],
                    ]);
                }
            }
        }
        painter.add(egui::Shape::mesh(triangle));
        painter.add(egui::Shape::closed_line(
            geometry.vertices.to_vec(),
            Stroke::new(1.0_f32, Color32::from_gray(65)),
        ));
        let marker = |p: Pos2, r: f32| {
            painter.circle_stroke(p, r, Stroke::new(3.0_f32, Color32::from_black_alpha(200)));
            painter.circle_stroke(p, r, Stroke::new(1.2_f32, Color32::WHITE));
        };
        marker(
            geometry.center + direction(self.h) * (geometry.inner + geometry.outer) * 0.5,
            (side * 0.025).clamp(2.5, 6.0),
        );
        marker(
            geometry.point([self.v * self.s, self.v * (1.0 - self.s), 1.0 - self.v]),
            (side * 0.02).clamp(2.5, 5.0),
        );
        response.clone().on_hover_text(
            "Anel: matiz. Triângulo: saturação e valor. Vértices: matiz pura, branco e preto.",
        );
        let mut degrees = self.h * 360.0;
        let mut saturation = self.s * 100.0;
        let mut value = self.v * 100.0;
        ui.horizontal_wrapped(|ui| {
            edited |= ui
                .add(
                    egui::DragValue::new(&mut degrees)
                        .range(0.0..=360.0)
                        .speed(1.0)
                        .max_decimals(1)
                        .prefix("H ")
                        .suffix("°"),
                )
                .on_hover_text("Matiz")
                .changed();
            edited |= ui
                .add(
                    egui::DragValue::new(&mut saturation)
                        .range(0.0..=100.0)
                        .speed(0.5)
                        .max_decimals(1)
                        .prefix("S ")
                        .suffix("%"),
                )
                .on_hover_text("Saturação")
                .changed();
            edited |= ui
                .add(
                    egui::DragValue::new(&mut value)
                        .range(0.0..=100.0)
                        .speed(0.5)
                        .max_decimals(1)
                        .prefix("V ")
                        .suffix("%"),
                )
                .on_hover_text("Valor / brilho")
                .changed();
        });
        if edited {
            self.h = (degrees / 360.0).rem_euclid(1.0);
            self.s = saturation / 100.0;
            self.v = value / 100.0;
            *selected = color(self.h, self.s, self.v);
            response.mark_changed();
        }
        self.last = Some(*selected);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hsv_roundtrips_rgb_and_triangle_weights_roundtrip_sv() {
        for r in (0..=255).step_by(51) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(51) {
                    let original = Color32::from_rgb(r, g, b);
                    let [h, s, v] = hsv(original);
                    let result = color(h, s, v);
                    assert!(
                        original.r().abs_diff(result.r()) <= 1
                            && original.g().abs_diff(result.g()) <= 1
                            && original.b().abs_diff(result.b()) <= 1
                    );
                }
            }
        }
        for hue in [0.0, 0.2, 0.7] {
            let g = Geometry::new(Rect::from_min_size(Pos2::ZERO, Vec2::splat(240.0)), hue);
            for s in [0.0, 0.3, 1.0] {
                for v in [0.0, 0.4, 1.0] {
                    let expected = [v * s, v * (1.0 - s), 1.0 - v];
                    let weights = g.weights(g.point(expected));
                    for (a, b) in weights.into_iter().zip(expected) {
                        assert!((a - b).abs() < 0.0001);
                    }
                }
            }
            for p in [
                Pos2::new(-1000.0, 100.0),
                Pos2::new(120.0, 120.0),
                Pos2::new(1000.0, 1000.0),
            ] {
                let w = g.project(p);
                assert!(w.iter().all(|v| *v >= -0.0001 && *v <= 1.0001));
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 0.0001);
            }
        }
    }
    #[test]
    fn selector_tracks_panel_width_in_scroll_area() {
        let ctx = egui::Context::default();
        let mut picker = TriangleSelector::default();
        let mut selected = Color32::RED;
        let mut sizes = Vec::new();
        for width in [180.0, 600.0, 240.0] {
            // Repeat to include the scroll area's settled layout.
            for _ in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 430.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            egui::ScrollArea::both()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let available =
                                        ui.available_width().min(ui.clip_rect().width());
                                    let rect = picker.show(ui, &mut selected).rect;
                                    assert!((rect.width() - available).abs() < 0.1);
                                    assert!((rect.width() - rect.height()).abs() < 0.1);
                                    sizes.push(rect.width());
                                });
                        });
                    },
                );
            }
        }
        assert!(sizes[5] > 500.0);
        assert!(sizes[2] < sizes[8] && sizes[8] < sizes[5]);
        assert_eq!(selected, Color32::RED);
    }
    fn frame(
        ctx: &egui::Context,
        picker: &mut TriangleSelector,
        selected: &mut Color32,
        events: Vec<egui::Event>,
    ) -> Rect {
        let mut rect = Rect::NOTHING;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 500.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = picker.show(ui, selected).rect;
                });
            },
        );
        rect
    }
    fn click(ctx: &egui::Context, picker: &mut TriangleSelector, selected: &mut Color32, p: Pos2) {
        frame(ctx, picker, selected, vec![egui::Event::PointerMoved(p)]);
        for pressed in [true, false] {
            frame(
                ctx,
                picker,
                selected,
                vec![egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
    }
    #[test]
    fn triangle_selects_white_black_and_hue_and_preserves_hue_when_achromatic() {
        let ctx = egui::Context::default();
        let mut picker = TriangleSelector::default();
        let mut selected = Color32::RED;
        let rect = frame(&ctx, &mut picker, &mut selected, vec![]);
        let g = Geometry::new(rect, picker.h);
        click(&ctx, &mut picker, &mut selected, g.vertices[1]);
        assert_eq!(selected, Color32::WHITE);
        let p = g.center + direction(1.0 / 3.0) * (g.inner + g.outer) * 0.5;
        click(&ctx, &mut picker, &mut selected, p);
        assert_eq!(selected, Color32::WHITE);
        assert!((picker.h - 1.0 / 3.0).abs() < 0.0001);
        let g = Geometry::new(rect, picker.h);
        click(&ctx, &mut picker, &mut selected, g.vertices[0]);
        assert_eq!(selected, Color32::GREEN);
        click(&ctx, &mut picker, &mut selected, g.vertices[2]);
        assert_eq!(selected, Color32::BLACK);
        frame(&ctx, &mut picker, &mut selected, vec![]);
        assert!((picker.h - 1.0 / 3.0).abs() < 0.0001);
        click(&ctx, &mut picker, &mut selected, g.vertices[0]);
        assert_eq!(selected, Color32::GREEN);
    }
    #[test]
    fn dragging_triangle_outside_clamps_to_edge_without_changing_hue() {
        let ctx = egui::Context::default();
        let mut picker = TriangleSelector::default();
        let mut selected = Color32::RED;
        let rect = frame(&ctx, &mut picker, &mut selected, vec![]);
        let g = Geometry::new(rect, picker.h);
        let p = g.point([0.4, 0.3, 0.3]);
        frame(
            &ctx,
            &mut picker,
            &mut selected,
            vec![egui::Event::PointerMoved(p)],
        );
        frame(
            &ctx,
            &mut picker,
            &mut selected,
            vec![egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        frame(
            &ctx,
            &mut picker,
            &mut selected,
            vec![egui::Event::PointerMoved(g.center + Vec2::new(1000.0, 0.0))],
        );
        assert!(picker.h.abs() < 0.0001);
        assert!((0.0..=1.0).contains(&picker.s));
        assert!((0.0..=1.0).contains(&picker.v));
        assert!(picker.drag.is_some());
    }
    #[test]
    fn external_color_changes_sync_without_changing_the_selected_rgb() {
        let ctx = egui::Context::default();
        let mut picker = TriangleSelector::default();
        let mut selected = Color32::from_rgb(134, 72, 191);
        frame(&ctx, &mut picker, &mut selected, vec![]);
        assert_eq!(selected, Color32::from_rgb(134, 72, 191));
        selected = Color32::from_rgb(22, 180, 90);
        frame(&ctx, &mut picker, &mut selected, vec![]);
        assert_eq!(selected, Color32::from_rgb(22, 180, 90));
        let [h, s, v] = hsv(selected);
        assert_eq!([picker.h, picker.s, picker.v], [h, s, v]);
    }
}
