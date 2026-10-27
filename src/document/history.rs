//! Reversible pixel tiles: a small stroke must not store every layer again.
use crate::document::{Document, Layer};
use std::collections::VecDeque;

const TILE: usize = 64;
const MAX_STEPS: usize = 30;
const MAX_HISTORY_BYTES: usize = 128 * 1024 * 1024;

#[derive(PartialEq)]
struct LayerProperties {
    rulers: Vec<crate::rulers::Ruler>,
    effects: std::sync::Arc<Vec<crate::layer_groups::GroupEffect>>,
    group: Option<u64>,
    parent: Option<u64>,
    label_color: Option<[u8; 3]>,
    blend_mode: crate::blend::BlendMode,
    name: String,
    visible: bool,
    opacity: f32,
    alpha_lock: bool,
}
impl LayerProperties {
    fn from(layer: &Layer) -> Self {
        Self {
            rulers: layer.rulers.clone(),
            effects: layer.effects.clone(),
            group: layer.group,
            parent: layer.parent,
            label_color: layer.label_color,
            blend_mode: layer.blend_mode,
            name: layer.name.clone(),
            visible: layer.visible,
            opacity: layer.opacity,
            alpha_lock: layer.alpha_lock,
        }
    }
    fn differs(a: &Layer, b: &Layer) -> bool {
        a.rulers != b.rulers
            || (!std::sync::Arc::ptr_eq(&a.effects, &b.effects) && a.effects != b.effects)
            || a.group != b.group
            || a.parent != b.parent
            || a.label_color != b.label_color
            || a.blend_mode != b.blend_mode
            || a.name != b.name
            || a.visible != b.visible
            || a.opacity != b.opacity
            || a.alpha_lock != b.alpha_lock
    }
    fn swap(&mut self, layer: &mut Layer) {
        std::mem::swap(&mut self.rulers, &mut layer.rulers);
        std::mem::swap(&mut self.effects, &mut layer.effects);
        std::mem::swap(&mut self.group, &mut layer.group);
        std::mem::swap(&mut self.parent, &mut layer.parent);
        std::mem::swap(&mut self.label_color, &mut layer.label_color);
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
    dpi: u32,
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
    fn document_bytes(doc: &Document) -> usize {
        doc.bytes().saturating_add(
            doc.layers
                .iter()
                .map(|layer| layer.name.len())
                .sum::<usize>(),
        )
    }
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
            dpi: before.dpi,
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
        if delta.tiles.is_empty() && delta.properties.is_empty() && before.dpi == after.dpi {
            None
        } else {
            Some(Self::Tiles(delta))
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Structure(doc) => Self::document_bytes(doc),
            Self::Tiles(delta) => {
                delta
                    .tiles
                    .iter()
                    .map(|t| t.pixels.len() + std::mem::size_of::<TilePatch>())
                    .sum::<usize>()
                    + delta
                        .properties
                        .iter()
                        .map(|(_, p)| {
                            p.name.len()
                                + p.rulers.len() * std::mem::size_of::<crate::rulers::Ruler>()
                                + std::mem::size_of::<LayerProperties>()
                                + p.effects.iter().map(|e| e.bytes()).sum::<usize>()
                        })
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
                std::mem::swap(&mut delta.dpi, &mut doc.dpi);
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
        doc.ruler_copies.clear();
        doc.selection = selection;
        doc.damage = None;
    }
}
pub struct History {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    pending: Option<Document>,
    pending_properties: Option<Delta>,
    stroke_layer: Option<usize>,
    limit_warning: bool,
}
impl History {
    pub fn new() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            pending: None,
            pending_properties: None,
            stroke_layer: None,
            limit_warning: false,
        }
    }
    /// Capture the start of one operation. Only one temporary full snapshot is
    /// retained; commit replaces it with reversible changed tiles.
    pub fn push(&mut self, doc: &Document) {
        self.commit(doc);
        self.push_with_limit(doc, MAX_HISTORY_BYTES);
    }
    /// Record a document replacement without cloning its previous full pixel
    /// buffers. The caller must commit pending edits before moving out `before`.
    pub fn record_replacement(&mut self, before: Document, after: &Document) {
        if let Some(entry) = Entry::between(before, after) {
            self.append(entry);
        }
    }
    fn push_with_limit(&mut self, doc: &Document, max_snapshot_bytes: usize) {
        // A full snapshot is temporary memory on top of the live document.
        // Don't duplicate a document when its eventual structure entry would
        // already exceed the history budget.
        if Entry::document_bytes(doc) > max_snapshot_bytes {
            self.undo.clear();
            self.redo.clear();
            self.pending = None;
            self.limit_warning = true;
            return;
        }
        let mut snapshot = doc.clone();
        snapshot.selection = None;
        self.pending = Some(snapshot);
    }
    pub fn push_stroke(&mut self, doc: &mut Document) {
        self.commit(doc);
        self.stroke_layer = Some(doc.active);
        *doc.stroke_tiles.get_mut() = Some(Default::default());
    }
    /// A metadata-only edit must not copy or compare document pixel buffers.
    pub fn push_layer_properties(&mut self, doc: &Document) {
        self.push_layers_properties(doc, &[doc.active]);
    }
    pub fn push_layers_properties(&mut self, doc: &Document, layers: &[usize]) {
        self.commit(doc);
        self.pending_properties = Some(Delta {
            active: doc.active,
            dpi: doc.dpi,
            properties: layers
                .iter()
                .map(|&index| (index, LayerProperties::from(&doc.layers[index])))
                .collect(),
            tiles: Vec::new(),
        });
    }
    /// Record an already computed edit of one layer without cloning other layers.
    pub fn record_layer_pixels(&mut self, doc: &Document, layer: usize, before: &[u8]) {
        self.commit(doc);
        assert_eq!(before.len(), doc.layers[layer].pixels.len());
        let width = doc.width as usize;
        let height = doc.height as usize;
        let after = &doc.layers[layer].pixels;
        let mut tiles = Vec::new();
        for y in (0..height).step_by(TILE) {
            for x in (0..width).step_by(TILE) {
                let w = TILE.min(width - x);
                let h = TILE.min(height - y);
                if (y..y + h).any(|row| {
                    let start = (row * width + x) * 4;
                    before[start..start + w * 4] != after[start..start + w * 4]
                }) {
                    let mut pixels = Vec::with_capacity(w * h * 4);
                    for row in y..y + h {
                        let start = (row * width + x) * 4;
                        pixels.extend_from_slice(&before[start..start + w * 4]);
                    }
                    tiles.push(TilePatch {
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
        if !tiles.is_empty() {
            self.append(Entry::Tiles(Delta {
                active: doc.active,
                dpi: doc.dpi,
                properties: Vec::new(),
                tiles,
            }));
        }
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
                dpi: doc.dpi,
                properties: Vec::new(),
                tiles: patches,
            }))
        });
        let properties = self.pending_properties.take().and_then(|mut delta| {
            delta
                .properties
                .retain(|(index, before)| *before != LayerProperties::from(&doc.layers[*index]));
            (!delta.properties.is_empty() || delta.dpi != doc.dpi).then_some(Entry::Tiles(delta))
        });
        let entry = stroke.or(properties).or_else(|| {
            self.pending
                .take()
                .and_then(|before| Entry::between(before, doc))
        });
        if let Some(entry) = entry {
            self.append(entry);
        }
    }
    fn append(&mut self, entry: Entry) {
        self.append_with_limit(entry, MAX_HISTORY_BYTES, MAX_STEPS);
    }
    fn append_with_limit(&mut self, entry: Entry, max_bytes: usize, max_steps: usize) {
        self.redo.clear();
        // A single structure snapshot can be larger than the entire history
        // budget. Keep the edit, but don't retain an unbounded undo entry.
        if entry.bytes() > max_bytes {
            self.undo.clear();
            self.limit_warning = true;
            return;
        }
        self.undo.push_back(entry);
        while self.undo.len() > max_steps || self.undo_bytes() > max_bytes {
            self.undo.pop_front();
        }
    }
    fn undo_bytes(&self) -> usize {
        self.undo
            .iter()
            .fold(0usize, |total, entry| total.saturating_add(entry.bytes()))
    }
    pub fn take_limit_warning(&mut self) -> bool {
        std::mem::take(&mut self.limit_warning)
    }
    pub fn counts(&self) -> (usize, usize) {
        (
            self.undo.len()
                + usize::from(
                    self.pending.is_some()
                        || self.pending_properties.is_some()
                        || self.stroke_layer.is_some(),
                ),
            self.redo.len(),
        )
    }
    /// Approximate CPU bytes retained by undo/redo and pending snapshots.
    pub fn memory_bytes(&self) -> usize {
        let entries = self
            .undo
            .iter()
            .chain(&self.redo)
            .fold(0usize, |total, entry| total.saturating_add(entry.bytes()));
        let pending = self
            .pending
            .as_ref()
            .map(Entry::document_bytes)
            .unwrap_or(0);
        let pending_properties = self
            .pending_properties
            .as_ref()
            .map(|delta| {
                delta
                    .tiles
                    .iter()
                    .fold(0usize, |total, tile| {
                        total.saturating_add(tile.pixels.len() + std::mem::size_of::<TilePatch>())
                    })
                    .saturating_add(delta.properties.iter().fold(
                        0usize,
                        |total, (_, properties)| {
                            total
                                    .saturating_add(std::mem::size_of::<LayerProperties>())
                                    .saturating_add(properties.name.len())
                                    .saturating_add(
                                        properties.rulers.len().saturating_mul(
                                            std::mem::size_of::<crate::rulers::Ruler>(),
                                        ),
                                    )
                                    .saturating_add(
                                        properties.effects.iter().fold(0usize, |bytes, effect| {
                                            bytes.saturating_add(effect.bytes())
                                        }),
                                    )
                        },
                    ))
            })
            .unwrap_or(0);
        entries
            .saturating_add(pending)
            .saturating_add(pending_properties)
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
    #[test]
    fn history_budget_rejects_single_oversized_entries() {
        let mut history = History::new();
        let entry = Entry::Structure(Document::new(1, 1, false));
        let limit = entry.bytes() - 1;
        history.append_with_limit(
            Entry::Structure(Document::new(1, 1, false)),
            limit,
            MAX_STEPS,
        );
        assert!(history.undo.is_empty());

        history.append_with_limit(
            Entry::Structure(Document::new(1, 1, false)),
            MAX_HISTORY_BYTES,
            MAX_STEPS,
        );
        assert_eq!(history.undo.len(), 1);
        assert!(history.undo_bytes() <= MAX_HISTORY_BYTES);
    }

    #[test]
    fn document_replacement_moves_previous_pixels_into_history_without_cloning() {
        let mut before = Document::new(3, 2, false);
        before.layers[0].pixels[0] = 91;
        let previous_pixels = before.layers[0].pixels.as_ptr();
        let mut after = Document::new(2, 3, false);
        after.layers[0].pixels[0] = 37;

        let mut history = History::new();
        history.record_replacement(before, &after);

        let Entry::Structure(snapshot) = &history.undo[0] else {
            panic!("dimension changes need a structure entry");
        };
        assert_eq!(snapshot.layers[0].pixels.as_ptr(), previous_pixels);
        let mut current = after;
        assert!(history.undo(&mut current));
        assert_eq!((current.width, current.height), (3, 2));
        assert_eq!(current.layers[0].pixels[0], 91);
        assert!(history.redo(&mut current));
        assert_eq!((current.width, current.height), (2, 3));
        assert_eq!(current.layers[0].pixels[0], 37);
    }

    #[test]
    fn oversized_pending_snapshot_is_not_cloned_and_clears_stale_history() {
        let doc = Document::new(4, 4, false);
        let mut history = History::new();
        history.append(Entry::Structure(Document::new(1, 1, false)));
        history
            .redo
            .push(Entry::Structure(Document::new(1, 1, false)));

        history.push_with_limit(&doc, Entry::document_bytes(&doc) - 1);

        assert!(history.pending.is_none());
        assert!(history.undo.is_empty());
        assert!(history.redo.is_empty());
        assert!(history.take_limit_warning());
        assert!(!history.take_limit_warning());
    }

    #[test]
    fn opacity_history_stores_only_metadata_and_preserves_redo_for_noops() {
        let mut doc = Document::new(3000, 3000, false);
        let pixels = doc.layers[0].pixels.as_ptr();
        let mut history = History::new();
        history.push_layer_properties(&doc);
        assert!(history.pending.is_none());
        assert!(
            history
                .pending_properties
                .as_ref()
                .unwrap()
                .tiles
                .is_empty()
        );
        for step in 1..=100 {
            doc.layers[0].opacity = step as f32 / 200.0;
        }
        history.commit(&doc);
        assert_eq!(history.counts(), (1, 0));
        assert!(history.undo[0].bytes() < 1024);
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[0].opacity, 1.0);
        history.push_layer_properties(&doc);
        doc.layers[0].opacity = 0.2;
        doc.layers[0].opacity = 1.0;
        history.commit(&doc);
        assert_eq!(history.counts(), (0, 1));
        assert!(history.redo(&mut doc));
        assert_eq!(doc.layers[0].opacity, 0.5);
        assert_eq!(doc.layers[0].pixels.as_ptr(), pixels);
    }
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
                "incremental={incremental}: {:?} for 30 strokes; begin/paint/commit={stages:?}; document={} MiB; history={} MiB (estimated retained CPU bytes)",
                start.elapsed(),
                doc.memory_bytes() as f64 / (1024.0 * 1024.0),
                h.memory_bytes() as f64 / (1024.0 * 1024.0),
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
        doc.layers[2].name = "Details".into();
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
