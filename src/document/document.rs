use bincode::Options;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::Path,
};

pub const PAINT_TILE: usize = 128;
pub type StrokeTiles = std::collections::BTreeMap<(usize, usize), Vec<u8>>;

pub const MAX_SIDE: u32 = 4096;
const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAGIC: &[u8] = b"PAINTATLAS\x08";
const MAGIC_V7: &[u8] = b"PAINTATLAS\x07";
const MAGIC_V6: &[u8] = b"PAINTATLAS\x06";
const MAGIC_V5: &[u8] = b"PAINTATLAS\x05";
const MAGIC_V4: &[u8] = b"PAINTATLAS\x04";
const MAGIC_V3: &[u8] = b"PAINTATLAS\x03";
const MAGIC_V2: &[u8] = b"PAINTATLAS\x02";
pub const DEFAULT_DPI: u32 = 300;
pub const MAX_DPI: u32 = 9600;
const MAGIC_V1: &[u8] = b"PAINTATLAS\x01";

#[derive(Clone, Serialize, Deserialize)]
pub struct Layer {
    pub rulers: Vec<crate::rulers::Ruler>,
    pub effects: std::sync::Arc<Vec<crate::layer_groups::GroupEffect>>,
    pub group: Option<u64>,
    pub parent: Option<u64>,
    pub label_color: Option<[u8; 3]>,
    pub blend_mode: crate::blend::BlendMode,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub pixels: Vec<u8>,
    pub alpha_lock: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    pub layers: Vec<Layer>,
    pub active: usize,
    #[serde(skip)]
    pub selection: Option<std::sync::Arc<crate::selection::Mask>>,
    #[serde(skip)]
    pub damage: Option<[usize; 4]>,
    #[serde(skip)]
    pub dirty_tiles: std::collections::BTreeSet<(usize, usize)>,
    #[serde(skip)]
    pub ruler_copies: Vec<crate::rulers::Transform>,
    #[serde(skip)]
    pub stroke_tiles: std::cell::RefCell<Option<StrokeTiles>>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Brush,
    Eraser,
    Fill,
    Picker,
    Line,
    Rectangle,
    Ellipse,
    Hand,
    Zoom,
    Ruler,
    SelectRect,
    Lasso,
}
impl Tool {
    pub fn localized_name(self, language: crate::i18n::Language) -> &'static str {
        let key = match self {
            Self::Brush => "tool.brush",
            Self::Eraser => "tool.eraser",
            Self::Fill => "tool.fill",
            Self::Picker => "tool.picker",
            Self::Line => "tool.line",
            Self::Rectangle => "tool.rectangle",
            Self::Ellipse => "tool.ellipse",
            Self::Hand => "tool.hand",
            Self::Zoom => "tool.zoom",
            Self::Ruler => "tool.ruler",
            Self::SelectRect => "tool.selection",
            Self::Lasso => "tool.lasso",
        };
        crate::i18n::text(language, key)
    }
    pub fn is_shape(self) -> bool {
        matches!(self, Self::Line | Self::Rectangle | Self::Ellipse)
    }
}

#[derive(Clone)]
pub struct Brush {
    pub color: [u8; 4],
    pub size: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub tip: Option<std::sync::Arc<crate::brush_tip::BrushTip>>,
    pub spacing: f32,
}

/// Source-over for straight (unpremultiplied) RGBA pixels.
pub fn blend(dst: &mut [u8], src: [u8; 4], opacity: f32) {
    let sa = src[3] as f32 / 255.0 * opacity;
    let da = dst[3] as f32 / 255.0;
    let a = sa + da * (1.0 - sa);
    if a <= 0.0 {
        return;
    }
    for c in 0..3 {
        dst[c] = ((src[c] as f32 * sa + dst[c] as f32 * da * (1.0 - sa)) / a).round() as u8;
    }
    dst[3] = (a * 255.0).round() as u8;
}

fn zeroed_pixels(size: usize) -> Result<Vec<u8>, String> {
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(size)
        .map_err(|error| format!("Unable to allocate canvas pixels: {error}"))?;
    pixels.resize(size, 0);
    Ok(pixels)
}

impl Document {
    pub fn new(width: u32, height: u32, white: bool) -> Self {
        Self::try_new(width, height, white)
            .expect("document dimensions must be valid and memory available")
    }
    pub fn try_new(width: u32, height: u32, white: bool) -> Result<Self, String> {
        Self::try_with_paper(width, height, white.then_some([255; 3]), DEFAULT_DPI)
    }
    pub fn try_from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, String> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(format!(
                "Dimensions must be between 1 and {MAX_SIDE} pixels."
            ));
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|count| count.checked_mul(4))
            .ok_or_else(|| "Image dimensions are too large.".to_owned())?;
        if pixels.len() != expected || expected > MAX_BYTES {
            return Err(
                "Image data does not match the document dimensions or memory limit.".into(),
            );
        }
        Ok(Self {
            width,
            height,
            dpi: DEFAULT_DPI,
            layers: vec![Layer {
                rulers: Default::default(),
                effects: Default::default(),
                group: None,
                parent: None,
                label_color: None,
                blend_mode: Default::default(),
                name: "Camada 1".into(),
                visible: true,
                opacity: 1.0,
                pixels,
                alpha_lock: false,
            }],
            active: 0,
            selection: None,
            damage: None,
            dirty_tiles: Default::default(),
            ruler_copies: Default::default(),
            stroke_tiles: Default::default(),
        })
    }
    pub fn with_paper(width: u32, height: u32, paper: Option<[u8; 3]>, dpi: u32) -> Self {
        Self::try_with_paper(width, height, paper, dpi)
            .expect("document dimensions must be valid and memory available")
    }
    pub fn try_with_paper(
        width: u32,
        height: u32,
        paper: Option<[u8; 3]>,
        dpi: u32,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(format!(
                "Dimensions must be between 1 and {MAX_SIDE} pixels."
            ));
        }
        if !(1..=MAX_DPI).contains(&dpi) {
            return Err(format!("DPI must be between 1 and {MAX_DPI}."));
        }
        let size = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "Canvas size is too large.".to_owned())?;
        let layer_count = if paper.is_some() { 2 } else { 1 };
        if size
            .checked_mul(layer_count)
            .is_none_or(|total| total > MAX_BYTES)
        {
            return Err("Canvas exceeds the document memory limit.".into());
        }
        let mut layers = Vec::new();
        if let Some(color) = paper {
            let mut pixels = zeroed_pixels(size)?;
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.copy_from_slice(&[color[0], color[1], color[2], 255]);
            }
            layers.push(Layer {
                rulers: Default::default(),
                effects: Default::default(),
                group: None,
                parent: None,
                label_color: None,
                blend_mode: Default::default(),
                name: "Papel".into(),
                visible: true,
                opacity: 1.0,
                pixels,
                alpha_lock: false,
            });
        }
        layers.push(Layer {
            rulers: Default::default(),
            effects: Default::default(),
            group: None,
            parent: None,
            label_color: None,
            blend_mode: Default::default(),
            name: "Camada 1".into(),
            visible: true,
            opacity: 1.0,
            pixels: zeroed_pixels(size)?,
            alpha_lock: false,
        });
        Ok(Self {
            width,
            height,
            dpi,
            active: layers.len() - 1,
            selection: None,
            damage: None,
            dirty_tiles: Default::default(),
            ruler_copies: Default::default(),
            stroke_tiles: Default::default(),
            layers,
        })
    }
    pub fn bytes(&self) -> usize {
        self.layers
            .iter()
            .map(|l| {
                l.effects.iter().fold(
                    l.pixels.len().saturating_add(
                        l.rulers
                            .len()
                            .saturating_mul(std::mem::size_of::<crate::rulers::Ruler>()),
                    ),
                    |bytes, effect| bytes.saturating_add(effect.bytes()),
                )
            })
            .fold(0usize, usize::saturating_add)
    }
    /// CPU bytes retained by document-owned pixels and an in-progress stroke.
    /// GPU textures, previews, history, and worker buffers are accounted separately.
    pub fn memory_bytes(&self) -> usize {
        let stroke_bytes = self
            .stroke_tiles
            .borrow()
            .as_ref()
            .map(|tiles| {
                tiles
                    .values()
                    .fold(0usize, |total, pixels| total.saturating_add(pixels.len()))
            })
            .unwrap_or(0);
        self.bytes().saturating_add(stroke_bytes)
    }
    pub fn can_add(&self) -> bool {
        self.layers.len() < 32
            && (self.width as usize)
                .checked_mul(self.height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .and_then(|layer_bytes| self.bytes().checked_add(layer_bytes))
                .is_some_and(|total| total <= MAX_BYTES)
    }
    pub fn add_layer(&mut self) {
        if !self.can_add() {
            return;
        }
        let Some(size) = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
        else {
            return;
        };
        let Ok(pixels) = zeroed_pixels(size) else {
            return;
        };
        if self.layers.try_reserve(1).is_err() {
            return;
        }
        let parent = self.layers[self.active]
            .group
            .or(self.layers[self.active].parent);
        if self.layers[self.active].group.is_none() {
            self.active += 1;
        }
        self.layers.insert(
            self.active,
            Layer {
                rulers: Default::default(),
                effects: Default::default(),
                group: None,
                parent,
                label_color: None,
                blend_mode: Default::default(),
                name: format!("Camada {}", self.layers.len()),
                visible: true,
                opacity: 1.0,
                pixels,
                alpha_lock: false,
            },
        );
    }
    pub fn composite(&self) -> Vec<u8> {
        self.try_composite()
            .expect("document composite allocation failed")
    }
    pub fn try_composite(&self) -> Result<Vec<u8>, String> {
        let size = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "Composite dimensions are too large.".to_owned())?;
        let mut out = Vec::new();
        out.try_reserve_exact(size)
            .map_err(|error| format!("Unable to allocate document composite: {error}"))?;
        out.resize(size, 0);
        for l in &self.layers {
            if !l.visible || l.parent.is_some() {
                continue;
            }
            for (dst, src) in out
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(l.pixels.as_chunks::<4>().0.iter())
            {
                if src[3] > 0 {
                    l.blend_mode.composite(dst, *src, l.opacity);
                }
            }
        }
        Ok(out)
    }
    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= 0.0 && p[1] >= 0.0 && p[0] < self.width as f32 && p[1] < self.height as f32
    }
    pub fn capture_stroke_tiles(&self) -> StrokeTiles {
        let mut result = StrokeTiles::new();
        if let Some(tiles) = self.stroke_tiles.borrow().as_ref() {
            for &(ty, tx) in tiles.keys() {
                let (x, y) = (tx * PAINT_TILE, ty * PAINT_TILE);
                let w = PAINT_TILE.min(self.width as usize - x);
                let h = PAINT_TILE.min(self.height as usize - y);
                let mut bytes = Vec::with_capacity(w * h * 4);
                for row in 0..h {
                    let start = ((y + row) * self.width as usize + x) * 4;
                    bytes.extend_from_slice(&self.layers[self.active].pixels[start..start + w * 4]);
                }
                result.insert((ty, tx), bytes);
            }
        }
        result
    }
    pub fn write_stroke_tiles(&mut self, tiles: &StrokeTiles) {
        for (&(ty, tx), bytes) in tiles {
            let (x, y) = (tx * PAINT_TILE, ty * PAINT_TILE);
            let w = PAINT_TILE.min(self.width as usize - x);
            let h = PAINT_TILE.min(self.height as usize - y);
            for row in 0..h {
                let start = ((y + row) * self.width as usize + x) * 4;
                self.layers[self.active].pixels[start..start + w * 4]
                    .copy_from_slice(&bytes[row * w * 4..(row + 1) * w * 4]);
            }
        }
    }
    /// Restore the current stroke's cached tiles without ending its undo transaction.
    pub fn restore_stroke_tiles(&mut self) {
        if let Some(tiles) = self.stroke_tiles.get_mut() {
            for (&(ty, tx), bytes) in tiles.iter() {
                let (x, y) = (tx * PAINT_TILE, ty * PAINT_TILE);
                let w = PAINT_TILE.min(self.width as usize - x);
                let h = PAINT_TILE.min(self.height as usize - y);
                for row in 0..h {
                    let start = ((y + row) * self.width as usize + x) * 4;
                    self.layers[self.active].pixels[start..start + w * 4]
                        .copy_from_slice(&bytes[row * w * 4..(row + 1) * w * 4]);
                }
                self.dirty_tiles.insert((ty, tx));
                let r = [x, y, x + w, y + h];
                self.damage = Some(self.damage.map_or(r, |d| {
                    [
                        d[0].min(r[0]),
                        d[1].min(r[1]),
                        d[2].max(r[2]),
                        d[3].max(r[3]),
                    ]
                }));
            }
        }
    }
    pub fn stamp(&mut self, p: [f32; 2], brush: Brush, erase: bool) {
        if self.ruler_copies.is_empty() {
            self.stamp_oriented(p, brush, erase, crate::rulers::Transform::IDENTITY);
        } else {
            let mut centers = [[f32::INFINITY; 2]; 16];
            for i in 0..self.ruler_copies.len() {
                let transform = self.ruler_copies[i];
                let center = transform.point(p);
                if centers[..i]
                    .iter()
                    .any(|c| (c[0] - center[0]).hypot(c[1] - center[1]) < 0.001)
                {
                    continue;
                }
                centers[i] = center;
                self.stamp_oriented(center, brush.clone(), erase, transform);
            }
        }
    }
    fn stamp_oriented(
        &mut self,
        p: [f32; 2],
        brush: Brush,
        erase: bool,
        transform: crate::rulers::Transform,
    ) {
        let radius = (brush.size * 0.5).max(0.5);
        let proportions = brush
            .tip
            .as_ref()
            .map_or([1.0, 1.0], |tip| tip.proportions());
        let half = [radius * proportions[0], radius * proportions[1]];
        let mask = brush.tip.as_ref().map(|tip| tip.mask(brush.size));
        let m = transform.matrix;
        let extent = if mask.is_some() {
            [
                m[0][0].abs() * half[0] + m[0][1].abs() * half[1],
                m[1][0].abs() * half[0] + m[1][1].abs() * half[1],
            ]
        } else {
            half
        };
        let x0 = ((p[0] - extent[0] - 1.0).floor() as i32).max(0);
        let y0 = ((p[1] - extent[1] - 1.0).floor() as i32).max(0);
        let x1 = ((p[0] + extent[0] + 1.0).ceil() as i32).min(self.width as i32);
        let y1 = ((p[1] + extent[1] + 1.0).ceil() as i32).min(self.height as i32);
        if x1 > x0 && y1 > y0 {
            let r = [x0 as usize, y0 as usize, x1 as usize, y1 as usize];
            if self.damage.is_none() {
                self.dirty_tiles.clear();
            }
            for ty in r[1] / PAINT_TILE..=(r[3] - 1) / PAINT_TILE {
                for tx in r[0] / PAINT_TILE..=(r[2] - 1) / PAINT_TILE {
                    self.dirty_tiles.insert((ty, tx));
                    if let Some(tiles) = self.stroke_tiles.get_mut() {
                        tiles.entry((ty, tx)).or_insert_with(|| {
                            let x = tx * PAINT_TILE;
                            let y = ty * PAINT_TILE;
                            let w = PAINT_TILE.min(self.width as usize - x);
                            let h = PAINT_TILE.min(self.height as usize - y);
                            let mut bytes = Vec::with_capacity(w * h * 4);
                            for row in y..y + h {
                                let start = (row * self.width as usize + x) * 4;
                                bytes.extend_from_slice(
                                    &self.layers[self.active].pixels[start..start + w * 4],
                                );
                            }
                            bytes
                        });
                    }
                }
            }
            self.damage = Some(match self.damage {
                Some(d) => [
                    d[0].min(r[0]),
                    d[1].min(r[1]),
                    d[2].max(r[2]),
                    d[3].max(r[3]),
                ],
                None => r,
            });
        }
        let locked = self.layers[self.active].alpha_lock;
        let pixels = &mut self.layers[self.active].pixels;
        for y in y0..y1 {
            for x in x0..x1 {
                let coverage = if let Some(mask) = mask {
                    let dx = x as f32 + 0.5 - p[0];
                    let dy = y as f32 + 0.5 - p[1];
                    let u = (m[0][0] * dx + m[1][0] * dy) / (half[0] * 2.0) + 0.5;
                    let v = (m[0][1] * dx + m[1][1] * dy) / (half[1] * 2.0) + 0.5;
                    let alpha = mask.sample(u, v);
                    // Preserve the image silhouette; hardness adjusts the soft mask values.
                    alpha * (brush.hardness + (1.0 - brush.hardness) * alpha)
                } else {
                    let distance =
                        ((x as f32 + 0.5 - p[0]).powi(2) + (y as f32 + 0.5 - p[1]).powi(2)).sqrt();
                    let edge = (radius - distance + 0.5).clamp(0.0, 1.0);
                    let softness = if brush.hardness >= 0.99 {
                        1.0
                    } else {
                        ((radius - distance) / (radius * (1.0 - brush.hardness)).max(0.5))
                            .clamp(0.0, 1.0)
                    };
                    edge * softness
                };
                let selection = self
                    .selection
                    .as_ref()
                    .map_or(1.0, |mask| mask.coverage(x as usize, y as usize));
                let alpha = coverage * brush.opacity * selection;
                if alpha <= 0.0 {
                    continue;
                }
                let i = ((y as u32 * self.width + x as u32) * 4) as usize;
                if locked {
                    if !erase {
                        paint_locked(&mut pixels[i..i + 4], brush.color, alpha);
                    }
                } else if erase {
                    pixels[i + 3] = (pixels[i + 3] as f32 * (1.0 - alpha)).round() as u8;
                    if pixels[i + 3] == 0 {
                        pixels[i..i + 3].fill(0);
                    }
                } else {
                    blend(&mut pixels[i..i + 4], brush.color, alpha);
                }
            }
        }
    }
    pub fn segment(&mut self, a: [f32; 2], b: [f32; 2], brush: Brush, erase: bool) {
        self.segment_variable(a, b, brush.size, brush, erase);
    }
    pub fn segment_variable(
        &mut self,
        a: [f32; 2],
        b: [f32; 2],
        start_size: f32,
        brush: Brush,
        erase: bool,
    ) {
        let distance = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let steps = (distance
            / (start_size.min(brush.size) * brush.spacing.clamp(0.05, 2.0)).max(0.5))
        .ceil()
        .max(1.0) as usize;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let mut interpolated = brush.clone();
            interpolated.size = start_size + (brush.size - start_size) * t;
            self.stamp(
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
                interpolated,
                erase,
            );
        }
    }
    /// Preserve texture dab spacing across input packets (tablet polling rate must
    /// not change the density of a stamp brush).
    pub fn stroke_segment(
        &mut self,
        a: [f32; 2],
        b: [f32; 2],
        start_size: f32,
        brush: Brush,
        erase: bool,
        remaining: &mut f32,
    ) {
        if brush.tip.is_none() {
            self.segment_variable(a, b, start_size, brush, erase);
            return;
        }
        let distance = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        if distance <= f32::EPSILON {
            return;
        }
        let mut next = (*remaining).max(0.0);
        while next <= distance {
            let t = next / distance;
            let mut dab = brush.clone();
            dab.size = start_size + (brush.size - start_size) * t;
            let step = (dab.size * brush.spacing.clamp(0.05, 2.0)).max(0.5);
            self.stamp(
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
                dab,
                erase,
            );
            next += step;
        }
        *remaining = next - distance;
    }
    pub fn shape(&mut self, tool: Tool, a: [f32; 2], b: [f32; 2], brush: Brush) {
        if tool != Tool::Ellipse {
            self.stamp(a, brush.clone(), false);
        }
        match tool {
            Tool::Rectangle => {
                let c = [b[0], a[1]];
                let d = [a[0], b[1]];
                for (p, q) in [(a, c), (c, b), (b, d), (d, a)] {
                    self.segment(p, q, brush.clone(), false);
                }
            }
            Tool::Ellipse => {
                let center = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
                let r = [(a[0] - b[0]).abs() * 0.5, (a[1] - b[1]).abs() * 0.5];
                let count = ((r[0] + r[1]) * std::f32::consts::PI / (brush.size * 0.15).max(0.5))
                    .ceil()
                    .clamp(24.0, 32768.0) as usize;
                let mut last = [center[0] + r[0], center[1]];
                for i in 1..=count {
                    let t = i as f32 / count as f32 * std::f32::consts::TAU;
                    let p = [center[0] + r[0] * t.cos(), center[1] + r[1] * t.sin()];
                    self.segment(last, p, brush.clone(), false);
                    last = p;
                }
            }
            _ => self.segment(a, b, brush, false),
        }
    }
    pub fn fill(
        &mut self,
        p: [f32; 2],
        color: [u8; 4],
        opacity: f32,
        tolerance: u8,
    ) -> Result<(), String> {
        if !self.contains(p) {
            return Ok(());
        }
        let start = p[1] as usize * self.width as usize + p[0] as usize;
        let pixel_count = (self.width as usize)
            .checked_mul(self.height as usize)
            .ok_or_else(|| "Canvas dimensions are too large for fill.".to_owned())?;
        if pixel_count > u32::MAX as usize {
            return Err("Canvas has too many pixels for fill.".into());
        }
        let mut visited = Vec::new();
        visited
            .try_reserve_exact(pixel_count)
            .map_err(|error| format!("Unable to allocate fill map: {error}"))?;
        visited.resize(pixel_count, 0u8);
        // Dimensions are capped at 4096², so u32 indices halve worst-case queue
        // storage compared with usize on 64-bit systems.
        let mut queue = VecDeque::<u32>::new();
        queue
            .try_reserve(1)
            .map_err(|error| format!("Unable to allocate fill queue: {error}"))?;
        queue.push_back(start as u32);
        visited[start] = 1;
        let locked = self.layers[self.active].alpha_lock;
        let pixels = &self.layers[self.active].pixels;
        let target: [u8; 4] = pixels[start * 4..start * 4 + 4].try_into().unwrap();
        while let Some(i) = queue.pop_front().map(|index| index as usize) {
            let coverage = self.selection.as_ref().map_or(1.0, |mask| {
                mask.coverage(i % self.width as usize, i / self.width as usize)
            });
            if coverage <= 0.0 {
                continue;
            }
            let pixel = &pixels[i * 4..i * 4 + 4];
            if !pixel
                .iter()
                .zip(target)
                .all(|(&a, b)| a.abs_diff(b) <= tolerance)
            {
                continue;
            }
            visited[i] = 2;
            let x = i % self.width as usize;
            let y = i / self.width as usize;
            for next in [
                if x > 0 { Some(i - 1) } else { None },
                if x + 1 < self.width as usize {
                    Some(i + 1)
                } else {
                    None
                },
                if y > 0 {
                    Some(i - self.width as usize)
                } else {
                    None
                },
                if y + 1 < self.height as usize {
                    Some(i + self.width as usize)
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            {
                if visited[next] == 0 {
                    if queue.len() == queue.capacity() {
                        queue.try_reserve(1).map_err(|error| {
                            format!("Unable to grow fill queue; canvas was not changed: {error}")
                        })?;
                    }
                    visited[next] = 1;
                    queue.push_back(next as u32);
                }
            }
        }
        let pixels = &mut self.layers[self.active].pixels;
        for (index, state) in visited.into_iter().enumerate() {
            if state == 2 {
                let coverage = self.selection.as_ref().map_or(1.0, |mask| {
                    mask.coverage(index % self.width as usize, index / self.width as usize)
                });
                let pixel = &mut pixels[index * 4..index * 4 + 4];
                if locked {
                    paint_locked(pixel, color, opacity * coverage);
                } else {
                    blend(pixel, color, opacity * coverage);
                }
            }
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), String> {
        if !(1..=MAX_DPI).contains(&self.dpi) {
            return Err("DPI inválido (limite: 1 a 9600).".into());
        }
        if self.width == 0 || self.height == 0 || self.width > MAX_SIDE || self.height > MAX_SIDE {
            return Err("Dimensões inválidas (limite: 4096 × 4096).".into());
        }
        if self.layers.is_empty()
            || self.layers.len() > 32
            || self.active >= self.layers.len()
            || self.bytes() > MAX_BYTES
        {
            return Err("Estrutura de camadas inválida ou muito grande.".into());
        }
        let expected = (self.width * self.height * 4) as usize;
        if self.layers.iter().any(|l| {
            l.rulers.len() > 8
                || l.rulers.iter().any(|r| !r.valid())
                || l.pixels.len() != expected
                || !l.opacity.is_finite()
                || !(0.0..=1.0).contains(&l.opacity)
                || l.name.len() > 1024
        }) {
            return Err("Dados de camada inválidos.".into());
        }
        self.validate_groups()?;
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let options = bincode::DefaultOptions::new();
        let payload_len =
            usize::try_from(options.serialized_size(self).map_err(|e| e.to_string())?)
                .map_err(|_| "Serialized project is too large for this platform.".to_owned())?;
        let total_len = MAGIC
            .len()
            .checked_add(payload_len)
            .ok_or_else(|| "Serialized project is too large.".to_owned())?;
        if total_len > MAX_BYTES + 65536 {
            return Err("Serialized project exceeds the 256 MiB limit.".into());
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total_len)
            .map_err(|e| format!("Unable to allocate project serialization buffer: {e}"))?;
        bytes.extend_from_slice(MAGIC);
        options
            .serialize_into(&mut bytes, self)
            .map_err(|e| e.to_string())?;
        atomic_write(path, &bytes)
    }
    pub fn open(path: &Path) -> Result<Self, String> {
        const MAX_PROJECT_FILE: usize = MAX_BYTES + 65536;
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let file_len = file.metadata().map_err(|e| e.to_string())?.len();
        if file_len > MAX_PROJECT_FILE as u64 {
            return Err("File excede o limite de 256 MB.".into());
        }
        let initial_capacity = usize::try_from(file_len)
            .map_err(|_| "Project file is too large for this platform.".to_owned())?;
        let mut data = Vec::new();
        data.try_reserve_exact(initial_capacity)
            .map_err(|e| format!("Unable to allocate project read buffer: {e}"))?;
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let read = file.read(&mut chunk).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            let next_len = data
                .len()
                .checked_add(read)
                .filter(|&len| len <= MAX_PROJECT_FILE)
                .ok_or_else(|| "File excede o limite de 256 MB.".to_owned())?;
            data.try_reserve(read)
                .map_err(|e| format!("Unable to grow project read buffer: {e}"))?;
            data.extend_from_slice(&chunk[..read]);
            debug_assert_eq!(data.len(), next_len);
        }
        let options = bincode::DefaultOptions::new().with_limit(MAX_BYTES as u64 + 65536);
        let mut doc: Self = if data.starts_with(MAGIC) {
            options
                .deserialize(&data[MAGIC.len()..])
                .map_err(|e| e.to_string())?
        } else if data.starts_with(MAGIC_V7) {
            let old: LegacyDocumentV7 = options
                .deserialize(&data[MAGIC_V7.len()..])
                .map_err(|e| e.to_string())?;
            let layers = old
                .layers
                .into_iter()
                .map(|l| Layer {
                    rulers: Vec::new(),
                    effects: l.effects,
                    group: l.group,
                    parent: l.parent,
                    label_color: l.label_color,
                    blend_mode: l.blend_mode,
                    name: l.name,
                    visible: l.visible,
                    opacity: l.opacity,
                    pixels: l.pixels,
                    alpha_lock: l.alpha_lock,
                })
                .collect();
            Self {
                width: old.width,
                height: old.height,
                dpi: old.dpi,
                active: old.active,
                layers,
                selection: None,
                damage: None,
                dirty_tiles: Default::default(),
                ruler_copies: Vec::new(),
                stroke_tiles: Default::default(),
            }
        } else if data.starts_with(MAGIC_V6) {
            let old: LegacyDocumentV6 = options
                .deserialize(&data[MAGIC_V6.len()..])
                .map_err(|e| e.to_string())?;
            Self {
                width: old.width,
                height: old.height,
                dpi: old.dpi,
                active: old.active,
                layers: old
                    .layers
                    .into_iter()
                    .map(|l| Layer {
                        rulers: Default::default(),
                        effects: Default::default(),
                        group: l.group,
                        parent: l.parent,
                        label_color: l.label_color,
                        blend_mode: l.blend_mode,
                        name: l.name,
                        visible: l.visible,
                        opacity: l.opacity,
                        pixels: l.pixels,
                        alpha_lock: l.alpha_lock,
                    })
                    .collect(),
                selection: None,
                damage: None,
                dirty_tiles: Default::default(),
                ruler_copies: Default::default(),
                stroke_tiles: Default::default(),
            }
        } else if data.starts_with(MAGIC_V5) {
            let old: LegacyDocumentV5 = options
                .deserialize(&data[MAGIC_V5.len()..])
                .map_err(|e| e.to_string())?;
            Self {
                width: old.width,
                height: old.height,
                dpi: old.dpi,
                active: old.active,
                layers: old
                    .layers
                    .into_iter()
                    .map(|l| Layer {
                        rulers: Default::default(),
                        effects: Default::default(),
                        group: None,
                        parent: None,
                        label_color: l.label_color,
                        blend_mode: l.blend_mode,
                        name: l.name,
                        visible: l.visible,
                        opacity: l.opacity,
                        pixels: l.pixels,
                        alpha_lock: l.alpha_lock,
                    })
                    .collect(),
                selection: None,
                damage: None,
                dirty_tiles: Default::default(),
                ruler_copies: Default::default(),
                stroke_tiles: Default::default(),
            }
        } else if data.starts_with(MAGIC_V4) {
            let old: LegacyDocumentV4 = options
                .deserialize(&data[MAGIC_V4.len()..])
                .map_err(|e| e.to_string())?;
            Self {
                width: old.width,
                height: old.height,
                dpi: old.dpi,
                layers: old
                    .layers
                    .into_iter()
                    .map(|l| Layer {
                        rulers: Default::default(),
                        effects: Default::default(),
                        group: None,
                        parent: None,
                        label_color: None,
                        blend_mode: l.blend_mode,
                        name: l.name,
                        visible: l.visible,
                        opacity: l.opacity,
                        pixels: l.pixels,
                        alpha_lock: l.alpha_lock,
                    })
                    .collect(),
                active: old.active,
                selection: None,
                damage: None,
                dirty_tiles: Default::default(),
                ruler_copies: Default::default(),
                stroke_tiles: Default::default(),
            }
        } else if data.starts_with(MAGIC_V3) {
            let old: LegacyDocumentV3 = options
                .deserialize(&data[MAGIC_V3.len()..])
                .map_err(|e| e.to_string())?;
            Self {
                width: old.width,
                height: old.height,
                dpi: old.dpi,
                layers: old.layers.into_iter().map(Layer::from).collect(),
                active: old.active,
                selection: None,
                damage: None,
                dirty_tiles: Default::default(),
                ruler_copies: Default::default(),
                stroke_tiles: Default::default(),
            }
        } else if data.starts_with(MAGIC_V2) {
            let old: LegacyDocumentV2 = options
                .deserialize(&data[MAGIC_V2.len()..])
                .map_err(|e| e.to_string())?;
            Self {
                width: old.width,
                height: old.height,
                dpi: DEFAULT_DPI,
                layers: old.layers.into_iter().map(Layer::from).collect(),
                active: old.active,
                selection: None,
                damage: None,
                dirty_tiles: Default::default(),
                ruler_copies: Default::default(),
                stroke_tiles: Default::default(),
            }
        } else if data.starts_with(MAGIC_V1) {
            let old: LegacyDocument = options
                .deserialize(&data[MAGIC_V1.len()..])
                .map_err(|e| e.to_string())?;
            Self {
                width: old.width,
                height: old.height,
                dpi: DEFAULT_DPI,
                active: old.active,
                selection: None,
                damage: None,
                dirty_tiles: Default::default(),
                ruler_copies: Default::default(),
                stroke_tiles: Default::default(),
                layers: old
                    .layers
                    .into_iter()
                    .map(|l| Layer {
                        rulers: Default::default(),
                        effects: Default::default(),
                        group: None,
                        parent: None,
                        label_color: None,
                        blend_mode: Default::default(),
                        name: l.name,
                        visible: l.visible,
                        opacity: l.opacity,
                        pixels: l.pixels,
                        alpha_lock: false,
                    })
                    .collect(),
            }
        } else {
            return Err("Este não é um projeto PaintAtlas válido.".into());
        };
        doc.validate()?;
        doc.try_rebuild_groups_after(None, None)?;
        Ok(doc)
    }
    pub fn import(path: &Path) -> Result<Self, String> {
        let mut reader = image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(MAX_BYTES as u64);
        reader.limits(limits);
        let image = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
        let mut doc = Self::try_from_rgba(image.width(), image.height(), image.into_raw())?;
        doc.layers[0].name = "Image importada".into();
        doc.validate()?;
        Ok(doc)
    }
    pub fn export(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let pixels_per_meter = (self.dpi as f64 / 0.0254).round() as u32;
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: pixels_per_meter,
                yppu: pixels_per_meter,
                unit: png::Unit::Meter,
            }));
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            let composite = self.try_composite()?;
            writer
                .write_image_data(&composite)
                .map_err(|e| e.to_string())?;
            writer.finish().map_err(|e| e.to_string())?;
        }
        atomic_write(path, &bytes)
    }
}
#[derive(Serialize, Deserialize)]
struct LegacyLayerV7 {
    pub effects: std::sync::Arc<Vec<crate::layer_groups::GroupEffect>>,
    pub group: Option<u64>,
    pub parent: Option<u64>,
    pub label_color: Option<[u8; 3]>,
    pub blend_mode: crate::blend::BlendMode,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub pixels: Vec<u8>,
    pub alpha_lock: bool,
}

#[derive(Serialize, Deserialize)]
struct LegacyDocumentV7 {
    width: u32,
    height: u32,
    dpi: u32,
    layers: Vec<LegacyLayerV7>,
    active: usize,
}
#[derive(Serialize, Deserialize)]
struct LegacyLayerV6 {
    group: Option<u64>,
    parent: Option<u64>,
    label_color: Option<[u8; 3]>,
    blend_mode: crate::blend::BlendMode,
    name: String,
    visible: bool,
    opacity: f32,
    pixels: Vec<u8>,
    alpha_lock: bool,
}
#[derive(Serialize, Deserialize)]
struct LegacyDocumentV6 {
    width: u32,
    height: u32,
    dpi: u32,
    layers: Vec<LegacyLayerV6>,
    active: usize,
}
#[derive(Serialize, Deserialize)]
struct LegacyLayerV5 {
    label_color: Option<[u8; 3]>,
    blend_mode: crate::blend::BlendMode,
    name: String,
    visible: bool,
    opacity: f32,
    pixels: Vec<u8>,
    alpha_lock: bool,
}
#[derive(Serialize, Deserialize)]
struct LegacyDocumentV5 {
    width: u32,
    height: u32,
    dpi: u32,
    layers: Vec<LegacyLayerV5>,
    active: usize,
}
#[derive(Serialize, Deserialize)]
struct LegacyLayerV4 {
    blend_mode: crate::blend::BlendMode,
    name: String,
    visible: bool,
    opacity: f32,
    pixels: Vec<u8>,
    alpha_lock: bool,
}
#[derive(Serialize, Deserialize)]
struct LegacyDocumentV4 {
    width: u32,
    height: u32,
    dpi: u32,
    layers: Vec<LegacyLayerV4>,
    active: usize,
}
#[derive(Serialize, Deserialize)]
struct LegacyLayerV3 {
    name: String,
    visible: bool,
    opacity: f32,
    pixels: Vec<u8>,
    alpha_lock: bool,
}
impl From<LegacyLayerV3> for Layer {
    fn from(l: LegacyLayerV3) -> Self {
        Self {
            rulers: Default::default(),
            effects: Default::default(),
            group: None,
            parent: None,
            label_color: None,
            name: l.name,
            visible: l.visible,
            opacity: l.opacity,
            pixels: l.pixels,
            alpha_lock: l.alpha_lock,
            blend_mode: Default::default(),
        }
    }
}
#[cfg(test)]
impl From<&Layer> for LegacyLayerV3 {
    fn from(l: &Layer) -> Self {
        Self {
            name: l.name.clone(),
            visible: l.visible,
            opacity: l.opacity,
            pixels: l.pixels.clone(),
            alpha_lock: l.alpha_lock,
        }
    }
}
#[derive(Serialize, Deserialize)]
struct LegacyDocumentV3 {
    width: u32,
    height: u32,
    dpi: u32,
    layers: Vec<LegacyLayerV3>,
    active: usize,
}
#[derive(Serialize, Deserialize)]
struct LegacyLayer {
    name: String,
    visible: bool,
    opacity: f32,
    pixels: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
struct LegacyDocumentV2 {
    width: u32,
    height: u32,
    layers: Vec<LegacyLayerV3>,
    active: usize,
}
#[derive(Serialize, Deserialize)]
struct LegacyDocument {
    width: u32,
    height: u32,
    layers: Vec<LegacyLayer>,
    active: usize,
}
fn paint_locked(dst: &mut [u8], color: [u8; 4], opacity: f32) {
    if dst[3] == 0 {
        return;
    }
    let alpha = opacity * color[3] as f32 / 255.0;
    for c in 0..3 {
        dst[c] = (dst[c] as f32 * (1.0 - alpha) + color[c] as f32 * alpha).round() as u8;
    }
}
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

pub use crate::history::History;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "manual repeatable 3000x3000 bucket-fill benchmark"]
    fn benchmark_bucket_fill_3000() {
        let mut doc = Document::new(3000, 3000, false);
        let start = std::time::Instant::now();
        doc.fill([0.0, 0.0], [40, 100, 180, 255], 1.0, 0).unwrap();
        println!(
            "bucket fill 3000x3000: {:?}; document={:.2} MiB",
            start.elapsed(),
            doc.memory_bytes() as f64 / (1024.0 * 1024.0)
        );
        assert!(
            doc.layers[0]
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [40, 100, 180, 255])
        );
    }

    #[test]
    fn fallible_document_construction_rejects_bad_dimensions_and_pixel_buffers() {
        assert!(Document::try_new(0, 8, false).is_err());
        assert!(Document::try_new(MAX_SIDE + 1, 8, false).is_err());
        assert!(Document::try_new(u32::MAX, u32::MAX, false).is_err());
        assert!(Document::try_with_paper(8, 8, None, 0).is_err());
        assert!(Document::try_from_rgba(2, 2, vec![0; 15]).is_err());

        let pixels = vec![42; 2 * 3 * 4];
        let doc = Document::try_from_rgba(2, 3, pixels.clone()).unwrap();
        assert_eq!(doc.layers[0].pixels, pixels);
        assert_eq!(doc.bytes(), 2 * 3 * 4);
    }

    #[test]
    fn adding_a_layer_reserves_storage_before_changing_document_metadata() {
        let mut doc = Document::try_new(3, 2, false).unwrap();
        let before_bytes = doc.bytes();
        doc.add_layer();
        assert_eq!(doc.layers.len(), 2);
        assert_eq!(doc.active, 1);
        assert_eq!(doc.bytes(), before_bytes + 3 * 2 * 4);
        assert!(doc.layers[1].pixels.iter().all(|&pixel| pixel == 0));
    }

    fn square_tip_brush() -> Brush {
        Brush {
            color: [10, 20, 30, 255],
            size: 20.0,
            opacity: 1.0,
            hardness: 1.0,
            spacing: 0.8,
            tip: Some(std::sync::Arc::new(
                crate::brush_tip::BrushTip::from_rgba(image::RgbaImage::from_pixel(
                    16,
                    16,
                    image::Rgba([0, 0, 0, 255]),
                ))
                .unwrap(),
            )),
        }
    }
    #[test]
    fn label_colors_roundtrip_and_v4_projects_keep_blend_modes() {
        let mut doc = Document::new(4, 4, true);
        doc.layers[1].label_color = Some([239, 100, 112]);
        doc.layers[1].blend_mode = crate::blend::BlendMode::Multiply;
        doc.layers[1].opacity = 0.4;
        doc.layers[1].alpha_lock = true;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("colors.patlas");
        doc.save(&path).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.layers[1].label_color, Some([239, 100, 112]));
        let old = LegacyDocumentV4 {
            width: doc.width,
            height: doc.height,
            dpi: doc.dpi,
            active: doc.active,
            layers: doc
                .layers
                .iter()
                .map(|layer| LegacyLayerV4 {
                    blend_mode: layer.blend_mode,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    opacity: layer.opacity,
                    pixels: layer.pixels.clone(),
                    alpha_lock: layer.alpha_lock,
                })
                .collect(),
        };
        let mut bytes = MAGIC_V4.to_vec();
        bytes.extend(bincode::DefaultOptions::new().serialize(&old).unwrap());
        std::fs::write(&path, bytes).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.layers[1].label_color, None);
        assert_eq!(loaded.layers[1].blend_mode, doc.layers[1].blend_mode);
        assert_eq!(loaded.layers[1].opacity, 0.4);
        assert!(loaded.layers[1].alpha_lock);
        assert_eq!(loaded.composite(), doc.composite());
    }

    #[test]
    fn v3_projects_migrate_layers_to_normal_preserving_dpi_alpha_lock_and_pixels() {
        let mut doc = Document::with_paper(8, 6, Some([100, 180, 240]), 600);
        doc.layers[0].alpha_lock = true;
        let old = LegacyDocumentV3 {
            width: doc.width,
            height: doc.height,
            dpi: doc.dpi,
            layers: doc.layers.iter().map(LegacyLayerV3::from).collect(),
            active: doc.active,
        };
        let mut bytes = MAGIC_V3.to_vec();
        bytes.extend(bincode::DefaultOptions::new().serialize(&old).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v3.patlas");
        std::fs::write(&path, bytes).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.dpi, 600);
        assert!(loaded.layers[0].alpha_lock);
        assert_eq!(loaded.composite(), doc.composite());
        assert!(
            loaded
                .layers
                .iter()
                .all(|l| l.blend_mode == crate::blend::BlendMode::Normal)
        );
    }
    #[test]
    fn textured_stamp_paints_square_corners_erases_and_respects_alpha_lock() {
        let brush = square_tip_brush();
        let mut doc = Document::new(64, 64, false);
        doc.stamp([32.0, 32.0], brush.clone(), false);
        let corner = (23 * 64 + 23) * 4;
        assert!(
            doc.layers[0].pixels[corner + 3] > 200,
            "texture must not be cropped to a circle"
        );
        assert_eq!(doc.layers[0].pixels[(20 * 64 + 20) * 4 + 3], 0);
        let alpha: Vec<_> = doc.layers[0]
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        doc.layers[0].alpha_lock = true;
        doc.stamp([36.0, 32.0], brush.clone(), false);
        assert!(
            alpha
                .iter()
                .copied()
                .eq(doc.layers[0].pixels.as_chunks::<4>().0.iter().map(|p| p[3]))
        );
        doc.layers[0].alpha_lock = false;
        doc.stamp([32.0, 32.0], brush, true);
        assert!(doc.layers[0].pixels[corner + 3] < 10);
    }
    #[test]
    fn texture_spacing_is_independent_of_tablet_packet_frequency() {
        let brush = square_tip_brush();
        let mut one = Document::new(128, 64, false);
        let mut many = one.clone();
        for doc in [&mut one, &mut many] {
            doc.stamp([10.0, 32.0], brush.clone(), false);
        }
        let mut remaining = brush.size * brush.spacing;
        one.stroke_segment(
            [10.0, 32.0],
            [110.0, 32.0],
            brush.size,
            brush.clone(),
            false,
            &mut remaining,
        );
        let mut remaining = brush.size * brush.spacing;
        for x in 10..110 {
            many.stroke_segment(
                [x as f32, 32.0],
                [(x + 1) as f32, 32.0],
                brush.size,
                brush.clone(),
                false,
                &mut remaining,
            );
        }
        assert_eq!(one.layers[0].pixels, many.layers[0].pixels);
    }
    #[test]
    fn alpha_lock_preserves_every_alpha_for_brush_fill_and_eraser() {
        let mut d = Document::new(16, 16, false);
        d.layers[0].pixels[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4]
            .copy_from_slice(&[255, 0, 0, 128]);
        d.layers[0].alpha_lock = true;
        let alphas: Vec<_> = d.layers[0]
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        let b = Brush {
            color: [0, 0, 255, 255],
            size: 12.0,
            opacity: 1.0,
            hardness: 1.0,
            tip: None,
            spacing: 0.15,
        };
        d.stamp([8.0, 8.0], b.clone(), false);
        assert_eq!(
            &d.layers[0].pixels[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
            &[0, 0, 255, 128]
        );
        d.fill([0.0, 0.0], [0, 255, 0, 255], 1.0, 255).unwrap();
        let after: Vec<_> = d.layers[0]
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        assert_eq!(alphas, after);
        let before = d.layers[0].pixels.clone();
        d.stamp([8.0, 8.0], b.clone(), true);
        assert_eq!(before, d.layers[0].pixels);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("locked.patlas");
        d.save(&p).unwrap();
        assert!(Document::open(&p).unwrap().layers[0].alpha_lock);
    }
    #[test]
    fn ruler_project_roundtrip_and_v7_migration_preserve_artwork() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rulers.patlas");
        let mut doc = Document::new(16, 16, false);
        doc.layers[0].pixels[3] = 211;
        doc.layers[0].rulers.push(crate::rulers::Ruler::new(
            crate::rulers::Kind::Perspective3,
            16,
            16,
        ));
        doc.save(&path).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.layers[0].rulers, doc.layers[0].rulers);
        assert_eq!(loaded.layers[0].pixels, doc.layers[0].pixels);
        let l = &doc.layers[0];
        let old = LegacyDocumentV7 {
            width: 16,
            height: 16,
            dpi: 300,
            active: 0,
            layers: vec![LegacyLayerV7 {
                effects: l.effects.clone(),
                group: l.group,
                parent: l.parent,
                label_color: Some([10, 20, 30]),
                blend_mode: l.blend_mode,
                name: l.name.clone(),
                visible: true,
                opacity: l.opacity,
                pixels: l.pixels.clone(),
                alpha_lock: true,
            }],
        };
        let mut bytes = MAGIC_V7.to_vec();
        bytes.extend(bincode::DefaultOptions::new().serialize(&old).unwrap());
        std::fs::write(&path, bytes).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert!(loaded.layers[0].rulers.is_empty());
        assert_eq!(loaded.layers[0].pixels, doc.layers[0].pixels);
        assert!(loaded.layers[0].alpha_lock);
        assert_eq!(loaded.layers[0].label_color, Some([10, 20, 30]));
        doc.layers[0].rulers[0].origin[0] = f32::NAN;
        assert!(doc.save(&path).is_err());
    }
    #[test]
    fn legacy_projects_migrate_without_losing_pixels() {
        let old = LegacyDocument {
            width: 2,
            height: 2,
            active: 0,
            layers: vec![LegacyLayer {
                name: "Antiga".into(),
                visible: true,
                opacity: 0.7,
                pixels: vec![123; 16],
            }],
        };
        let mut bytes = MAGIC_V1.to_vec();
        bytes.extend(bincode::DefaultOptions::new().serialize(&old).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("legacy.patlas");
        std::fs::write(&p, bytes).unwrap();
        let mut d = Document::open(&p).unwrap();
        assert_eq!(d.layers[0].pixels, vec![123; 16]);
        assert!(!d.layers[0].alpha_lock);
        let mut history = History::new();
        history.push(&d);
        d.layers[0].alpha_lock = true;
        assert!(history.undo(&mut d));
        assert!(!d.layers[0].alpha_lock);
        assert!(history.redo(&mut d));
        assert!(d.layers[0].alpha_lock);
        d.save(&p).unwrap();
        assert!(Document::open(&p).unwrap().layers[0].alpha_lock);
    }
    #[test]
    fn brush_damage_is_local_and_unions_segments() {
        let mut d = Document::new(1280, 800, false);
        let b = Brush {
            color: [0, 0, 0, 255],
            size: 12.0,
            opacity: 1.0,
            hardness: 1.0,
            tip: None,
            spacing: 0.15,
        };
        d.stamp([100.0, 100.0], b.clone(), false);
        d.segment([100.0, 100.0], [120.0, 110.0], b.clone(), false);
        let r = d.damage.unwrap();
        assert!(r[0] <= 94 && r[1] <= 94 && r[2] >= 126 && r[3] >= 116);
        assert!((r[2] - r[0]) * (r[3] - r[1]) < 2000);
    }
    #[test]
    fn transparent_composite_and_eraser() {
        let mut d = Document::new(16, 16, false);
        let b = Brush {
            color: [255, 0, 0, 255],
            size: 8.0,
            opacity: 1.0,
            hardness: 1.0,
            tip: None,
            spacing: 0.15,
        };
        d.stamp([8.0, 8.0], b.clone(), false);
        assert_eq!(
            &d.composite()[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
            &[255, 0, 0, 255]
        );
        d.stamp([8.0, 8.0], b.clone(), true);
        assert_eq!(d.composite()[(8 * 16 + 8) * 4 + 3], 0);
    }
    #[test]
    fn fallible_composite_matches_compatibility_wrapper() {
        let doc = Document::new(3, 2, true);
        assert_eq!(doc.try_composite().unwrap(), doc.composite());
    }
    #[test]
    fn fill_stops_at_boundary_and_undo_restores() {
        let mut d = Document::new(5, 5, false);
        for y in 0..5 {
            d.layers[0].pixels[(y * 5 + 2) * 4..(y * 5 + 2) * 4 + 4]
                .copy_from_slice(&[0, 0, 0, 255]);
        }
        let mut h = History::new();
        h.push(&d);
        d.fill([0.0, 0.0], [255, 0, 0, 255], 1.0, 0).unwrap();
        assert_eq!(&d.layers[0].pixels[0..4], &[255, 0, 0, 255]);
        assert_eq!(&d.layers[0].pixels[16..20], &[0, 0, 0, 0]);
        assert!(h.undo(&mut d));
        assert_eq!(d.layers[0].pixels[3], 0);
        assert!(h.redo(&mut d));
        assert_eq!(d.layers[0].pixels[3], 255);
    }
    #[test]
    fn colored_paper_and_dpi_survive_project_and_png_export() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paper.patlas");
        let doc = Document::with_paper(8, 4, Some([235, 220, 190]), 600);
        assert_eq!(doc.active, 1);
        assert!(doc.layers[1].pixels.iter().all(|&channel| channel == 0));
        assert!(
            doc.composite()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [235, 220, 190, 255])
        );
        doc.save(&path).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.dpi, 600);
        assert_eq!(loaded.composite(), doc.composite());
        let png_path = dir.path().join("paper.png");
        loaded.export(&png_path).unwrap();
        let file = std::io::BufReader::new(std::fs::File::open(&png_path).unwrap());
        let reader = png::Decoder::new(file).read_info().unwrap();
        let dimensions = reader.info().pixel_dims.unwrap();
        assert_eq!(dimensions.unit, png::Unit::Meter);
        assert_eq!(dimensions.xppu, (600.0_f64 / 0.0254).round() as u32);
        assert_eq!(dimensions.xppu, dimensions.yppu);
        assert_eq!(
            Document::import(&png_path).unwrap().composite(),
            doc.composite()
        );
        let transparent = Document::with_paper(8, 4, None, 72);
        assert_eq!(transparent.layers.len(), 1);
        assert!(transparent.composite().iter().all(|&channel| channel == 0));
    }

    #[test]
    fn v2_projects_receive_default_dpi_without_losing_layers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v2.patlas");
        let mut doc = Document::new(4, 4, true);
        doc.layers[0].alpha_lock = true;
        let old = LegacyDocumentV2 {
            width: doc.width,
            height: doc.height,
            layers: doc.layers.iter().map(LegacyLayerV3::from).collect(),
            active: doc.active,
        };
        let mut bytes = MAGIC_V2.to_vec();
        bytes.extend(bincode::DefaultOptions::new().serialize(&old).unwrap());
        std::fs::write(&path, bytes).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.dpi, DEFAULT_DPI);
        assert!(loaded.layers[0].alpha_lock);
        assert_eq!(loaded.composite(), doc.composite());
        assert_eq!(loaded.active, doc.active);
    }

    #[test]
    fn save_roundtrip_and_png_export() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.patlas");
        let mut d = Document::new(4, 4, true);
        d.layers[1].opacity = 0.5;
        d.layers[1].pixels[0..4].copy_from_slice(&[0, 0, 0, 255]);
        d.save(&path).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.layers.len(), 2);
        assert_eq!(loaded.layers[1].opacity, 0.5);
        assert_eq!(loaded.composite(), d.composite());
        let png = dir.path().join("test.png");
        d.export(&png).unwrap();
        assert_eq!(Document::import(&png).unwrap().composite(), d.composite());
        std::fs::write(&path, b"broken").unwrap();
        assert!(Document::open(&path).is_err());
    }
    #[test]
    fn failed_save_keeps_existing_project_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.patlas");
        let original = Document::new(3, 2, false);
        original.save(&path).unwrap();
        let known_good = std::fs::read(&path).unwrap();

        let mut invalid = original.clone();
        invalid.layers[0].pixels.pop();
        assert!(invalid.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), known_good);
        assert_eq!(
            Document::open(&path).unwrap().layers[0].pixels.len(),
            3 * 2 * 4
        );
    }
    #[test]
    fn saving_overwrites_a_valid_project_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.patlas");
        let first = Document::new(3, 2, false);
        first.save(&path).unwrap();
        let mut replacement = Document::new(5, 4, true);
        replacement.layers[1].pixels[0..4].copy_from_slice(&[12, 34, 56, 255]);
        replacement.save(&path).unwrap();

        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.width, replacement.width);
        assert_eq!(loaded.height, replacement.height);
        assert_eq!(loaded.composite(), replacement.composite());
    }
    #[test]
    fn failed_atomic_replace_leaves_destination_and_cleans_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("destination");
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("keep.txt"), b"known-good").unwrap();

        assert!(atomic_write(&destination, b"replacement").is_err());
        assert_eq!(
            std::fs::read(destination.join("keep.txt")).unwrap(),
            b"known-good"
        );
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(
            entries.len(),
            1,
            "failed atomic write should remove its temp file"
        );
    }
    #[test]
    fn truncated_project_files_are_rejected_at_every_format_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let valid_path = dir.path().join("valid.patlas");
        Document::new(5, 3, true).save(&valid_path).unwrap();
        let valid_bytes = std::fs::read(&valid_path).unwrap();
        let truncation_points = [0, MAGIC.len() - 1, MAGIC.len(), valid_bytes.len() - 1];

        for (index, length) in truncation_points.into_iter().enumerate() {
            let path = dir.path().join(format!("truncated-{index}.patlas"));
            std::fs::write(&path, &valid_bytes[..length]).unwrap();
            assert!(
                Document::open(&path).is_err(),
                "accepted truncation at {length}"
            );
        }
    }
    #[test]
    fn oversized_project_is_rejected_before_reading_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oversized.patlas");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len((MAX_BYTES + 65537) as u64).unwrap();
        let error = match Document::open(&path) {
            Ok(_) => panic!("oversized project unexpectedly loaded"),
            Err(error) => error,
        };
        assert!(error.contains("256 MB"));
    }
    #[test]
    fn ellipse_does_not_paint_its_bounding_corner() {
        let mut d = Document::new(64, 64, false);
        let b = Brush {
            color: [0, 0, 0, 255],
            size: 2.0,
            opacity: 1.0,
            hardness: 1.0,
            tip: None,
            spacing: 0.15,
        };
        d.shape(Tool::Ellipse, [10.0, 10.0], [54.0, 54.0], b);
        assert_eq!(d.layers[0].pixels[(10 * 64 + 10) * 4 + 3], 0);
        assert!(d.layers[0].pixels[(32 * 64 + 53) * 4 + 3] > 0);
    }
    #[test]
    fn malformed_project_is_rejected_and_edit_discards_redo() {
        let mut d = Document::new(8, 8, false);
        let mut h = History::new();
        h.push(&d);
        d.layers[0].pixels[3] = 255;
        h.undo(&mut d);
        h.push(&d);
        d.layers[0].pixels[7] = 255;
        assert!(!h.redo(&mut d));
        d.layers[0].pixels.pop();
        let dir = tempfile::tempdir().unwrap();
        assert!(d.save(&dir.path().join("bad.patlas")).is_err());
    }
    #[test]
    fn opacity_and_visibility() {
        let mut d = Document::new(1, 1, true);
        d.layers[1].pixels = vec![255, 0, 0, 255];
        d.layers[1].opacity = 0.5;
        assert_eq!(d.composite(), vec![255, 128, 128, 255]);
        d.layers[1].visible = false;
        assert_eq!(d.composite(), vec![255; 4]);
    }
    #[test]
    fn version_five_projects_migrate_without_groups_or_pixel_changes() {
        let doc = Document::new(3, 2, true);
        let old = LegacyDocumentV5 {
            width: doc.width,
            height: doc.height,
            dpi: doc.dpi,
            active: doc.active,
            layers: doc
                .layers
                .iter()
                .map(|l| LegacyLayerV5 {
                    label_color: Some([20, 80, 220]),
                    blend_mode: l.blend_mode,
                    name: l.name.clone(),
                    visible: l.visible,
                    opacity: l.opacity,
                    pixels: l.pixels.clone(),
                    alpha_lock: l.alpha_lock,
                })
                .collect(),
        };
        let mut bytes = MAGIC_V5.to_vec();
        bytes.extend(bincode::DefaultOptions::new().serialize(&old).unwrap());
        let path = std::env::temp_dir().join(format!(
            "paintatlas-v5-migration-{}.patlas",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        let loaded = Document::open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(loaded.composite(), doc.composite());
        assert!(loaded.layers.iter().all(|l| l.group.is_none()
            && l.parent.is_none()
            && l.label_color == Some([20, 80, 220])));
    }
    #[test]
    fn version_six_group_hierarchy_migrates_with_editable_children() {
        let mut doc = Document::new(3, 2, true);
        doc.group_layers(&[1]);
        let old = LegacyDocumentV6 {
            width: doc.width,
            height: doc.height,
            dpi: doc.dpi,
            active: doc.active,
            layers: doc
                .layers
                .iter()
                .map(|l| LegacyLayerV6 {
                    group: l.group,
                    parent: l.parent,
                    label_color: l.label_color,
                    blend_mode: l.blend_mode,
                    name: l.name.clone(),
                    visible: l.visible,
                    opacity: l.opacity,
                    pixels: l.pixels.clone(),
                    alpha_lock: l.alpha_lock,
                })
                .collect(),
        };
        let mut bytes = MAGIC_V6.to_vec();
        bytes.extend(bincode::DefaultOptions::new().serialize(&old).unwrap());
        let path = std::env::temp_dir().join(format!(
            "paintatlas-v6-migration-{}.patlas",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        let loaded = Document::open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(loaded.layers.len(), 3);
        assert_eq!(loaded.layers[1].parent, loaded.layers[2].group);
        assert!(loaded.layers.iter().all(|l| l.effects.is_empty()));
        assert_eq!(loaded.composite(), doc.composite());
    }
}
