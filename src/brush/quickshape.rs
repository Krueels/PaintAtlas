//! QuickShape geometry and recognition, independent of canvas widgets.
use eframe::egui::Vec2;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};
pub trait VecGeometry {
    fn distance(self, other: Vec2) -> f32;
    fn distance_sq(self, other: Vec2) -> f32;
    fn lerp(self, other: Vec2, t: f32) -> Vec2;
}
impl VecGeometry for Vec2 {
    fn distance(self, other: Vec2) -> f32 {
        (self - other).length()
    }
    fn distance_sq(self, other: Vec2) -> f32 {
        (self - other).length_sq()
    }
    fn lerp(self, other: Vec2, t: f32) -> Vec2 {
        self + (other - self) * t
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: bool,
    pub delay: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            delay: 0.65,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Line,
    Arc,
    Polyline,
    Ellipse,
    Circle,
    Triangle,
    Rectangle,
    Square,
    Quadrilateral,
}
impl Kind {
    pub const ALL: [Self; 9] = [
        Self::Line,
        Self::Arc,
        Self::Polyline,
        Self::Ellipse,
        Self::Circle,
        Self::Triangle,
        Self::Rectangle,
        Self::Square,
        Self::Quadrilateral,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Arc => "Arc",
            Self::Polyline => "Polyline",
            Self::Ellipse => "Ellipse",
            Self::Circle => "Circle",
            Self::Triangle => "Triangle",
            Self::Rectangle => "Rectangle",
            Self::Square => "Square",
            Self::Quadrilateral => "Quadrilateral",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub kind: Kind,
    pub nodes: Vec<Vec2>,
    pub closed: bool,
}
fn distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    p.distance(a + d * ((p - a).dot(d) / d.length_sq().max(0.0001)).clamp(0.0, 1.0))
}
fn simplify(points: &[Vec2], tolerance: f32) -> Vec<Vec2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let far = (a + 1..b)
            .map(|i| (i, distance(points[i], points[a], points[b])))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, d)) = far
            && d > tolerance
        {
            keep[i] = true;
            stack.push((a, i));
            stack.push((i, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}
fn polygon(points: &[Vec2], tolerance: f32) -> Vec<Vec2> {
    let split = (1..points.len())
        .max_by(|a, b| {
            points[*a]
                .distance_sq(points[0])
                .total_cmp(&points[*b].distance_sq(points[0]))
        })
        .unwrap_or(1);
    let mut vertices = simplify(&points[..=split], tolerance);
    let mut rest = points[split..].to_vec();
    rest.push(points[0]);
    let other = simplify(&rest, tolerance);
    vertices.extend_from_slice(&other[1..other.len() - 1]);
    loop {
        if vertices.len() <= 3 {
            break;
        }
        let n = vertices.len();
        let redundant = (0..n).find(|i| {
            distance(
                vertices[*i],
                vertices[(*i + n - 1) % n],
                vertices[(*i + 1) % n],
            ) < tolerance
        });
        if let Some(i) = redundant {
            vertices.remove(i);
        } else {
            break;
        }
    }
    vertices
}
fn axes(points: &[Vec2]) -> (Vec2, Vec2, Vec2) {
    let mean = points.iter().copied().fold(Vec2::ZERO, |a, b| a + b) / points.len() as f32;
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for p in points {
        let q = *p - mean;
        xx += q.x * q.x;
        yy += q.y * q.y;
        xy += q.x * q.y;
    }
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    let u = Vec2::angled(angle);
    let v = u.rot90();
    let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for p in points {
        let q = Vec2::new(p.dot(u), p.dot(v));
        lo = lo.min(q);
        hi = hi.max(q);
    }
    let c = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    (u * c.x + v * c.y, u * half.x.max(0.5), v * half.y.max(0.5))
}
fn circle(a: Vec2, b: Vec2, c: Vec2) -> Option<(Vec2, f32, f32)> {
    let d = b - a;
    let e = c - a;
    let det = 2.0 * (d.x * e.y - d.y * e.x);
    if det.abs() < 0.001 {
        return None;
    }
    let offset = Vec2::new(
        e.y * d.length_sq() - d.y * e.length_sq(),
        d.x * e.length_sq() - e.x * d.length_sq(),
    ) / det;
    let center = a + offset;
    let start = (a - center).angle();
    let mid = ((b - center).angle() - start).rem_euclid(TAU);
    let end = ((c - center).angle() - start).rem_euclid(TAU);
    Some((center, start, if mid <= end { end } else { end - TAU }))
}
impl Shape {
    pub fn center(&self) -> Vec2 {
        if matches!(self.kind, Kind::Ellipse | Kind::Circle) {
            self.nodes[0]
        } else {
            self.nodes.iter().copied().fold(Vec2::ZERO, |a, b| a + b) / self.nodes.len() as f32
        }
    }
    pub fn transformed(&self, pivot: Vec2, scale: f32, angle: f32, translation: Vec2) -> Self {
        let (s, c) = angle.sin_cos();
        let mut result = self.clone();
        for p in &mut result.nodes {
            let q = (*p - pivot) * scale;
            *p = pivot + Vec2::new(c * q.x - s * q.y, s * q.x + c * q.y) + translation;
        }
        result
    }
    pub fn regular(&self) -> Self {
        let mut s = self.clone();
        match s.kind {
            Kind::Ellipse | Kind::Circle => {
                let c = s.nodes[0];
                let u = s.nodes[1] - c;
                let v = s.nodes[2] - c;
                let radius = u.length().max(v.length());
                s.nodes[1] = c + u.normalized() * radius;
                s.nodes[2] = c + v.normalized() * radius;
                s.kind = Kind::Circle;
            }
            Kind::Rectangle | Kind::Square | Kind::Quadrilateral => {
                let c = s.center();
                let u = (s.nodes[1] - s.nodes[0]).normalized();
                let v = u.rot90();
                let r = s
                    .nodes
                    .iter()
                    .map(|p| (*p - c).dot(u).abs().max((*p - c).dot(v).abs()))
                    .fold(1.0_f32, f32::max);
                s.nodes = vec![
                    c - u * r - v * r,
                    c + u * r - v * r,
                    c + u * r + v * r,
                    c - u * r + v * r,
                ];
                s.kind = Kind::Square;
            }
            Kind::Triangle => {
                let c = s.center();
                let radius = s.nodes.iter().map(|p| p.distance(c)).sum::<f32>() / 3.0;
                let angle = (s.nodes[0] - c).angle();
                s.nodes = (0..3)
                    .map(|i| c + Vec2::angled(angle + i as f32 * TAU / 3.0) * radius)
                    .collect();
            }
            _ => {}
        }
        s
    }
    pub fn path(&self) -> Vec<Vec2> {
        match self.kind {
            Kind::Ellipse | Kind::Circle => {
                let (c, u, v) = (
                    self.nodes[0],
                    self.nodes[1] - self.nodes[0],
                    self.nodes[2] - self.nodes[0],
                );
                let count = ((u.length() + v.length()) * PI / 2.0)
                    .ceil()
                    .clamp(48.0, 512.0) as usize;
                (0..=count)
                    .map(|i| {
                        let t = i as f32 / count as f32 * TAU;
                        c + u * t.cos() + v * t.sin()
                    })
                    .collect()
            }
            Kind::Arc => {
                let [a, b, c] = [self.nodes[0], self.nodes[1], self.nodes[2]];
                if let Some((center, start, sweep)) = circle(a, b, c) {
                    let radius = a.distance(center);
                    if radius * sweep.abs() > 32768.0 {
                        return vec![a, b, c];
                    }
                    let count = (radius * sweep.abs() / 2.0).ceil().clamp(24.0, 512.0) as usize;
                    (0..=count)
                        .map(|i| {
                            center + Vec2::angled(start + sweep * i as f32 / count as f32) * radius
                        })
                        .collect()
                } else {
                    vec![a, b, c]
                }
            }
            _ => {
                let mut p = self.nodes.clone();
                if self.closed {
                    p.push(p[0]);
                }
                p
            }
        }
    }
    pub fn convert(&self, kind: Kind) -> Self {
        if kind == self.kind {
            return self.clone();
        }
        let path = self.path();
        let (c, u, v) = axes(&path);
        let mut shape = Self {
            kind,
            nodes: Vec::new(),
            closed: matches!(
                kind,
                Kind::Ellipse
                    | Kind::Circle
                    | Kind::Triangle
                    | Kind::Rectangle
                    | Kind::Square
                    | Kind::Quadrilateral
            ),
        };
        shape.nodes = match kind {
            Kind::Line => {
                let end = if self.closed {
                    c + u + v
                } else {
                    *path.last().unwrap()
                };
                vec![path[0], end]
            }
            Kind::Arc => vec![c - u, c - v, c + u],
            Kind::Ellipse | Kind::Circle => vec![c, c + u, c + v],
            Kind::Triangle => vec![c - v, c + u + v, c - u + v],
            Kind::Polyline => {
                shape.closed = self.closed;
                let mut p = simplify(&path, u.length().max(v.length()) * 0.08);
                if shape.closed {
                    p.pop();
                }
                p
            }
            _ => vec![c - u - v, c + u - v, c + u + v, c - u + v],
        };
        if matches!(kind, Kind::Circle | Kind::Square) {
            shape = shape.regular();
        }
        shape
    }
    pub fn move_node(&mut self, index: usize, p: Vec2, regular: bool) {
        if matches!(self.kind, Kind::Ellipse | Kind::Circle) {
            let c = self.nodes[0];
            let u = self.nodes[1] - c;
            let v = self.nodes[2] - c;
            match index {
                0 => {
                    let delta = p - c;
                    for q in &mut self.nodes {
                        *q += delta;
                    }
                }
                1 => {
                    let d = p - c;
                    if d.length() > 0.5 {
                        self.nodes[1] = p;
                        self.nodes[2] = c + d.normalized().rot90()
                            * if self.kind == Kind::Circle {
                                d.length()
                            } else {
                                v.length()
                            };
                    }
                }
                _ => {
                    let n = u.normalized().rot90();
                    let length = (p - c).dot(n).abs().max(0.5);
                    self.nodes[2] = c + n * length;
                    if self.kind == Kind::Circle {
                        self.nodes[1] = c + u.normalized() * length;
                    }
                }
            }
        } else if matches!(self.kind, Kind::Rectangle | Kind::Square) {
            let u = (self.nodes[1] - self.nodes[0]).normalized();
            let v = (self.nodes[3] - self.nodes[0]).normalized();
            let old = self.nodes[index];
            let opposite = self.nodes[(index + 2) % 4];
            let delta = p - opposite;
            let (mut x, mut y) = (delta.dot(u), delta.dot(v));
            if self.kind == Kind::Square || regular {
                let length = x.abs().max(y.abs()).max(1.0);
                x = x.signum() * length;
                y = y.signum() * length;
            }
            for q in &mut self.nodes {
                let d = *q - old;
                *q = opposite
                    + u * if d.dot(u).abs() < 0.01 { x } else { 0.0 }
                    + v * if d.dot(v).abs() < 0.01 { y } else { 0.0 };
            }
        } else {
            self.nodes[index] = p;
        }
        if regular {
            *self = self.regular();
        }
    }
}
pub fn recognize(points: &[Vec2], zoom: f32) -> Option<Shape> {
    if points.len() < 2 {
        return None;
    }
    // Fit by traveled distance, so drawing slowly on one side cannot bias the axes.
    let lengths: Vec<_> = points.windows(2).map(|p| p[0].distance(p[1])).collect();
    let total: f32 = lengths.iter().sum();
    if total * zoom < 28.0 {
        return None;
    }
    let mut normalized = Vec::with_capacity(192);
    let (mut edge, mut traveled) = (0, 0.0);
    for i in 0..192 {
        let target = total * i as f32 / 191.0;
        while edge + 1 < lengths.len() && traveled + lengths[edge] < target {
            traveled += lengths[edge];
            edge += 1;
        }
        normalized.push(points[edge].lerp(
            points[edge + 1],
            ((target - traveled) / lengths[edge].max(0.0001)).clamp(0.0, 1.0),
        ));
    }
    let points = normalized.as_slice();
    let (c, u, v) = axes(points);
    let span = (u.length_sq() + v.length_sq()).sqrt() * 2.0;
    let length = points.windows(2).map(|p| p[0].distance(p[1])).sum::<f32>();
    if span * zoom < 24.0 || length * zoom < 28.0 {
        return None;
    }
    let a = points[0];
    let b = *points.last()?;
    let closed = a.distance(b) < (span * 0.22).max(6.0 / zoom) && length > span * 1.8;
    if !closed && a.distance(b) > span * 0.55 {
        let error = points.iter().map(|p| distance(*p, a, b)).sum::<f32>() / points.len() as f32;
        if error < span * 0.045 && length < a.distance(b) * 1.6 {
            return Some(Shape {
                kind: Kind::Line,
                nodes: vec![a, b],
                closed: false,
            });
        }
    }
    if closed {
        let vertices = polygon(points, span * 0.028);
        if vertices.len() == 3 {
            return Some(Shape {
                kind: Kind::Triangle,
                nodes: vertices,
                closed: true,
            });
        }
        if vertices.len() == 4 {
            let right = (0..4).all(|i| {
                (vertices[(i + 1) % 4] - vertices[i])
                    .normalized()
                    .dot((vertices[(i + 3) % 4] - vertices[i]).normalized())
                    .abs()
                    < 0.3
            });
            if right {
                let axis = (vertices[1] - vertices[0]).normalized();
                let normal = axis.rot90();
                let mut lo = Vec2::splat(f32::INFINITY);
                let mut hi = Vec2::splat(f32::NEG_INFINITY);
                for p in points {
                    let q = Vec2::new(p.dot(axis), p.dot(normal));
                    lo = lo.min(q);
                    hi = hi.max(q);
                }
                let nodes = [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)]
                    .map(|q| axis * q.x + normal * q.y)
                    .to_vec();
                return Some(Shape {
                    kind: Kind::Rectangle,
                    nodes,
                    closed: true,
                });
            }
        }
        let axis = u.normalized();
        let normal = v.normalized();
        let error = points
            .iter()
            .map(|p| {
                let d = *p - c;
                ((d.dot(axis) / u.length()).hypot(d.dot(normal) / v.length()) - 1.0).abs()
            })
            .sum::<f32>()
            / points.len() as f32;
        if error < 0.13 {
            return Some(Shape {
                kind: Kind::Ellipse,
                nodes: vec![c, c + u, c + v],
                closed: true,
            });
        }
        if (4..=8).contains(&vertices.len()) {
            return Some(Shape {
                kind: if vertices.len() == 4 {
                    Kind::Quadrilateral
                } else {
                    Kind::Polyline
                },
                nodes: vertices,
                closed: true,
            });
        }
    } else {
        let mid = points[points.len() / 2];
        if let Some((center, _, sweep)) = circle(a, mid, b) {
            let radius = a.distance(center);
            let error = points
                .iter()
                .map(|p| (p.distance(center) - radius).abs())
                .sum::<f32>()
                / points.len() as f32;
            if error < span * 0.035
                && sweep.abs() > 0.2
                && (length - radius * sweep.abs()).abs() < length * 0.18
            {
                return Some(Shape {
                    kind: Kind::Arc,
                    nodes: vec![a, mid, b],
                    closed: false,
                });
            }
        }
        let vertices = simplify(points, span * 0.035);
        if (3..=8).contains(&vertices.len()) && length < span * 6.0 {
            return Some(Shape {
                kind: Kind::Polyline,
                nodes: vertices,
                closed: false,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edges(vertices: &[Vec2], closed: bool) -> Vec<Vec2> {
        let mut vertices = vertices.to_vec();
        if closed {
            vertices.push(vertices[0]);
        }
        let mut points = Vec::new();
        for edge in vertices.windows(2) {
            for i in 0..20 {
                points.push(edge[0].lerp(edge[1], i as f32 / 20.0));
            }
        }
        points.push(*vertices.last().unwrap());
        points
    }
    #[test]
    fn detects_noisy_line_rotated_rectangle_triangle_and_polyline() {
        let points: Vec<_> = (0..80)
            .map(|i| {
                Vec2::new(
                    i as f32 * 4.0,
                    i as f32 * 0.5 + (i as f32 * 1.7).sin() * 3.0,
                )
            })
            .collect();
        assert_eq!(recognize(&points, 1.0).unwrap().kind, Kind::Line);
        let r = Shape {
            kind: Kind::Rectangle,
            nodes: vec![
                Vec2::new(-100.0, -70.0),
                Vec2::new(100.0, -70.0),
                Vec2::new(100.0, 70.0),
                Vec2::new(-100.0, 70.0),
            ],
            closed: true,
        }
        .transformed(Vec2::ZERO, 1.0, 0.6, Vec2::new(250.0, 250.0));
        let points = edges(&r.nodes, true);
        assert_eq!(recognize(&points, 1.0).unwrap().kind, Kind::Rectangle);
        let tri = edges(
            &[
                Vec2::new(20.0, 20.0),
                Vec2::new(200.0, 40.0),
                Vec2::new(100.0, 200.0),
            ],
            true,
        );
        assert_eq!(recognize(&tri, 1.0).unwrap().kind, Kind::Triangle);
        let poly = edges(
            &[
                Vec2::ZERO,
                Vec2::new(100.0, 0.0),
                Vec2::new(100.0, 100.0),
                Vec2::new(200.0, 100.0),
            ],
            false,
        );
        assert_eq!(recognize(&poly, 1.0).unwrap().kind, Kind::Polyline);
    }
    #[test]
    fn recognizes_rotated_ellipse_and_arc_but_rejects_dots_and_dense_scribbles() {
        let ellipse = Shape {
            kind: Kind::Ellipse,
            nodes: vec![Vec2::ZERO, Vec2::new(150.0, 0.0), Vec2::new(0.0, 70.0)],
            closed: true,
        }
        .transformed(Vec2::ZERO, 1.0, 0.7, Vec2::new(250.0, 250.0));
        assert_eq!(recognize(&ellipse.path(), 1.0).unwrap().kind, Kind::Ellipse);
        let biased: Vec<_> = (0..=650)
            .map(|i| {
                let t = if i < 600 {
                    i as f32 / 600.0 * PI * 0.5
                } else {
                    PI * 0.5 + (i - 600) as f32 / 50.0 * PI * 1.5
                };
                Vec2::new(250.0 + 150.0 * t.cos(), 250.0 + 70.0 * t.sin())
            })
            .collect();
        assert_eq!(recognize(&biased, 1.0).unwrap().kind, Kind::Ellipse);
        let arc: Vec<_> = (0..80)
            .map(|i| Vec2::angled(0.2 + i as f32 / 79.0 * 2.3) * 150.0)
            .collect();
        assert_eq!(recognize(&arc, 1.0).unwrap().kind, Kind::Arc);
        assert!(recognize(&[Vec2::ZERO, Vec2::new(1.0, 2.0)], 1.0).is_none());
        let scribble: Vec<_> = (0..60)
            .map(|i| Vec2::new(if i % 2 == 0 { 0.0 } else { 200.0 }, i as f32 * 3.0))
            .collect();
        assert!(recognize(&scribble, 1.0).is_none());
    }
    #[test]
    fn regular_forms_and_node_edits_keep_rectangles_orthogonal() {
        let ellipse = Shape {
            kind: Kind::Ellipse,
            nodes: vec![Vec2::ZERO, Vec2::new(150.0, 0.0), Vec2::new(0.0, 70.0)],
            closed: true,
        };
        let circle = ellipse.regular();
        assert_eq!(circle.kind, Kind::Circle);
        assert!((circle.nodes[1].length() - circle.nodes[2].length()).abs() < 0.001);
        let mut rectangle = ellipse.convert(Kind::Rectangle);
        rectangle.move_node(0, Vec2::new(-180.0, -80.0), false);
        let a = rectangle.nodes[1] - rectangle.nodes[0];
        let b = rectangle.nodes[3] - rectangle.nodes[0];
        assert!(a.dot(b).abs() < 0.01);
        let square = rectangle.regular();
        assert!(
            ((square.nodes[1] - square.nodes[0]).length()
                - (square.nodes[3] - square.nodes[0]).length())
            .abs()
                < 0.01
        );
        for kind in Kind::ALL {
            let s = ellipse.convert(kind);
            assert!(s.nodes.len() >= 2);
            assert!(s.path().iter().all(|p| p.is_finite()));
        }
    }
}
