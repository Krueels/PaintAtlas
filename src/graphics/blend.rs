//! Layer blending in straight sRGB, with source-over alpha composition.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    Difference,
    Exclusion,
    Add,
    Subtract,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
impl BlendMode {
    pub const ALL: [Self; 18] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::SoftLight,
        Self::HardLight,
        Self::Darken,
        Self::Lighten,
        Self::ColorDodge,
        Self::ColorBurn,
        Self::Difference,
        Self::Exclusion,
        Self::Add,
        Self::Subtract,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Overlay => "Overlay",
            Self::SoftLight => "Soft Light",
            Self::HardLight => "Hard Light",
            Self::Darken => "Darken",
            Self::Lighten => "Lighten",
            Self::ColorDodge => "Color Burn",
            Self::ColorBurn => "Color Dodge",
            Self::Difference => "Difference",
            Self::Exclusion => "Exclusion",
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Hue => "Hue",
            Self::Saturation => "Saturation",
            Self::Color => "Color",
            Self::Luminosity => "Luminosity",
        }
    }
    fn channel(self, b: f32, s: f32) -> f32 {
        match self {
            Self::Multiply => b * s,
            Self::Screen => b + s - b * s,
            Self::Overlay => {
                if b <= 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - s)
                }
            }
            Self::HardLight => {
                if s <= 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - s)
                }
            }
            Self::SoftLight => {
                if s <= 0.5 {
                    b - (1.0 - 2.0 * s) * b * (1.0 - b)
                } else {
                    let d = if b <= 0.25 {
                        ((16.0 * b - 12.0) * b + 4.0) * b
                    } else {
                        b.sqrt()
                    };
                    b + (2.0 * s - 1.0) * (d - b)
                }
            }
            Self::Darken => b.min(s),
            Self::Lighten => b.max(s),
            Self::ColorDodge => {
                if b == 0.0 {
                    0.0
                } else if s == 1.0 {
                    1.0
                } else {
                    (b / (1.0 - s)).min(1.0)
                }
            }
            Self::ColorBurn => {
                if b == 1.0 {
                    1.0
                } else if s == 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - b) / s).min(1.0)
                }
            }
            Self::Difference => (b - s).abs(),
            Self::Exclusion => b + s - 2.0 * b * s,
            Self::Add => (b + s).min(1.0),
            Self::Subtract => (b - s).max(0.0),
            _ => s,
        }
    }
    fn rgb(self, b: [f32; 3], s: [f32; 3]) -> [f32; 3] {
        match self {
            Self::Hue => set_lum(set_sat(s, sat(b)), lum(b)),
            Self::Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
            Self::Color => set_lum(s, lum(b)),
            Self::Luminosity => set_lum(b, lum(s)),
            _ => std::array::from_fn(|i| self.channel(b[i], s[i])),
        }
    }
    pub fn composite(self, dst: &mut [u8], src: [u8; 4], opacity: f32) {
        if self == Self::Normal {
            crate::document::blend(dst, src, opacity);
            return;
        }
        let sa = src[3] as f32 / 255.0 * opacity;
        let ba = dst[3] as f32 / 255.0;
        if sa <= 0.0 {
            return;
        }
        let alpha = sa + ba * (1.0 - sa);
        let b = std::array::from_fn(|i| dst[i] as f32 / 255.0);
        let s = std::array::from_fn(|i| src[i] as f32 / 255.0);
        let mixed = self.rgb(b, s);
        for i in 0..3 {
            let c = (1.0 - sa) * ba * b[i] + (1.0 - ba) * sa * s[i] + sa * ba * mixed[i];
            dst[i] = (c / alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        dst[3] = (alpha * 255.0).round() as u8;
    }
}
fn lum(c: [f32; 3]) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}
fn sat(c: [f32; 3]) -> f32 {
    c.into_iter().fold(f32::NEG_INFINITY, f32::max) - c.into_iter().fold(f32::INFINITY, f32::min)
}
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let min = c.into_iter().fold(f32::INFINITY, f32::min);
    let range = sat(c);
    if range <= 0.0 {
        [0.0; 3]
    } else {
        c.map(|v| (v - min) * s / range)
    }
}
fn set_lum(c: [f32; 3], target: f32) -> [f32; 3] {
    let delta = target - lum(c);
    let mut c = c.map(|v| v + delta);
    let l = lum(c);
    let min = c.into_iter().fold(f32::INFINITY, f32::min);
    let max = c.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if min < 0.0 {
        c = c.map(|v| l + (v - l) * l / (l - min));
    }
    if max > 1.0 {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (max - l));
    }
    c.map(|v| v.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_reference_colors() {
        for (mode, expected) in [
            (BlendMode::Normal, [64, 192, 128, 255]),
            (BlendMode::Multiply, [32, 48, 96, 255]),
            (BlendMode::Screen, [160, 208, 224, 255]),
            (BlendMode::Overlay, [65, 96, 192, 255]),
            (BlendMode::Darken, [64, 64, 128, 255]),
            (BlendMode::Lighten, [128, 192, 192, 255]),
            (BlendMode::Difference, [64, 128, 64, 255]),
            (BlendMode::Exclusion, [128, 160, 127, 255]),
            (BlendMode::Add, [192, 255, 255, 255]),
            (BlendMode::Subtract, [64, 0, 64, 255]),
        ] {
            let mut pixel = [128, 64, 192, 255];
            mode.composite(&mut pixel, [64, 192, 128, 255], 1.0);
            assert_eq!(pixel, expected, "{mode:?}");
        }
    }
    #[test]
    fn every_mode_respects_alpha_and_zero_opacity_on_transparent_backdrop() {
        for mode in BlendMode::ALL {
            let mut pixel = [0; 4];
            mode.composite(&mut pixel, [120, 80, 200, 128], 0.5);
            assert_eq!(pixel, [120, 80, 200, 64], "{mode:?}");
            let original = [20, 80, 150, 128];
            pixel = original;
            mode.composite(&mut pixel, [240, 30, 10, 200], 0.0);
            assert_eq!(pixel, original);
            mode.composite(&mut pixel, [255, 255, 255, 0], 1.0);
            assert_eq!(pixel, original);
        }
    }
    #[test]
    fn color_and_luminosity_preserve_the_correct_luminance() {
        let b = [0.2, 0.5, 0.7];
        let s = [0.9, 0.1, 0.3];
        for mode in [BlendMode::Hue, BlendMode::Saturation, BlendMode::Color] {
            let color = mode.rgb(b, s);
            assert!((lum(color) - lum(b)).abs() < 0.0001);
            assert!(
                color
                    .iter()
                    .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
            );
        }
        assert!((lum(BlendMode::Luminosity.rgb(b, s)) - lum(s)).abs() < 0.0001);
        assert_eq!(BlendMode::SoftLight.rgb(b, [0.5; 3]), b);
    }
    #[test]
    fn layer_blending_survives_project_export_clipboard_and_history() {
        let mut doc = crate::document::Document::with_paper(4, 4, Some([128, 64, 192]), 300);
        doc.layers[1].pixels = [64, 192, 128, 255].repeat(16);
        let mut history = crate::history::History::new();
        history.push(&doc);
        doc.layers[1].blend_mode = BlendMode::Multiply;
        history.commit(&doc);
        assert_eq!(&doc.composite()[..4], &[32, 48, 96, 255]);
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("blend.patlas");
        let png = dir.path().join("blend.png");
        doc.save(&project).unwrap();
        doc.export(&png).unwrap();
        let loaded = crate::document::Document::open(&project).unwrap();
        assert_eq!(loaded.layers[1].blend_mode, BlendMode::Multiply);
        assert_eq!(loaded.composite(), doc.composite());
        assert_eq!(
            crate::document::Document::import(&png).unwrap().composite(),
            doc.composite()
        );
        assert_eq!(
            crate::clipboard::capture(&doc, None, true).unwrap().pixels,
            doc.composite()
        );
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[1].blend_mode, BlendMode::Normal);
        assert!(history.redo(&mut doc));
        assert_eq!(doc.layers[1].blend_mode, BlendMode::Multiply);
    }
}
