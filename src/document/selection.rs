//! Transient selection masks and destructive transforms applied only on confirmation.
use crate::document::{Document, blend};
use eframe::egui::{self, Pos2, Rect, Vec2};
use std::sync::Arc;

fn zeroed_bytes(len: usize, label: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|error| format!("Unable to allocate {label}: {error}"))?;
    bytes.resize(len, 0);
    Ok(bytes)
}
fn copied_bytes(source: &[u8], label: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(source.len())
        .map_err(|error| format!("Unable to allocate {label}: {error}"))?;
    bytes.extend_from_slice(source);
    Ok(bytes)
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Combine {
    #[default]
    Replace,
    Add,
    Subtract,
}
#[derive(Clone)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub values: Vec<u8>,
    pub bounds: Rect,
    pub edges: Vec<([f32; 2], [f32; 2])>,
}
impl Mask {
    pub fn memory_bytes(&self) -> usize {
        self.values.len().saturating_add(
            self.edges
                .len()
                .saturating_mul(std::mem::size_of::<([f32; 2], [f32; 2])>()),
        )
    }
    pub fn coverage(&self, x: usize, y: usize) -> f32 {
        self.values
            .get(y * self.width as usize + x)
            .copied()
            .unwrap_or(0) as f32
            / 255.0
    }
    #[allow(dead_code)]
    pub fn from_values(width: u32, height: u32, values: Vec<u8>) -> Option<Self> {
        Self::try_from_values(width, height, values).ok().flatten()
    }
    pub fn try_from_values(
        width: u32,
        height: u32,
        values: Vec<u8>,
    ) -> Result<Option<Self>, String> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| "Selection dimensions are too large.".to_owned())?;
        if width == 0
            || height == 0
            || width > crate::document::MAX_SIDE
            || height > crate::document::MAX_SIDE
            || values.len() != expected
        {
            return Err("Selection data does not match its dimensions.".into());
        }
        let w = width as usize;
        let h = height as usize;
        let mut min = [w, h];
        let mut max = [0, 0];
        for (i, &v) in values.iter().enumerate() {
            if v > 0 {
                let x = i % w;
                let y = i / w;
                min[0] = min[0].min(x);
                min[1] = min[1].min(y);
                max[0] = max[0].max(x + 1);
                max[1] = max[1].max(y + 1);
            }
        }
        if max[0] == 0 {
            return Ok(None);
        }
        let selected = |x: usize, y: usize| values[y * w + x] > 0;
        let mut edges = Vec::new();
        // Merge unit edges into runs, so a rectangular selection is just four edges.
        for y in min[1]..=max[1] {
            let mut start = None;
            for x in min[0]..=max[0] {
                let edge =
                    x < max[0] && ((y < h && selected(x, y)) != (y > 0 && selected(x, y - 1)));
                if edge {
                    start.get_or_insert(x);
                } else if let Some(a) = start.take() {
                    edges
                        .try_reserve(1)
                        .map_err(|e| format!("Unable to allocate selection edges: {e}"))?;
                    edges.push(([a as f32, y as f32], [x as f32, y as f32]));
                }
            }
        }
        for x in min[0]..=max[0] {
            let mut start = None;
            for y in min[1]..=max[1] {
                let edge =
                    y < max[1] && ((x < w && selected(x, y)) != (x > 0 && selected(x - 1, y)));
                if edge {
                    start.get_or_insert(y);
                } else if let Some(a) = start.take() {
                    edges
                        .try_reserve(1)
                        .map_err(|e| format!("Unable to allocate selection edges: {e}"))?;
                    edges.push(([x as f32, a as f32], [x as f32, y as f32]));
                }
            }
        }
        Ok(Some(Self {
            width,
            height,
            values,
            bounds: Rect::from_min_max(
                Pos2::new(min[0] as f32, min[1] as f32),
                Pos2::new(max[0] as f32, max[1] as f32),
            ),
            edges,
        }))
    }
    pub fn rectangle(width: u32, height: u32, rect: Rect) -> Option<Self> {
        Self::try_rectangle(width, height, rect).ok().flatten()
    }
    pub fn try_rectangle(width: u32, height: u32, rect: Rect) -> Result<Option<Self>, String> {
        if width == 0
            || height == 0
            || width > crate::document::MAX_SIDE
            || height > crate::document::MAX_SIDE
        {
            return Err("Selection dimensions exceed the document limits.".into());
        }
        let len = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| "Selection dimensions are too large.".to_owned())?;
        let mut values = zeroed_bytes(len, "selection mask")?;
        let x0 = rect.min.x.floor().clamp(0.0, width as f32) as usize;
        let x1 = rect.max.x.ceil().clamp(0.0, width as f32) as usize;
        let y0 = rect.min.y.floor().clamp(0.0, height as f32) as usize;
        let y1 = rect.max.y.ceil().clamp(0.0, height as f32) as usize;
        for y in y0..y1 {
            values[y * width as usize + x0..y * width as usize + x1].fill(255);
        }
        Self::try_from_values(width, height, values)
    }
    #[allow(dead_code)]
    pub fn polygon(width: u32, height: u32, points: &[Pos2]) -> Option<Self> {
        Self::try_polygon(width, height, points).ok().flatten()
    }
    pub fn try_polygon(width: u32, height: u32, points: &[Pos2]) -> Result<Option<Self>, String> {
        if points.len() < 3 {
            return Ok(None);
        }
        if width == 0
            || height == 0
            || width > crate::document::MAX_SIDE
            || height > crate::document::MAX_SIDE
        {
            return Err("Selection dimensions exceed the document limits.".into());
        }
        let bounds = Rect::from_points(points);
        let len = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| "Selection dimensions are too large.".to_owned())?;
        let mut values = zeroed_bytes(len, "lasso mask")?;
        // Even-odd scanline filling also handles self-intersecting lasso paths.
        for y in bounds.top().floor().max(0.0) as usize
            ..(bounds.bottom().ceil().min(height as f32) as usize)
        {
            let scan = y as f32 + 0.5;
            let mut intersections = Vec::new();
            intersections
                .try_reserve_exact(points.len())
                .map_err(|e| format!("Unable to allocate lasso intersections: {e}"))?;
            for (a, b) in points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
            {
                if (a.y > scan) != (b.y > scan) {
                    intersections.push(a.x + (scan - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
            intersections.sort_by(f32::total_cmp);
            for pair in intersections.as_chunks::<2>().0 {
                let start = (pair[0] - 0.5).ceil().clamp(0.0, width as f32) as usize;
                let end = (pair[1] - 0.5).ceil().clamp(0.0, width as f32) as usize;
                values[y * width as usize + start..y * width as usize + end].fill(255);
            }
        }
        Self::try_from_values(width, height, values)
    }
    #[allow(dead_code)]
    pub fn combine(old: Option<&Self>, new: Option<Self>, mode: Combine) -> Option<Arc<Self>> {
        Self::try_combine(old, new, mode).ok().flatten()
    }
    pub fn try_combine(
        old: Option<&Self>,
        new: Option<Self>,
        mode: Combine,
    ) -> Result<Option<Arc<Self>>, String> {
        if mode == Combine::Replace {
            return Ok(new.map(Arc::new));
        }
        let Some(old) = old else {
            return Ok(if mode == Combine::Add {
                new.map(Arc::new)
            } else {
                None
            });
        };
        let Some(new) = new else {
            return Ok(Some(Arc::new(old.clone())));
        };
        if old.width != new.width || old.height != new.height {
            return Err("Selection dimensions do not match.".into());
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(old.values.len())
            .map_err(|e| format!("Unable to combine selection masks: {e}"))?;
        values.extend(old.values.iter().zip(new.values).map(|(&a, b)| {
            if mode == Combine::Add {
                a.max(b)
            } else {
                a.saturating_sub(b)
            }
        }));
        Ok(Self::try_from_values(old.width, old.height, values)?.map(Arc::new))
    }
    #[allow(dead_code)]
    pub fn inverted(&self) -> Option<Arc<Self>> {
        self.try_inverted().ok().flatten()
    }
    pub fn try_inverted(&self) -> Result<Option<Arc<Self>>, String> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(self.values.len())
            .map_err(|e| format!("Unable to invert selection mask: {e}"))?;
        values.extend(self.values.iter().map(|v| 255 - v));
        Ok(Self::try_from_values(self.width, self.height, values)?.map(Arc::new))
    }
}

pub struct Floating {
    pub layer: usize,
    pub new_layer: bool,
    pub name: Option<String>,
    pub original: Rect,
    pub bounds: Rect,
    pub base: Vec<u8>,
    pub pixels: Vec<u8>,
    pub mask: Vec<u8>,
    pub size: [usize; 2],
    pub duplicate: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub angle: f32,
}
impl Floating {
    pub fn memory_bytes(&self) -> usize {
        self.base
            .len()
            .saturating_add(self.pixels.len())
            .saturating_add(self.mask.len())
            .saturating_add(self.name.as_ref().map_or(0, String::len))
    }
}
impl Floating {
    pub fn new(doc: &Document, mask: &Mask, duplicate: bool) -> Result<Self, String> {
        Self::from_mask(doc, mask, duplicate, false)
    }
    pub fn for_layer(doc: &Document, mask: &Mask, duplicate: bool) -> Result<Self, String> {
        Self::from_mask(doc, mask, duplicate, true)
    }
    fn from_mask(
        doc: &Document,
        mask: &Mask,
        duplicate: bool,
        whole_layer: bool,
    ) -> Result<Self, String> {
        let layer = &doc.layers[doc.active];
        if !layer.visible {
            return Err("Make the layer visible to transform it.".into());
        }
        if layer.alpha_lock && !whole_layer {
            return Err("Desative Bloquear transparência para transformar o recorte.".into());
        }
        let r = mask.bounds;
        let size = [r.width() as usize, r.height() as usize];
        let crop_pixels = size[0]
            .checked_mul(size[1])
            .and_then(|count| count.checked_mul(4))
            .ok_or_else(|| "Selection is too large to transform.".to_owned())?;
        let crop_mask_len = crop_pixels / 4;
        let mut base = copied_bytes(&layer.pixels, "selection base")?;
        let mut pixels = zeroed_bytes(crop_pixels, "selected pixels")?;
        let mut crop_mask = zeroed_bytes(crop_mask_len, "selection mask")?;
        let mut has_pixels = false;
        for y in 0..size[1] {
            for x in 0..size[0] {
                let dx = r.min.x as usize + x;
                let dy = r.min.y as usize + y;
                let i = (dy * doc.width as usize + dx) * 4;
                let j = (y * size[0] + x) * 4;
                let coverage = mask.coverage(dx, dy);
                pixels[j..j + 4].copy_from_slice(&layer.pixels[i..i + 4]);
                pixels[j + 3] = (pixels[j + 3] as f32 * coverage).round() as u8;
                crop_mask[y * size[0] + x] = mask.values[dy * doc.width as usize + dx];
                has_pixels |= pixels[j + 3] > 0;
                if !duplicate {
                    base[i + 3] = (base[i + 3] as f32 * (1.0 - coverage)).round() as u8;
                    if base[i + 3] == 0 {
                        base[i..i + 3].fill(0);
                    }
                }
            }
        }
        if !has_pixels {
            return Err("A seleção não contém pixels na camada ativa.".into());
        }
        Ok(Self {
            layer: doc.active,
            new_layer: false,
            name: None,
            original: r,
            bounds: r,
            base,
            pixels,
            mask: crop_mask,
            size,
            duplicate,
            flip_x: false,
            flip_y: false,
            angle: 0.0,
        })
    }
    pub fn pasted(
        doc: &Document,
        size: [usize; 2],
        pixels: Vec<u8>,
        bounds: Rect,
    ) -> Result<Self, String> {
        if size.contains(&0)
            || size.iter().any(|&s| s > crate::document::MAX_SIDE as usize)
            || size[0].checked_mul(size[1]).and_then(|n| n.checked_mul(4)) != Some(pixels.len())
            || !bounds.is_finite()
            || bounds.width() <= 0.0
            || bounds.height() <= 0.0
        {
            return Err("Image ou dimensões de colagem inválidas.".into());
        }
        if !doc.can_add() {
            return Err("Limite de camadas ou de memória atingido.".into());
        }
        if !pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0) {
            return Err("A imagem está totalmente transparente.".into());
        }
        // The pasted rectangle is selected independently of image alpha. Using
        // alpha as selection coverage would fade translucent pixels on the next copy.
        let mask_len = size[0]
            .checked_mul(size[1])
            .ok_or_else(|| "Selection is too large to paste.".to_owned())?;
        let mut mask = Vec::new();
        mask.try_reserve_exact(mask_len)
            .map_err(|error| format!("Unable to allocate pasted selection mask: {error}"))?;
        mask.resize(mask_len, 255);
        let base_len = (doc.width as usize)
            .checked_mul(doc.height as usize)
            .and_then(|count| count.checked_mul(4))
            .ok_or_else(|| "Document is too large for pasted selection.".to_owned())?;
        let base = zeroed_bytes(base_len, "pasted selection base")?;
        Ok(Self {
            layer: doc.active + 1,
            new_layer: true,
            name: None,
            original: bounds,
            bounds,
            base,
            pixels,
            mask,
            size,
            duplicate: true,
            flip_x: false,
            flip_y: false,
            angle: 0.0,
        })
    }
    pub fn to_world(&self, point: Pos2) -> Pos2 {
        self.bounds.center()
            + egui::emath::Rot2::from_angle(self.angle) * (point - self.bounds.center())
    }
    pub fn to_local(&self, point: Pos2) -> Pos2 {
        self.bounds.center()
            + egui::emath::Rot2::from_angle(-self.angle) * (point - self.bounds.center())
    }
    pub fn rotated_bounds(&self) -> Rect {
        Rect::from_points(
            &[
                self.bounds.left_top(),
                self.bounds.right_top(),
                self.bounds.right_bottom(),
                self.bounds.left_bottom(),
            ]
            .map(|p| self.to_world(p)),
        )
    }
    pub fn unchanged(&self) -> bool {
        !self.duplicate
            && self.bounds == self.original
            && !self.flip_x
            && !self.flip_y
            && self.angle == 0.0
    }
    pub fn rasterize(
        &self,
        width: u32,
        height: u32,
    ) -> Result<(Vec<u8>, Option<Arc<Mask>>), String> {
        let mut out = copied_bytes(&self.base, "transformed selection")?;
        let mask_len = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| "Transformed selection is too large.".to_owned())?;
        let mut mask = zeroed_bytes(mask_len, "transformed selection mask")?;
        let r = self.bounds;
        let area = self.rotated_bounds();
        for y in area.top().floor().clamp(0.0, height as f32) as usize
            ..area.bottom().ceil().clamp(0.0, height as f32) as usize
        {
            for x in area.left().floor().clamp(0.0, width as f32) as usize
                ..area.right().ceil().clamp(0.0, width as f32) as usize
            {
                let local = self.to_local(Pos2::new(x as f32 + 0.5, y as f32 + 0.5));
                let mut uv = Vec2::new(
                    (local.x - r.min.x) / r.width(),
                    (local.y - r.min.y) / r.height(),
                );
                if self.flip_x {
                    uv.x = 1.0 - uv.x;
                }
                if self.flip_y {
                    uv.y = 1.0 - uv.y;
                }
                if uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 {
                    continue;
                }
                let p = Vec2::new(
                    uv.x * self.size[0] as f32 - 0.5,
                    uv.y * self.size[1] as f32 - 0.5,
                );
                let sample = sample_rgba(&self.pixels, self.size, p);
                let i = (y * width as usize + x) * 4;
                blend(&mut out[i..i + 4], sample, 1.0);
                mask[y * width as usize + x] = sample_mask(&self.mask, self.size, p);
            }
        }
        Ok((
            out,
            Mask::try_from_values(width, height, mask)?.map(Arc::new),
        ))
    }
}
fn taps(size: [usize; 2], p: Vec2) -> [(usize, f32); 4] {
    // Match OpenGL's clamp-to-edge sampling of the preview texture.
    let p = Vec2::new(
        p.x.clamp(0.0, size[0] as f32 - 1.0),
        p.y.clamp(0.0, size[1] as f32 - 1.0),
    );
    let x = p.x.floor() as i32;
    let y = p.y.floor() as i32;
    let fx = p.x - x as f32;
    let fy = p.y - y as f32;
    let mut out = [(0, 0.0); 4];
    let mut n = 0;
    for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
        for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
            if x + dx >= 0 && y + dy >= 0 && x + dx < size[0] as i32 && y + dy < size[1] as i32 {
                out[n] = (((y + dy) as usize * size[0] + (x + dx) as usize), wx * wy);
                n += 1;
            }
        }
    }
    out
}
fn sample_rgba(pixels: &[u8], size: [usize; 2], p: Vec2) -> [u8; 4] {
    let mut rgba = [0.0; 4];
    for (i, w) in taps(size, p) {
        let a = pixels[i * 4 + 3] as f32 / 255.0;
        rgba[3] += a * w;
        for c in 0..3 {
            rgba[c] += pixels[i * 4 + c] as f32 * a * w;
        }
    }
    if rgba[3] <= 0.0 {
        return [0; 4];
    }
    [
        (rgba[0] / rgba[3]).round() as u8,
        (rgba[1] / rgba[3]).round() as u8,
        (rgba[2] / rgba[3]).round() as u8,
        (rgba[3] * 255.0).round() as u8,
    ]
}
fn sample_mask(pixels: &[u8], size: [usize; 2], p: Vec2) -> u8 {
    taps(size, p)
        .iter()
        .map(|(i, w)| pixels[*i] as f32 * w)
        .sum::<f32>()
        .round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }
    fn pixel(doc: &mut Document, x: usize, y: usize, rgba: [u8; 4]) {
        let i = (y * doc.width as usize + x) * 4;
        doc.layers[doc.active].pixels[i..i + 4].copy_from_slice(&rgba);
    }
    fn at(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
        pixels[(y * width + x) * 4..(y * width + x) * 4 + 4]
            .try_into()
            .unwrap()
    }
    #[test]
    fn rotation_preserves_cut_pixels_and_rotates_selection_mask() {
        let doc = Document::new(8, 8, false);
        let mut image = vec![0; 16];
        image[..4].copy_from_slice(&[200, 30, 10, 128]);
        let mut f = Floating::pasted(&doc, [2, 2], image, rect(3.0, 3.0, 2.0, 2.0)).unwrap();
        f.angle = std::f32::consts::FRAC_PI_2;
        let (pixels, mask) = f.rasterize(8, 8).unwrap();
        assert_eq!(at(&pixels, 8, 4, 3), [200, 30, 10, 128]);
        assert_eq!(at(&pixels, 8, 3, 3), [0; 4]);
        assert!(mask.unwrap().coverage(4, 3) > 0.99);
        f.angle = 0.0;
        assert_eq!(
            at(&f.rasterize(8, 8).unwrap().0, 8, 3, 3),
            [200, 30, 10, 128]
        );
        f.angle = std::f32::consts::FRAC_PI_4;
        assert!(f.rotated_bounds().width() > f.bounds.width());
        let p = Pos2::new(3.2, 3.7);
        assert!(f.to_local(f.to_world(p)).distance(p) < 0.0001);
    }
    #[test]
    fn masks_rectangle_lasso_combine_invert_and_boundaries() {
        let a = Mask::rectangle(8, 8, rect(1.0, 1.0, 3.0, 3.0)).unwrap();
        assert_eq!(a.values.iter().filter(|&&v| v > 0).count(), 9);
        assert_eq!(a.edges.len(), 4);
        let b = Mask::polygon(
            8,
            8,
            &[
                Pos2::new(3.0, 1.0),
                Pos2::new(5.0, 1.0),
                Pos2::new(5.0, 4.0),
                Pos2::new(3.0, 4.0),
            ],
        )
        .unwrap();
        let union = Mask::combine(Some(&a), Some(b), Combine::Add).unwrap();
        assert_eq!(union.values.iter().filter(|&&v| v > 0).count(), 12);
        let sub = Mask::combine(Some(&union), Some(a), Combine::Subtract).unwrap();
        assert_eq!(sub.values.iter().filter(|&&v| v > 0).count(), 3);
        let inverse = sub.inverted().unwrap();
        assert_eq!(inverse.values.iter().filter(|&&v| v > 0).count(), 61);
        let full = Mask::rectangle(8, 8, rect(-20.0, -20.0, 100.0, 100.0)).unwrap();
        assert_eq!(full.edges.len(), 4);
        assert_eq!(full.bounds, rect(0.0, 0.0, 8.0, 8.0));
        assert!(Mask::rectangle(8, 8, rect(20.0, 20.0, 1.0, 1.0)).is_none());
    }
    #[test]
    fn integer_move_copy_flip_and_outside_preview_preserve_source_pixels() {
        let mut doc = Document::new(8, 8, false);
        pixel(&mut doc, 1, 1, [255, 20, 10, 128]);
        pixel(&mut doc, 2, 1, [20, 255, 10, 255]);
        let original = doc.layers[0].pixels.clone();
        let mask = Mask::rectangle(8, 8, rect(1.0, 1.0, 2.0, 2.0)).unwrap();
        for duplicate in [false, true] {
            let mut f = Floating::new(&doc, &mask, duplicate).unwrap();
            f.bounds = f.bounds.translate(Vec2::new(3.0, 2.0));
            let (out, new_mask) = f.rasterize(8, 8).unwrap();
            assert_eq!(at(&out, 8, 4, 3), [255, 20, 10, 128]);
            assert_eq!(at(&out, 8, 5, 3), [20, 255, 10, 255]);
            assert_eq!(
                at(&out, 8, 1, 1),
                if duplicate {
                    [255, 20, 10, 128]
                } else {
                    [0; 4]
                }
            );
            assert_eq!(new_mask.unwrap().bounds, rect(4.0, 3.0, 2.0, 2.0));
            assert_eq!(doc.layers[0].pixels, original);
        }
        let mut f = Floating::new(&doc, &mask, false).unwrap();
        f.flip_x = true;
        let (out, _) = f.rasterize(8, 8).unwrap();
        assert_eq!(at(&out, 8, 1, 1), [20, 255, 10, 255]);
        f.bounds = f.bounds.translate(Vec2::new(-30.0, 0.0));
        let (out, mask) = f.rasterize(8, 8).unwrap();
        assert!(mask.is_none());
        assert_eq!(at(&out, 8, 1, 1), [0; 4]);
        f.bounds = f.original;
        f.flip_x = false;
        assert!(f.unchanged());
        assert_eq!(doc.layers[0].pixels, original);
    }
    #[test]
    fn scaling_uses_premultiplied_alpha_and_original_crop() {
        let mut doc = Document::new(4, 4, false);
        pixel(&mut doc, 0, 0, [255, 0, 0, 255]);
        pixel(&mut doc, 1, 0, [0, 0, 255, 0]);
        let mask = Mask::rectangle(4, 4, rect(0.0, 0.0, 2.0, 1.0)).unwrap();
        let mut f = Floating::new(&doc, &mask, false).unwrap();
        f.bounds = rect(0.0, 0.0, 4.0, 2.0);
        let (a, _) = f.rasterize(4, 4).unwrap();
        for p in a.as_chunks::<4>().0.iter().filter(|p| p[3] > 0) {
            assert_eq!(p[0], 255);
            assert_eq!(p[2], 0);
        }
        f.bounds = rect(0.0, 0.0, 1.0, 1.0);
        let _ = f.rasterize(4, 4).unwrap();
        f.bounds = rect(0.0, 0.0, 4.0, 2.0);
        assert_eq!(f.rasterize(4, 4).unwrap().0, a);
    }

    #[test]
    fn failed_selection_rasterization_preserves_floating_source() {
        let mut doc = Document::new(4, 4, false);
        doc.layers[0].pixels[(4 + 1) * 4 + 3] = 255;
        let mask = Mask::rectangle(4, 4, rect(1.0, 1.0, 2.0, 2.0)).unwrap();
        let floating = Floating::new(&doc, &mask, false).unwrap();
        let source = floating.base.clone();
        assert!(floating.rasterize(u32::MAX, u32::MAX).is_err());
        assert_eq!(floating.base, source);
        assert_eq!(doc.layers[0].pixels[(4 + 1) * 4 + 3], 255);
    }
    #[test]
    fn fallible_mask_constructors_reject_overflow_and_mismatched_buffers() {
        assert!(Mask::try_rectangle(u32::MAX, u32::MAX, rect(0.0, 0.0, 1.0, 1.0)).is_err());
        assert!(Mask::try_from_values(2, 2, vec![0; 3]).is_err());
        assert!(Mask::try_polygon(u32::MAX, 2, &[Pos2::ZERO; 3]).is_err());
    }
    #[test]
    fn masks_clip_brush_fill_and_are_not_saved_in_projects() {
        let mut doc = Document::new(8, 8, false);
        doc.selection = Mask::rectangle(8, 8, rect(2.0, 2.0, 3.0, 3.0)).map(Arc::new);
        let brush = crate::document::Brush {
            color: [255, 0, 0, 255],
            size: 30.0,
            opacity: 1.0,
            hardness: 1.0,
            tip: None,
            spacing: 0.15,
        };
        doc.stamp([4.0, 4.0], brush.clone(), false);
        assert_eq!(at(&doc.layers[0].pixels, 8, 2, 2), [255, 0, 0, 255]);
        assert_eq!(at(&doc.layers[0].pixels, 8, 1, 2), [0; 4]);
        doc.layers[0].pixels.fill(0);
        doc.fill([3.0, 3.0], [0, 255, 0, 255], 1.0, 0).unwrap();
        assert_eq!(
            doc.layers[0]
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 0)
                .count(),
            9
        );
        doc.stamp([4.0, 4.0], brush, true);
        assert!(doc.layers[0].pixels.iter().all(|&v| v == 0));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("selection.patlas");
        doc.save(&path).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert!(loaded.selection.is_none());
        assert_eq!(loaded.layers[0].pixels, doc.layers[0].pixels);
    }
}
