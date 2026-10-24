//! Reversible pixel tiles: a small stroke must not store every layer again.
use crate::document::{Document, Layer};
use std::collections::VecDeque;

const TILE: usize = 64;
const MAX_STEPS: usize = 30;
const MAX_HISTORY_BYTES: usize = 128 * 1024 * 1024;

struct LayerProperties {
    blend_mode: crate::blend::BlendMode,
    name: String,
    visible: bool,
    opacity: f32,
    alpha_lock: bool,
}
impl LayerProperties {
    fn from(layer: &Layer) -> Self {
        Self {
            blend_mode: layer.blend_mode,
            name: layer.name.clone(),
            visible: layer.visible,
            opacity: layer.opacity,
            alpha_lock: layer.alpha_lock,
        }
    }
    fn differs(a: &Layer, b: &Layer) -> bool {
        a.blend_mode != b.blend_mode
            || a.name != b.name
            || a.visible != b.visible
            || a.opacity != b.opacity
            || a.alpha_lock != b.alpha_lock
    }
    fn swap(&mut self, layer: &mut Layer) {
        std::mem::swap(&mut self.blend_mode, &mut layer.blend_mode);
        std::mem::swap(&mut self.name, &mut layer.name);
        std::mem::swap(&mut self.visible, &mut layer.visible);
        std::mem::swap(&mut self.opacity, &mut layer.opacity);
        std::mem::swap(&mut self.alpha_lock, &mut layer.alpha_lock);
    }
}
struct TilePatch {
    layer: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}
struct Delta {
    active: usize,
    properties: Vec<(usize, LayerProperties)>,
    tiles: Vec<TilePatch>,
}
enum Entry {
    Tiles(Delta),
    // Changes to layer structure use a full reversible snapshot. Ordinary
    // strokes, fills, names, visibility and opacity use compact differences.
    Structure(Document),
}
impl Entry {
    fn between(before: Document, after: &Document) -> Option<Self> {
        if before.width != after.width
            || before.height != after.height
            || before.layers.len() != after.layers.len()
            || before
                .layers
                .iter()
                .zip(&after.layers)
                .any(|(a, b)| a.pixels.len() != b.pixels.len())
        {
            return Some(Self::Structure(before));
        }
        let width = before.width as usize;
        let height = before.height as usize;
        let mut delta = Delta {
            active: before.active,
            properties: Vec::new(),
            tiles: Vec::new(),
        };
        for (layer, (old, new)) in before.layers.iter().zip(&after.layers).enumerate() {
            if LayerProperties::differs(old, new) {
                delta.properties.push((layer, LayerProperties::from(old)));
            }
            for y in (0..height).step_by(TILE) {
                for x in (0..width).step_by(TILE) {
                    let w = TILE.min(width - x);
                    let h = TILE.min(height - y);
                    let changed = (y..y + h).any(|row| {
                        let start = (row * width + x) * 4;
                        old.pixels[start..start + w * 4] != new.pixels[start..start + w * 4]
                    });
                    if changed {
                        let mut pixels = Vec::with_capacity(w * h * 4);
                        for row in y..y + h {
                            let start = (row * width + x) * 4;
                            pixels.extend_from_slice(&old.pixels[start..start + w * 4]);
                        }
                        delta.tiles.push(TilePatch {
                            layer,
                            x,
                            y,
                            width: w,
                            height: h,
                            pixels,
                        });
                    }
                }
            }
        }
        // Changing only the selected layer, or an operation that painted no
        // pixels, must not consume a history step or destroy redo.
        if delta.tiles.is_empty() && delta.properties.is_empty() {
            None
        } else {
            Some(Self::Tiles(delta))
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Structure(doc) => {
                doc.bytes() + doc.layers.iter().map(|l| l.name.len()).sum::<usize>()
            }
            Self::Tiles(delta) => {
                delta
                    .tiles
                    .iter()
                    .map(|t| t.pixels.len() + std::mem::size_of::<TilePatch>())
                    .sum::<usize>()
                    + delta
                        .properties
                        .iter()
                        .map(|(_, p)| p.name.len() + std::mem::size_of::<LayerProperties>())
                        .sum::<usize>()
            }
        }
    }
    fn swap(&mut self, doc: &mut Document) {
        let selection = doc.selection.take();
        match self {
            Self::Structure(previous) => std::mem::swap(previous, doc),
            Self::Tiles(delta) => {
                std::mem::swap(&mut delta.active, &mut doc.active);
                for (layer, properties) in &mut delta.properties {
                    properties.swap(&mut doc.layers[*layer]);
                }
                for tile in &mut delta.tiles {
                    let pixels = &mut doc.layers[tile.layer].pixels;
                    for row in 0..tile.height {
                        let start = ((tile.y + row) * doc.width as usize + tile.x) * 4;
                        pixels[start..start + tile.width * 4].swap_with_slice(
                            &mut tile.pixels[row * tile.width * 4..(row + 1) * tile.width * 4],
                        );
                    }
                }
            }
        }
        // Restoring history must invalidate every affected GPU layer, including
        // layer insertions/removals; a previous partial upload is not sufficient.
        doc.selection = selection;
        doc.damage = None;
    }
}
pub struct History {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    pending: Option<Document>,
    stroke_layer: Option<usize>,
}
impl History {
    pub fn new() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            pending: None,
            stroke_layer: None,
        }
    }
    /// Capture the start of one operation. Only one temporary full snapshot is
    /// retained; commit replaces it with reversible changed tiles.
    pub fn push(&mut self, doc: &Document) {
        self.commit(doc);
        let mut snapshot = doc.clone();
        snapshot.selection = None;
        self.pending = Some(snapshot);
    }
    pub fn push_stroke(&mut self, doc: &mut Document) {
        self.commit(doc);
        self.stroke_layer = Some(doc.active);
        *doc.stroke_tiles.get_mut() = Some(Default::default());
    }
    pub fn commit(&mut self, doc: &Document) {
        let stroke = self.stroke_layer.take().and_then(|layer| {
            let tiles = doc.stroke_tiles.borrow_mut().take().unwrap_or_default();
            let mut patches = Vec::with_capacity(tiles.len());
            for ((ty, tx), pixels) in tiles {
                let x = tx * crate::document::PAINT_TILE;
                let y = ty * crate::document::PAINT_TILE;
                let width = crate::document::PAINT_TILE.min(doc.width as usize - x);
                let height = crate::document::PAINT_TILE.min(doc.height as usize - y);
                let changed = (0..height).any(|row| {
                    let start = ((y + row) * doc.width as usize + x) * 4;
                    doc.layers[layer].pixels[start..start + width * 4]
                        != pixels[row * width * 4..(row + 1) * width * 4]
                });
                if changed {
                    patches.push(TilePatch {
                        layer,
                        x,
                        y,
                        width,
                        height,
                        pixels,
                    });
                }
            }
            (!patches.is_empty()).then_some(Entry::Tiles(Delta {
                active: layer,
                properties: Vec::new(),
                tiles: patches,
            }))
        });
        let entry = stroke.or_else(|| {
            self.pending
                .take()
                .and_then(|before| Entry::between(before, doc))
        });
        if let Some(entry) = entry {
            self.redo.clear();
            self.undo.push_back(entry);
            let mut bytes = self.undo.iter().map(Entry::bytes).sum::<usize>();
            while self.undo.len() > 1 && (self.undo.len() > MAX_STEPS || bytes > MAX_HISTORY_BYTES)
            {
                bytes = bytes.saturating_sub(self.undo.pop_front().unwrap().bytes());
            }
        }
    }
    pub fn counts(&self) -> (usize, usize) {
        (
            self.undo.len() + usize::from(self.pending.is_some() || self.stroke_layer.is_some()),
            self.redo.len(),
        )
    }
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        self.commit(doc);
        if let Some(mut entry) = self.undo.pop_back() {
            entry.swap(doc);
            self.redo.push(entry);
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        self.commit(doc);
        if let Some(mut entry) = self.redo.pop() {
            entry.swap(doc);
            self.undo.push_back(entry);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn brush() -> crate::document::Brush {
        crate::document::Brush {
            color: [20, 80, 160, 255],
            size: 31.0,
            opacity: 0.4,
            hardness: 0.5,
            tip: None,
            spacing: 0.15,
        }
    }
    #[test]
    fn incremental_strokes_capture_original_pixels_once_and_preserve_redo_on_noop() {
        let mut doc = Document::new(263, 259, true);
        let original = doc.layers[1].pixels.clone();
        let mut h = History::new();
        h.push_stroke(&mut doc);
        for _ in 0..8 {
            doc.stamp([128.0, 128.0], brush(), false);
        }
        doc.stamp([262.0, 258.0], brush(), false);
        let painted = doc.layers[1].pixels.clone();
        h.commit(&doc);
        assert!(h.undo(&mut doc));
        assert_eq!(doc.layers[1].pixels, original);
        h.push_stroke(&mut doc);
        let mut invisible = brush();
        invisible.opacity = 0.0;
        doc.stamp([50.0, 50.0], invisible, false);
        h.commit(&doc);
        assert_eq!(h.counts(), (0, 1));
        assert!(h.redo(&mut doc));
        assert_eq!(doc.layers[1].pixels, painted);
        h.push_stroke(&mut doc);
        doc.stamp([128.0, 128.0], brush(), true);
        h.commit(&doc);
        assert!(h.undo(&mut doc));
        assert_eq!(doc.layers[1].pixels, painted);
        assert!(h.redo(&mut doc));
        assert_ne!(doc.layers[1].pixels, painted);
    }
    #[test]
    fn uploads_reset_each_frame_but_history_keeps_original_tiles() {
        let mut doc = Document::new(3000, 3000, true);
        let mut h = History::new();
        h.push_stroke(&mut doc);
        doc.stamp([10.0, 10.0], brush(), false);
        doc.damage = None; // texture upload consumed the previous frame
        doc.stamp([2900.0, 2900.0], brush(), false);
        assert!(!doc.dirty_tiles.contains(&(0, 0)));
        h.commit(&doc);
        assert!(h.undo.iter().map(Entry::bytes).sum::<usize>() < 256 * 1024);
        assert!(h.undo(&mut doc));
        assert!(doc.layers[1].pixels.iter().all(|v| *v == 0));
    }
    #[test]
    #[ignore = "manual repeatable 3000x3000 stroke benchmark"]
    fn benchmark_stroke_history_3000() {
        for incremental in [false, true] {
            let mut doc = Document::new(3000, 3000, true);
            let mut h = History::new();
            let mut stages = [std::time::Duration::ZERO; 3];
            let start = std::time::Instant::now();
            for n in 0..30 {
                let stage = std::time::Instant::now();
                if incremental {
                    h.push_stroke(&mut doc);
                } else {
                    h.push(&doc);
                }
                stages[0] += stage.elapsed();
                let stage = std::time::Instant::now();
                for step in 0..40 {
                    doc.stamp(
                        [300.0 + step as f32 * 3.0, 300.0 + n as f32 * 2.0],
                        brush(),
                        false,
                    );
                }
                stages[1] += stage.elapsed();
                let stage = std::time::Instant::now();
                h.commit(&doc);
                stages[2] += stage.elapsed();
            }
            eprintln!(
                "incremental={incremental}: {:?} for 30 strokes; begin/paint/commit={stages:?}",
                start.elapsed()
            );
        }
    }
    #[test]
    fn twenty_small_edits_on_large_two_layer_canvas_roundtrip_without_losing_history() {
        let mut doc = Document::new(2048, 2048, true);
        let mut history = History::new();
        for i in 0..20 {
            history.push(&doc);
            doc.layers[1].pixels[(100 * 2048 + 100 + i) * 4 + 3] = 255;
            history.commit(&doc);
        }
        assert_eq!(history.counts(), (20, 0));
        assert!(history.undo.iter().map(Entry::bytes).sum::<usize>() < 1024 * 1024);
        for _ in 0..3 {
            for i in (0..20).rev() {
                assert!(history.undo(&mut doc));
                assert_eq!(doc.layers[1].pixels[(100 * 2048 + 100 + i) * 4 + 3], 0);
            }
            assert!(!history.undo(&mut doc));
            for i in 0..20 {
                assert!(history.redo(&mut doc));
                assert_eq!(doc.layers[1].pixels[(100 * 2048 + 100 + i) * 4 + 3], 255);
            }
            assert!(!history.redo(&mut doc));
        }
    }
    #[test]
    fn layer_structure_metadata_reorder_and_edge_tiles_roundtrip() {
        let mut doc = Document::new(67, 71, true);
        let original = doc.clone();
        let mut h = History::new();
        h.push(&doc);
        doc.layers[1].pixels[(70 * 67 + 66) * 4 + 3] = 200;
        h.commit(&doc);
        h.push(&doc);
        doc.add_layer();
        h.commit(&doc);
        h.push(&doc);
        doc.layers[2].name = "Detalhes".into();
        doc.layers[2].opacity = 0.5;
        doc.layers[2].alpha_lock = true;
        doc.layers[0].visible = false;
        h.commit(&doc);
        h.push(&doc);
        doc.layers.swap(1, 2);
        doc.active = 1;
        h.commit(&doc);
        h.push(&doc);
        doc.layers.remove(0);
        doc.active = 0;
        h.commit(&doc);
        let final_doc = doc.clone();
        for _ in 0..2 {
            for _ in 0..5 {
                assert!(h.undo(&mut doc));
            }
            assert_eq!(doc.composite(), original.composite());
            assert_eq!(doc.layers.len(), 2);
            assert_eq!(doc.active, 1);
            for _ in 0..5 {
                assert!(h.redo(&mut doc));
            }
            assert_eq!(doc.composite(), final_doc.composite());
            assert_eq!(doc.layers[0].name, final_doc.layers[0].name);
            assert_eq!(doc.layers[0].alpha_lock, final_doc.layers[0].alpha_lock);
            assert_eq!(doc.active, final_doc.active);
        }
    }
    #[test]
    fn no_op_does_not_consume_steps_or_destroy_redo_and_new_edit_does() {
        let mut doc = Document::new(8, 8, false);
        let mut h = History::new();
        h.push(&doc);
        doc.layers[0].pixels[3] = 255;
        assert!(h.undo(&mut doc));
        h.push(&doc);
        h.commit(&doc);
        assert_eq!(h.counts(), (0, 1));
        assert!(h.redo(&mut doc));
        assert!(h.undo(&mut doc));
        h.push(&doc);
        doc.layers[0].pixels[7] = 100;
        h.commit(&doc);
        assert_eq!(h.counts(), (1, 0));
        assert!(!h.redo(&mut doc));
    }
    #[test]
    fn count_limit_discards_oldest_step_and_redo_is_reversible() {
        let mut doc = Document::new(8, 8, false);
        let mut h = History::new();
        for i in 1..=35 {
            h.push(&doc);
            doc.layers[0].pixels[3] = i;
            h.commit(&doc);
        }
        assert_eq!(h.counts(), (30, 0));
        for _ in 0..30 {
            assert!(h.undo(&mut doc));
        }
        assert_eq!(doc.layers[0].pixels[3], 5);
        assert!(!h.undo(&mut doc));
        for _ in 0..30 {
            assert!(h.redo(&mut doc));
        }
        assert_eq!(doc.layers[0].pixels[3], 35);
    }
}
