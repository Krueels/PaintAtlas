//! Layer-owned drawing rulers; all coordinates are in document pixels.
use crate::document::Document;
use eframe::egui::Vec2;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Symmetry,
    Parallel,
    Radial,
    Guide,
    Perspective1,
    Perspective2,
    Perspective3,
}
impl Kind {
    pub const ALL: [Self; 7] = [
        Self::Symmetry,
        Self::Parallel,
        Self::Radial,
        Self::Guide,
        Self::Perspective1,
        Self::Perspective2,
        Self::Perspective3,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Symmetry => "Symmetry",
            Self::Parallel => "Parallel lines",
            Self::Radial => "Radial lines",
            Self::Guide => "Straight guide",
            Self::Perspective1 => "Perspective: 1 point",
            Self::Perspective2 => "Perspective: 2 points",
            Self::Perspective3 => "Perspective: 3 points",
        }
    }
    pub fn perspective(self) -> usize {
        match self {
            Self::Perspective1 => 1,
            Self::Perspective2 => 2,
            Self::Perspective3 => 3,
            _ => 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Layer,
    Folder,
    All,
}
impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Layer => "This layer only",
            Self::Folder => "Same folder / group",
            Self::All => "All layers",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ruler {
    pub kind: Kind,
    pub origin: [f32; 2],
    pub angle: f32,
    pub lines: u8,
    pub mirror: bool,
    pub scope: Scope,
    pub visible: bool,
    pub enabled: bool,
    pub vanishing: [[f32; 2]; 3],
    pub directions: [bool; 3],
}
impl Ruler {
    pub fn new(kind: Kind, width: u32, height: u32) -> Self {
        let (w, h) = (width as f32, height as f32);
        Self {
            kind,
            origin: [w * 0.5, h * 0.5],
            angle: PI * 0.5,
            lines: 2,
            mirror: true,
            scope: Scope::Layer,
            visible: true,
            enabled: true,
            vanishing: [
                [
                    w * if kind.perspective() >= 2 { 0.15 } else { 0.5 },
                    h * 0.35,
                ],
                [w * 0.85, h * 0.35],
                [w * 0.5, h * 0.9],
            ],
            directions: [true; 3],
        }
    }
    pub fn valid(&self) -> bool {
        self.origin
            .iter()
            .chain(self.vanishing.iter().flatten())
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
            && self.angle.is_finite()
            && (2..=16).contains(&self.lines)
            && (!self.mirror || self.lines.is_multiple_of(2))
    }
    pub fn transforms(&self) -> Vec<Transform> {
        if self.kind != Kind::Symmetry {
            return Vec::new();
        }
        let n = self.lines as usize;
        (0..n)
            .map(|i| {
                let angle = if self.mirror {
                    (i / 2) as f32 * TAU / (n / 2) as f32
                } else {
                    i as f32 * TAU / n as f32
                };
                let (s, c) = angle.sin_cos();
                let mut matrix = [[c, -s], [s, c]];
                if self.mirror && i % 2 == 1 {
                    let (s2, c2) = (2.0 * self.angle).sin_cos();
                    matrix = [
                        [c * c2 - s * s2, c * s2 + s * c2],
                        [s * c2 + c * s2, s * s2 - c * c2],
                    ];
                }
                // Cardinal axes must remain pixel-exact, including mask edge samples.
                for value in matrix.iter_mut().flatten() {
                    if value.abs() < 0.000001 {
                        *value = 0.0;
                    } else if (value.abs() - 1.0).abs() < 0.000001 {
                        *value = value.signum();
                    }
                }
                Transform {
                    center: self.origin,
                    matrix,
                }
            })
            .collect()
    }
    pub fn map_geometry(&mut self, map: impl Fn([f32; 2]) -> [f32; 2]) {
        let o = self.origin;
        let q = map([o[0] + self.angle.cos(), o[1] + self.angle.sin()]);
        self.origin = map(o);
        self.angle = (q[1] - self.origin[1]).atan2(q[0] - self.origin[0]);
        self.vanishing = self.vanishing.map(map);
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub center: [f32; 2],
    pub matrix: [[f32; 2]; 2],
}
impl Transform {
    pub const IDENTITY: Self = Self {
        center: [0.0; 2],
        matrix: [[1.0, 0.0], [0.0, 1.0]],
    };
    pub fn point(self, p: [f32; 2]) -> [f32; 2] {
        let [x, y] = [p[0] - self.center[0], p[1] - self.center[1]];
        [
            self.center[0] + self.matrix[0][0] * x + self.matrix[0][1] * y,
            self.center[1] + self.matrix[1][0] * x + self.matrix[1][1] * y,
        ]
    }
}
impl Document {
    pub fn ruler_applies(&self, owner: usize, ruler: &Ruler) -> bool {
        match ruler.scope {
            Scope::Layer => owner == self.active,
            Scope::All => true,
            Scope::Folder => {
                if let Some(group) = self.layers[owner].group.or(self.layers[owner].parent) {
                    self.belongs_to(self.active, group)
                        || self.layers[self.active].group == Some(group)
                } else {
                    self.layers[self.active].parent.is_none()
                }
            }
        }
    }
    pub fn drawing_ruler(&self, symmetry: bool) -> Option<&Ruler> {
        std::iter::once(self.active)
            .chain((0..self.layers.len()).rev().filter(|i| *i != self.active))
            .flat_map(|i| self.layers[i].rulers.iter().rev().map(move |r| (i, r)))
            .find(|(i, r)| {
                r.visible
                    && r.enabled
                    && (r.kind == Kind::Symmetry) == symmetry
                    && self.ruler_applies(*i, r)
            })
            .map(|(_, r)| r)
    }
    pub fn begin_ruler_stroke(&mut self, enabled: bool, start: [f32; 2]) -> Option<StrokeGuide> {
        self.ruler_copies = if enabled {
            self.drawing_ruler(true)
                .map(Ruler::transforms)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        enabled
            .then(|| {
                self.drawing_ruler(false).map(|r| StrokeGuide {
                    ruler: r.clone(),
                    start: Vec2::from(start),
                    direction: None,
                    reset: false,
                })
            })
            .flatten()
    }
}
pub struct StrokeGuide {
    ruler: Ruler,
    start: Vec2,
    direction: Option<Vec2>,
    reset: bool,
}
impl StrokeGuide {
    pub fn take_reset(&mut self) -> bool {
        std::mem::take(&mut self.reset)
    }
    pub fn snap(&mut self, raw: [f32; 2]) -> [f32; 2] {
        let p = Vec2::from(raw);
        let delta = p - self.start;
        let origin = Vec2::from(self.ruler.origin);
        let axis = Vec2::angled(self.ruler.angle);
        match self.ruler.kind {
            Kind::Guide => (origin + axis * (p - origin).dot(axis)).into(),
            Kind::Parallel => (self.start + axis * delta.dot(axis)).into(),
            Kind::Radial => {
                let d = (self.start - origin).normalized();
                let d = if d == Vec2::ZERO {
                    if self.direction.is_none() && delta.length_sq() >= 4.0 {
                        self.direction = Some(delta.normalized());
                    }
                    self.direction.unwrap_or(Vec2::ZERO)
                } else {
                    d
                };
                if d == Vec2::ZERO {
                    self.start.into()
                } else {
                    (self.start + d * delta.dot(d)).into()
                }
            }
            Kind::Perspective1 | Kind::Perspective2 | Kind::Perspective3 => {
                if delta.length_sq() < 4.0 {
                    self.reset |= self.direction.is_some();
                    self.direction = None;
                    return self.start.into();
                }
                let n = self.ruler.kind.perspective();
                let mut candidates = [Vec2::ZERO; 4];
                let mut used = 0;
                for i in 0..n {
                    if self.ruler.directions[i] {
                        let d = (Vec2::from(self.ruler.vanishing[i]) - self.start).normalized();
                        if d != Vec2::ZERO {
                            candidates[used] = d;
                            used += 1;
                        }
                    }
                }
                let horizon = if n >= 2 {
                    (Vec2::from(self.ruler.vanishing[1]) - Vec2::from(self.ruler.vanishing[0]))
                        .normalized()
                } else {
                    Vec2::X
                };
                if n == 1 {
                    candidates[used] = horizon;
                    used += 1;
                }
                if n <= 2 {
                    candidates[used] = Vec2::new(-horizon.y, horizon.x);
                    used += 1;
                }
                if self.direction.is_none() {
                    self.direction = candidates[..used]
                        .iter()
                        .copied()
                        .max_by(|a, b| a.dot(delta).abs().total_cmp(&b.dot(delta).abs()));
                }
                self.direction
                    .map_or(raw, |d| (self.start + d * delta.dot(d)).into())
            }
            Kind::Symmetry => raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Brush, History};
    fn near(a: [f32; 2], b: [f32; 2]) {
        assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 0.001, "{a:?} vs {b:?}");
    }
    fn brush() -> Brush {
        Brush {
            color: [80, 110, 220, 255],
            size: 10.0,
            opacity: 0.7,
            hardness: 1.0,
            tip: None,
            spacing: 0.15,
        }
    }
    #[test]
    fn symmetry_mirrors_and_rotates_around_arbitrary_axis() {
        let mut r = Ruler::new(Kind::Symmetry, 128, 128);
        near(r.transforms()[1].point([20.0, 30.0]), [108.0, 30.0]);
        r.angle = std::f32::consts::FRAC_PI_4;
        near(r.transforms()[1].point([20.0, 30.0]), [30.0, 20.0]);
        r.lines = 5;
        r.mirror = false;
        for t in r.transforms() {
            let p = t.point([84.0, 64.0]);
            assert!(((p[0] - 64.0).hypot(p[1] - 64.0) - 20.0).abs() < 0.001);
        }
        r.lines = 6;
        r.mirror = true;
        let copies = r.transforms();
        assert_eq!(copies.len(), 6);
        for t in copies {
            let m = t.matrix;
            assert!((m[0][0] * m[1][1] - m[0][1] * m[1][0]).abs() > 0.999);
        }
    }
    #[test]
    fn textured_symmetry_and_eraser_are_pixel_mirrored_and_one_undo_step() {
        let mut doc = Document::new(128, 128, false);
        doc.layers[0]
            .rulers
            .push(Ruler::new(Kind::Symmetry, 128, 128));
        let mut history = History::new();
        let mut image = image::RgbaImage::new(16, 8);
        for y in 0..8 {
            for x in 0..16 {
                if x < y + 4 {
                    image.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
                }
            }
        }
        let mut b = brush();
        b.size = 24.0;
        b.tip = Some(std::sync::Arc::new(
            crate::brush_tip::BrushTip::from_rgba(image).unwrap(),
        ));
        history.push_stroke(&mut doc);
        doc.begin_ruler_stroke(true, [28.5, 32.5]);
        doc.stamp([28.5, 32.5], b.clone(), false);
        history.commit(&doc);
        assert_eq!(history.counts(), (1, 0));
        assert!(doc.layers[0].pixels.iter().any(|p| *p != 0));
        for y in 0..128 {
            for x in 0..64 {
                let a = doc.layers[0].pixels[(y * 128 + x) * 4 + 3];
                let b = doc.layers[0].pixels[(y * 128 + 127 - x) * 4 + 3];
                assert!(a.abs_diff(b) <= 1, "{x},{y}: {a} vs {b}");
            }
        }
        let painted = doc.layers[0].pixels.clone();
        assert!(history.undo(&mut doc));
        assert!(doc.layers[0].pixels.iter().all(|p| *p == 0));
        assert!(history.redo(&mut doc));
        assert_eq!(doc.layers[0].pixels, painted);
        history.push_stroke(&mut doc);
        doc.begin_ruler_stroke(true, [28.5, 32.5]);
        doc.stamp([28.5, 32.5], b, true);
        history.commit(&doc);
        assert_ne!(doc.layers[0].pixels, painted);
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[0].pixels, painted);
    }
    #[test]
    fn scope_and_snap_toggle_choose_only_applicable_rulers() {
        let mut doc = Document::new(128, 128, false);
        doc.layers[0]
            .rulers
            .push(Ruler::new(Kind::Symmetry, 128, 128));
        doc.add_layer();
        assert!(doc.drawing_ruler(true).is_none());
        doc.layers[0].rulers[0].scope = Scope::All;
        assert!(doc.drawing_ruler(true).is_some());
        doc.begin_ruler_stroke(false, [0.0, 0.0]);
        assert!(doc.ruler_copies.is_empty());
        doc.begin_ruler_stroke(true, [0.0, 0.0]);
        assert_eq!(doc.ruler_copies.len(), 2);
        doc.layers[0].rulers[0].scope = Scope::Folder;
        doc.group_layers(&[0, 1]);
        doc.active = 1;
        assert!(doc.drawing_ruler(true).is_some());
        doc.layers[0].rulers[0].visible = false;
        assert!(doc.drawing_ruler(true).is_none());
    }
    #[test]
    fn perspective_locks_direction_resets_at_start_and_honors_disabled_vp() {
        let mut doc = Document::new(128, 128, false);
        let mut r = Ruler::new(Kind::Perspective3, 128, 128);
        r.vanishing = [[120.0, 20.0], [0.0, 0.0], [20.0, 120.0]];
        r.directions = [true, false, true];
        doc.layers[0].rulers.push(r);
        let mut g = doc.begin_ruler_stroke(true, [20.0, 20.0]).unwrap();
        near(g.snap([40.0, 21.0]), [40.0, 20.0]);
        near(g.snap([40.0, 80.0]), [40.0, 20.0]);
        near(g.snap([20.0, 20.0]), [20.0, 20.0]);
        near(g.snap([21.0, 60.0]), [20.0, 60.0]);
        g.snap([20.0, 20.0]);
        let p = g.snap([5.0, 4.0]);
        assert!(p[0] == 20.0 || p[1] == 20.0);
    }
    #[test]
    fn guides_project_without_jitter_and_ruler_edits_undo_without_pixels() {
        let mut doc = Document::new(128, 128, false);
        let mut r = Ruler::new(Kind::Parallel, 128, 128);
        r.angle = 0.0;
        doc.layers[0].rulers.push(r);
        let mut g = doc.begin_ruler_stroke(true, [10.0, 15.0]).unwrap();
        near(g.snap([40.0, 75.0]), [40.0, 15.0]);
        let mut history = History::new();
        history.push_layer_properties(&doc);
        doc.layers[0].rulers[0].origin = [13.0, 21.0];
        history.commit(&doc);
        assert!(history.undo(&mut doc));
        near(doc.layers[0].rulers[0].origin, [64.0, 64.0]);
        assert!(history.redo(&mut doc));
        near(doc.layers[0].rulers[0].origin, [13.0, 21.0]);
        assert!(doc.layers[0].pixels.iter().all(|p| *p == 0));
    }
}
