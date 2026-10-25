//! Isolated groups are cached raster composites; children remain ordinary editable layers.
use crate::document::{Document, Layer};
use std::collections::BTreeSet;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GroupEffect {
    pub settings: crate::image_ops::Settings,
    pub mask: Option<std::sync::Arc<Vec<u8>>>,
    pub width: u32,
    pub height: u32,
}
impl GroupEffect {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.mask.as_ref().map_or(0, |m| m.len())
            + self
                .settings
                .filter
                .as_ref()
                .and_then(|f| f.map.as_ref())
                .map_or(0, |m| m.pixels.len())
    }
    fn apply(&self, source: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
        let mut settings = self.settings.clone();
        if let Some(filter) = &mut settings.filter {
            filter.width = width as usize;
            filter.height = height as usize;
        }
        let mask = self.mask.as_ref().map(|values| crate::selection::Mask {
            width,
            height,
            values: values.as_ref().clone(),
            bounds: eframe::egui::Rect::NOTHING,
            edges: Vec::new(),
        });
        crate::image_ops::adjust(
            source,
            mask.as_ref(),
            &settings,
            &std::sync::atomic::AtomicBool::new(false),
        )
    }
}

impl Document {
    pub fn validate_groups(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for layer in &self.layers {
            if layer.effects.len() > 32 || (!layer.effects.is_empty() && layer.group.is_none()) {
                return Err("Estrutura de efeitos inválida.".into());
            }
            for effect in layer.effects.iter() {
                if effect.width != self.width
                    || effect.height != self.height
                    || effect
                        .mask
                        .as_ref()
                        .is_some_and(|m| m.len() != (self.width * self.height) as usize)
                    || !effect.settings.p.iter().all(|v| v.is_finite())
                    || effect.settings.filter.as_ref().is_some_and(|f| {
                        f.width != self.width as usize
                            || f.height != self.height as usize
                            || !f.p.iter().all(|v| v.is_finite())
                            || f.map.as_ref().is_some_and(|m| {
                                m.width == 0
                                    || m.height == 0
                                    || m.width.checked_mul(m.height).and_then(|n| n.checked_mul(4))
                                        != Some(m.pixels.len())
                            })
                    })
                {
                    return Err("Dados de efeito de grupo inválidos.".into());
                }
            }
        }

        for (index, layer) in self.layers.iter().enumerate().rev() {
            if let Some(parent) = layer.parent {
                let Some(end) = self.layers.iter().position(|l| l.group == Some(parent)) else {
                    return Err("Grupo pai inexistente.".into());
                };
                if end <= index || (index + 1..end).any(|i| !self.belongs_to(i, parent)) {
                    return Err("Hierarquia de grupos inválida.".into());
                }
            }
            if let Some(id) = layer.group
                && (id == 0 || id > (1_u64 << 60) || !ids.insert(id))
            {
                return Err("Identificador de grupo inválido.".into());
            }
        }
        Ok(())
    }
    pub fn belongs_to(&self, index: usize, id: u64) -> bool {
        let mut parent = self.layers[index].parent;
        for _ in 0..self.layers.len() {
            let Some(p) = parent else {
                return false;
            };
            if p == id {
                return true;
            }
            parent = self
                .layers
                .iter()
                .find(|l| l.group == Some(p))
                .and_then(|l| l.parent);
        }
        false
    }
    pub fn block_start(&self, index: usize) -> usize {
        self.layers[index].group.map_or(index, |id| {
            (0..index)
                .find(|&i| self.belongs_to(i, id))
                .unwrap_or(index)
        })
    }
    pub fn depth(&self, index: usize) -> usize {
        let mut parent = self.layers[index].parent;
        let mut depth = 0;
        while let Some(id) = parent {
            depth += 1;
            if depth > self.layers.len() {
                break;
            }
            parent = self
                .layers
                .iter()
                .find(|l| l.group == Some(id))
                .and_then(|l| l.parent);
        }
        depth
    }
    pub fn rebuild_groups(&mut self, damage: Option<[usize; 4]>) {
        self.rebuild_groups_after(damage, None);
    }
    pub fn rebuild_groups_after(&mut self, damage: Option<[usize; 4]>, source: Option<usize>) {
        let width = self.width as usize;
        let full = [0, 0, width, self.height as usize];
        // Spatial effects can spread damage outside the child's edited bounds.
        let area = if self.layers.iter().any(|l| !l.effects.is_empty()) {
            full
        } else {
            damage.unwrap_or(full)
        };
        for index in 0..self.layers.len() {
            let Some(id) = self.layers[index].group else {
                continue;
            };
            if source.is_some_and(|s| !self.belongs_to(s, id)) {
                continue;
            }
            let (children, rest) = self.layers.split_at_mut(index);
            let group = &mut rest[0];
            let pixels = &mut group.pixels;
            for y in area[1]..area[3] {
                for x in area[0]..area[2] {
                    let offset = (y * width + x) * 4;
                    let dst = &mut pixels[offset..offset + 4];
                    dst.fill(0);
                    for child in children
                        .iter()
                        .filter(|l| l.parent == Some(id) && l.visible)
                    {
                        child.blend_mode.composite(
                            dst,
                            child.pixels[offset..offset + 4].try_into().unwrap(),
                            child.opacity,
                        );
                    }
                }
            }
            for effect in group.effects.iter() {
                if let Ok(result) = effect.apply(pixels, self.width, self.height) {
                    *pixels = result;
                }
            }
        }
    }
    pub fn group_layers(&mut self, selected: &[usize]) -> Option<usize> {
        if selected.is_empty() || !self.can_add() {
            return None;
        }
        let parent = self.layers[*selected.first()?].parent;
        if selected
            .iter()
            .any(|&i| i >= self.layers.len() || self.layers[i].parent != parent)
        {
            return None;
        }
        let id = self
            .layers
            .iter()
            .filter_map(|l| l.group)
            .max()
            .unwrap_or(0)
            .checked_add(1)?;
        let selected: BTreeSet<_> = selected.iter().copied().collect();
        let mut members = Vec::new();
        for &index in &selected {
            members.extend(self.block_start(index)..=index);
        }
        let positions: BTreeSet<_> = members.into_iter().collect();
        let top = *selected.last()?;
        let insertion = (0..=top).filter(|i| !positions.contains(i)).count();
        let mut children = Vec::new();
        let mut cursor = 0;
        self.layers.retain_mut(|layer| {
            let take = positions.contains(&cursor);
            cursor += 1;
            if take {
                let mut child = layer.clone();
                if selected.contains(&(cursor - 1)) {
                    child.parent = Some(id);
                }
                children.push(child);
            }
            !take
        });
        let group = Layer {
            effects: Default::default(),
            group: Some(id),
            parent,
            name: "Grupo".into(),
            label_color: None,
            blend_mode: Default::default(),
            visible: true,
            opacity: 1.0,
            alpha_lock: false,
            pixels: vec![0; (self.width * self.height * 4) as usize],
        };
        children.push(group);
        let active = insertion + children.len() - 1;
        self.layers.splice(insertion..insertion, children);
        self.active = active;
        self.rebuild_groups(None);
        Some(active)
    }
    pub fn transform_group_children(
        &mut self,
        index: usize,
        transform: &crate::selection::Floating,
        mask: &crate::selection::Mask,
    ) {
        let Some(id) = self.layers[index].group else {
            return;
        };
        let active = self.active;
        for child in 0..index {
            if !self.belongs_to(child, id) || self.layers[child].group.is_some() {
                continue;
            }
            self.active = child;
            if let Ok(mut floating) =
                crate::selection::Floating::for_layer(self, mask, transform.duplicate)
            {
                floating.bounds = transform.bounds;
                floating.angle = transform.angle;
                floating.flip_x = transform.flip_x;
                floating.flip_y = transform.flip_y;
                self.layers[child].pixels = floating.rasterize(self.width, self.height).0;
            }
        }
        self.active = active;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_groups_isolate_opacity_and_preserve_children_on_disk() {
        let mut doc = Document::new(3, 2, true);
        doc.layers[1].pixels = [230, 30, 60, 180].repeat(6);
        doc.add_layer();
        doc.layers[2].pixels = [20, 100, 240, 190].repeat(6);
        doc.layers[2].opacity = 0.4;
        let expected = doc.composite();
        let group = doc.group_layers(&[1, 2]).unwrap();
        assert_eq!(group, 3);
        assert!(
            doc.composite()
                .iter()
                .zip(&expected)
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
        assert!(doc.validate_groups().is_ok());
        doc.layers[group].opacity = 0.3;
        let outer = doc.group_layers(&[group]).unwrap();
        doc.layers[outer].label_color = Some([1, 2, 3]);
        doc.rebuild_groups(None);
        let expected = doc.composite();
        let path =
            std::env::temp_dir().join(format!("paintatlas-group-{}.patlas", std::process::id()));
        doc.save(&path).unwrap();
        let loaded = Document::open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(loaded.composite(), expected);
        assert_eq!(loaded.layers.len(), 5);
        assert_eq!(loaded.layers[outer].label_color, Some([1, 2, 3]));
        assert_eq!(loaded.depth(1), 2);
        // Editing a child updates both isolated ancestors, including partial damage.
        doc.layers[1].pixels[..4].copy_from_slice(&[255, 255, 255, 255]);
        doc.rebuild_groups_after(Some([0, 0, 1, 1]), Some(1));
        assert_ne!(doc.composite(), expected);
        assert_eq!(&doc.composite()[4..], &expected[4..]);
    }
    #[test]
    fn hierarchy_rejects_cycles_missing_parents_and_split_subtrees() {
        let mut doc = Document::new(2, 2, true);
        doc.group_layers(&[1]);
        doc.layers[2].parent = doc.layers[2].group;
        assert!(doc.validate_groups().is_err());
        doc.layers[2].parent = None;
        doc.layers[1].parent = Some(999);
        assert!(doc.validate_groups().is_err());
        doc.layers[1].parent = doc.layers[2].group;
        doc.layers.swap(0, 1);
        assert!(doc.validate_groups().is_err());
    }
    #[test]
    fn group_effect_stack_survives_save_and_reapplies_to_edited_children() {
        let mut doc = Document::new(4, 3, false);
        doc.layers[0].pixels = [30, 100, 180, 255].repeat(12);
        doc.group_layers(&[0]);
        let settings = crate::image_ops::Settings::new(crate::image_ops::Adjustment::Invert);
        let mut mask = vec![0; 12];
        mask[0] = 255;
        std::sync::Arc::make_mut(&mut doc.layers[1].effects).push(GroupEffect {
            settings,
            mask: Some(std::sync::Arc::new(mask)),
            width: 4,
            height: 3,
        });
        doc.rebuild_groups(None);
        let expected = doc.composite();
        assert_eq!(&expected[..4], &[225, 155, 75, 255]);
        assert_eq!(&expected[4..8], &[30, 100, 180, 255]);
        let path = std::env::temp_dir().join(format!(
            "paintatlas-group-effects-{}.patlas",
            std::process::id()
        ));
        doc.save(&path).unwrap();
        let mut loaded = Document::open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(loaded.composite(), expected);
        assert_eq!(loaded.layers.len(), 2);
        assert_eq!(loaded.layers[1].effects.len(), 1);
        loaded.layers[0].pixels[..4].copy_from_slice(&[10, 20, 30, 255]);
        loaded.rebuild_groups_after(Some([0, 0, 1, 1]), Some(0));
        assert_eq!(&loaded.composite()[..4], &[245, 235, 225, 255]);
        // Geometry transforms the effect's selection along with its source layers.
        let mut flipped = crate::image_ops::geometry(
            loaded,
            crate::image_ops::Geometry::FlipHorizontal,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        flipped.rebuild_groups(None);
        assert_eq!(&flipped.composite()[12..16], &[245, 235, 225, 255]);
        assert!(flipped.validate_groups().is_ok());
    }
    #[test]
    fn transforming_a_group_moves_each_child_without_rasterizing_the_hierarchy() {
        let mut doc = Document::new(6, 3, false);
        doc.layers[0].pixels[..4].copy_from_slice(&[255, 0, 0, 255]);
        doc.add_layer();
        doc.layers[1].pixels[4..8].copy_from_slice(&[0, 0, 255, 255]);
        let group = doc.group_layers(&[0, 1]).unwrap();
        let mask = crate::selection::Mask::rectangle(
            6,
            3,
            eframe::egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(2.0, 1.0),
            ),
        )
        .unwrap();
        let mut transform = crate::selection::Floating::new(&doc, &mask, false).unwrap();
        transform.bounds = transform.bounds.translate(eframe::egui::vec2(2.0, 1.0));
        doc.transform_group_children(group, &transform, &mask);
        doc.rebuild_groups(None);
        assert_eq!(doc.layers.len(), 3);
        assert!(doc.validate_groups().is_ok());
        assert_eq!(&doc.layers[0].pixels[32..36], &[255, 0, 0, 255]);
        assert_eq!(&doc.layers[1].pixels[36..40], &[0, 0, 255, 255]);
        assert_eq!(doc.layers[0].pixels[3], 0);
        assert_eq!(doc.layers[1].pixels[7], 0);
    }
}
