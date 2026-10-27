use super::*;

fn clone_layer_fallible(layer: &crate::document::Layer) -> Result<crate::document::Layer, String> {
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(layer.pixels.len())
        .map_err(|error| format!("Não foi possível duplicar os pixels da camada: {error}"))?;
    pixels.extend_from_slice(&layer.pixels);
    let mut name = String::new();
    name.try_reserve_exact(layer.name.len())
        .map_err(|error| format!("Não foi possível duplicar o nome da camada: {error}"))?;
    name.push_str(&layer.name);
    let mut rulers = Vec::new();
    rulers
        .try_reserve_exact(layer.rulers.len())
        .map_err(|error| format!("Não foi possível duplicar as réguas da camada: {error}"))?;
    rulers.extend_from_slice(&layer.rulers);
    Ok(crate::document::Layer {
        rulers,
        effects: layer.effects.clone(),
        group: layer.group,
        parent: layer.parent,
        label_color: layer.label_color,
        blend_mode: layer.blend_mode,
        name,
        visible: layer.visible,
        opacity: layer.opacity,
        pixels,
        alpha_lock: layer.alpha_lock,
    })
}

#[derive(Clone, Copy)]
pub(super) enum LayerAction {
    Duplicate,
    Delete,
    MergeDown,
    MergeSelected,
    Group,
    Rasterize,
    ClearEffects,
    Rename,
    EditRulers,
    RulerScope(crate::rulers::Scope),
    RulerVisible(bool),
    RemoveRulers,
    Label(Option<[u8; 3]>),
}

impl PaintAtlas {
    fn can_merge_down(&self, index: usize) -> bool {
        self.doc
            .block_start(index)
            .checked_sub(1)
            .is_some_and(|bottom| {
                self.doc.layers[index].parent == self.doc.layers[bottom].parent
                    && self.doc.layers[index].visible
                    && self.doc.layers[bottom].visible
                    && self.doc.layers[index].blend_mode == crate::blend::BlendMode::Normal
                    && self.doc.layers[bottom].blend_mode == crate::blend::BlendMode::Normal
            })
    }

    pub(super) fn layer_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
    ) -> Option<LayerAction> {
        let mut action = None;
        let available = self.selection.floating.is_none() && self.gesture.is_none();
        ui.add_enabled_ui(available, |ui| {
            if ui.button(self.settings.shortcuts.menu(Shortcut::LayerGroup, "Agrupar selecionadas")).clicked() { action = Some(LayerAction::Group); }
            if ui.add_enabled(self.selected_layer_indices().len() > 1, egui::Button::new(self.settings.shortcuts.menu(Shortcut::LayerMerge, "Mesclar selecionadas"))).clicked() { action = Some(LayerAction::MergeSelected); }
            if !self.doc.layers[index].rulers.is_empty() {
                ui.menu_button("Réguas da camada", |ui| {
                    if ui.button("Editar réguas").clicked() { action = Some(LayerAction::EditRulers); }
                    for scope in [crate::rulers::Scope::Layer, crate::rulers::Scope::Folder, crate::rulers::Scope::All] {
                        if ui.button(scope.label()).clicked() { action = Some(LayerAction::RulerScope(scope)); }
                    }
                    let visible = self.doc.layers[index].rulers.iter().any(|r| r.visible);
                    if ui.button(if visible { "Ocultar réguas" } else { "Mostrar réguas" }).clicked() { action = Some(LayerAction::RulerVisible(!visible)); }
                    if ui.button("Excluir réguas").clicked() { action = Some(LayerAction::RemoveRulers); }
                });
            }
            if ui.button("Renomear…").clicked() {
                action = Some(LayerAction::Rename);
            }
            if ui.add_enabled(self.doc.can_add(), egui::Button::new("Duplicar selecionadas / grupo")).clicked() {
                action = Some(LayerAction::Duplicate);
            }
            if ui.add_enabled(self.can_merge_down(index), egui::Button::new("Merge Down"))
                .on_hover_text("Une duas camadas visíveis no modo Normal, preservando sua aparência e transparência.")
                .on_disabled_hover_text("É preciso ter uma camada abaixo e ambas devem estar visíveis no modo Normal.")
                .clicked() {
                action = Some(LayerAction::MergeDown);
            }
            if ui.add_enabled(!self.doc.layers[index].effects.is_empty(), egui::Button::new("Remove Group Filters and Adjustments")).clicked() { action = Some(LayerAction::ClearEffects); }
            if ui.add_enabled(self.doc.layers[index].group.is_some(), egui::Button::new("Rasterize Group")).on_hover_text("Permanently merges child layers. Use Undo to restore them.").clicked() { action = Some(LayerAction::Rasterize); }
            if ui.add_enabled(self.doc.layers.len() > 1, egui::Button::new("Excluir selecionadas / grupo")).clicked() {
                action = Some(LayerAction::Delete);
            }
            ui.separator();
            ui.label("Cor de organização");
            ui.horizontal(|ui| {
                if ui.small_button("×").on_hover_text("No color").clicked() {
                    action = Some(LayerAction::Label(None));
                }
                for color in [
                    [239, 100, 112], [240, 166, 88], [237, 208, 109],
                    [117, 198, 145], [96, 177, 227], [170, 137, 226], [224, 142, 196],
                ] {
                    let (rect, response) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                    ui.painter().rect_filled(rect.shrink(2.0), 5.0, Color32::from_rgb(color[0], color[1], color[2]));
                    if self.doc.layers[index].label_color == Some(color) {
                        ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
                    }
                    if response.clicked() {
                        action = Some(LayerAction::Label(Some(color)));
                    }
                }
            });
        });
        if action.is_some() {
            ui.close();
        }
        action
    }

    pub(super) fn selected_layer_indices(&self) -> Vec<usize> {
        let mut chosen = self.selected_layers.clone();
        chosen.insert(self.doc.active);
        chosen
            .into_iter()
            .filter(|&index| index < self.doc.layers.len())
            .filter(|&index| {
                !self
                    .selected_layers
                    .iter()
                    .copied()
                    .chain([self.doc.active])
                    .any(|other| {
                        other != index
                            && other < self.doc.layers.len()
                            && self.doc.layers[other]
                                .group
                                .is_some_and(|id| self.doc.belongs_to(index, id))
                    })
            })
            .collect()
    }
    pub(super) fn select_layer(&mut self, index: usize, toggle: bool, range: bool) {
        self.commit_name();
        if range {
            self.selected_layers
                .extend(self.doc.active.min(index)..=self.doc.active.max(index));
        } else if toggle {
            self.selected_layers.insert(self.doc.active);
            if !self.selected_layers.remove(&index) {
                self.selected_layers.insert(index);
            }
            if !self.selected_layers.contains(&index) {
                self.doc.active = self.selected_layers.last().copied().unwrap_or(index);
                self.rename = self.doc.layers[self.doc.active].name.clone();
                return;
            }
        } else {
            self.selected_layers.clear();
            self.selected_layers.insert(index);
        }
        self.doc.active = index;
        self.rename = self.doc.layers[index].name.clone();
    }
    pub(super) fn apply_layer_action(&mut self, index: usize, action: LayerAction) {
        if index >= self.doc.layers.len()
            || self.selection.floating.is_some()
            || self.gesture.is_some()
            || self.image_editor.is_some()
            || self.ruler_ui.editing()
        {
            return;
        }
        self.commit_name();
        self.doc.active = index;
        let mut selected = self.selected_layer_indices();
        if matches!(action, LayerAction::MergeDown) {
            if !self.can_merge_down(index) {
                return;
            }
            selected = vec![self.doc.block_start(index) - 1, index];
        }
        match action {
            LayerAction::EditRulers => {
                self.ruler_ui.selected = Some(0);
                self.select_tool(Tool::Ruler);
            }
            LayerAction::RulerScope(_)
            | LayerAction::RulerVisible(_)
            | LayerAction::RemoveRulers => {
                self.history.push_layer_properties(&self.doc);
                match action {
                    LayerAction::RulerScope(scope) => {
                        for r in &mut self.doc.layers[index].rulers {
                            r.scope = scope;
                        }
                    }
                    LayerAction::RulerVisible(visible) => {
                        for r in &mut self.doc.layers[index].rulers {
                            r.visible = visible;
                        }
                    }
                    LayerAction::RemoveRulers => {
                        self.doc.layers[index].rulers.clear();
                        self.ruler_ui.selected = None;
                    }
                    _ => unreachable!(),
                }
                self.dirty = true;
            }
            LayerAction::Rename => self.rename_layer_focus = true,
            LayerAction::Label(color) => {
                self.history.push_layers_properties(&self.doc, &selected);
                for &i in &selected {
                    self.doc.layers[i].label_color = color;
                }
                self.history.commit(&self.doc);
                self.dirty = true;
            }
            LayerAction::ClearEffects => {
                if self.doc.layers[index].effects.is_empty() {
                    return;
                }
                self.checkpoint();
                self.doc.layers[index].effects = Default::default();
                self.changed();
            }
            LayerAction::Group => {
                if !self.doc.can_add()
                    || selected
                        .iter()
                        .any(|&i| self.doc.layers[i].parent != self.doc.layers[index].parent)
                {
                    self.status =
                        "Selecione camadas do mesmo nível; verifique também o limite de memória."
                            .into();
                    return;
                }
                self.checkpoint();
                self.doc.group_layers(&selected);
                self.changed();
                self.selected_layers.clear();
            }
            LayerAction::Duplicate => {
                let mut positions = std::collections::BTreeSet::new();
                for &i in &selected {
                    positions.extend(self.doc.block_start(i)..=i);
                }
                let extra = positions.iter().try_fold(0usize, |total, &i| {
                    let layer_bytes = self.doc.layers[i]
                        .effects
                        .iter()
                        .fold(self.doc.layers[i].pixels.len(), |bytes, effect| {
                            bytes.saturating_add(effect.bytes())
                        });
                    total.checked_add(layer_bytes)
                });
                if self
                    .doc
                    .layers
                    .len()
                    .checked_add(positions.len())
                    .is_none_or(|count| count > 32)
                    || extra
                        .and_then(|bytes| self.doc.bytes().checked_add(bytes))
                        .is_none_or(|bytes| bytes > 256 * 1024 * 1024)
                {
                    self.status = "Duplicação excede o limite de camadas ou memória.".into();
                    return;
                }
                let mut prepared = Vec::new();
                if prepared.try_reserve_exact(selected.len()).is_err() {
                    self.status =
                        "Não foi possível reservar memória para duplicar as camadas.".into();
                    return;
                }
                for &i in selected.iter().rev() {
                    let start = self.doc.block_start(i);
                    let count = i - start + 1;
                    let mut copies = Vec::new();
                    if let Err(error) = copies.try_reserve_exact(count) {
                        self.status =
                            format!("Não foi possível reservar as camadas duplicadas: {error}");
                        return;
                    }
                    for layer in &self.doc.layers[start..=i] {
                        match clone_layer_fallible(layer) {
                            Ok(copy) => copies.push(copy),
                            Err(error) => {
                                self.status = error;
                                return;
                            }
                        }
                    }
                    prepared.push((i, copies));
                }
                if self.doc.layers.try_reserve(positions.len()).is_err() {
                    self.status =
                        "Não foi possível reservar espaço para as camadas duplicadas.".into();
                    return;
                }
                self.checkpoint();
                // Duplicate each subtree beside its source, retaining the hierarchy.
                let mut next_id = self
                    .doc
                    .layers
                    .iter()
                    .filter_map(|l| l.group)
                    .max()
                    .unwrap_or(0)
                    + 1;
                for (i, mut copies) in prepared {
                    let mut ids = std::collections::BTreeMap::new();
                    for l in &copies {
                        if let Some(id) = l.group {
                            ids.insert(id, next_id);
                            next_id += 1;
                        }
                    }
                    for l in &mut copies {
                        l.group = l.group.map(|id| ids[&id]);
                        l.parent = l.parent.map(|id| ids.get(&id).copied().unwrap_or(id));
                    }
                    let last = copies.last_mut().unwrap();
                    last.name = format!("{} cópia", last.name.chars().take(90).collect::<String>());
                    self.doc.active = i + copies.len();
                    for (offset, copy) in copies.drain(..).enumerate() {
                        self.doc.layers.insert(i + 1 + offset, copy);
                    }
                }
                self.selected_layers.clear();
                self.changed();
            }
            LayerAction::Delete => {
                let mut positions = std::collections::BTreeSet::new();
                for &i in &selected {
                    positions.extend(self.doc.block_start(i)..=i);
                }
                if positions.len() == self.doc.layers.len() {
                    return;
                }
                self.checkpoint();
                for &i in positions.iter().rev() {
                    self.doc.layers.remove(i);
                }
                self.doc.active = index.min(self.doc.layers.len() - 1);
                self.selected_layers.clear();
                self.changed();
            }
            LayerAction::MergeSelected | LayerAction::MergeDown | LayerAction::Rasterize => {
                if matches!(action, LayerAction::Rasterize) {
                    selected = vec![index];
                }
                if selected.len() < 2 && !matches!(action, LayerAction::Rasterize) {
                    return;
                }
                let parent = self.doc.layers[selected[0]].parent;
                if selected
                    .iter()
                    .any(|&i| self.doc.layers[i].parent != parent)
                {
                    self.status = "Para mesclar, selecione camadas do mesmo nível de grupo.".into();
                    return;
                }
                let Some(pixel_count) = (self.doc.width as usize)
                    .checked_mul(self.doc.height as usize)
                    .and_then(|count| count.checked_mul(4))
                else {
                    self.status = "A imagem é grande demais para mesclar camadas.".into();
                    return;
                };
                let mut pixels = Vec::new();
                if let Err(error) = pixels.try_reserve_exact(pixel_count) {
                    self.status =
                        format!("Não foi possível reservar memória para mesclar: {error}");
                    return;
                }
                pixels.resize(pixel_count, 0);
                for &i in &selected {
                    let layer = &self.doc.layers[i];
                    if !layer.visible {
                        continue;
                    }
                    for (dst, src) in pixels
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(layer.pixels.as_chunks::<4>().0)
                    {
                        layer.blend_mode.composite(dst, *src, layer.opacity);
                    }
                }
                self.checkpoint();
                let top = *selected.last().unwrap();
                let mut result = self.doc.layers[top].clone();
                result.effects = Default::default();
                result.group = None;
                result.parent = parent;
                result.pixels = pixels;
                result.opacity = 1.0;
                result.blend_mode = Default::default();
                result.visible = true;
                result.alpha_lock = false;
                let mut positions = std::collections::BTreeSet::new();
                for &i in &selected {
                    positions.extend(self.doc.block_start(i)..=i);
                }
                let insertion = (0..=top).filter(|i| !positions.contains(i)).count();
                for &i in positions.iter().rev() {
                    self.doc.layers.remove(i);
                }
                self.doc.layers.insert(insertion, result);
                self.doc.active = insertion;
                self.selected_layers.clear();
                self.changed();
            }
        }
        self.rename = self.doc.layers[self.doc.active].name.clone();
        self.opacity_edit = false;
        self.layer_motion = None;
        self.layer_row_slides.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_menu_actions_preserve_pixels_labels_and_undo() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(4, 4, false), None, false);
        app.doc.layers[0].pixels = [100, 150, 200, 190].repeat(16);
        app.doc.layers[0].opacity = 0.6;
        app.doc.layers[0].rulers.push(crate::rulers::Ruler::new(
            crate::rulers::Kind::Symmetry,
            4,
            4,
        ));
        app.apply_layer_action(0, LayerAction::Label(Some([239, 100, 112])));
        app.undo();
        assert_eq!(app.doc.layers[0].label_color, None);
        app.redo();
        assert_eq!(app.doc.layers[0].label_color, Some([239, 100, 112]));
        app.apply_layer_action(0, LayerAction::Duplicate);
        app.history.commit(&app.doc);
        assert_eq!(app.doc.layers.len(), 2);
        assert_eq!(app.doc.layers[1].label_color, app.doc.layers[0].label_color);
        assert_eq!(app.doc.layers[1].rulers, app.doc.layers[0].rulers);
        app.doc.layers[1].rulers[0].lines = 4;
        assert_eq!(app.doc.layers[0].rulers[0].lines, 2);
        app.doc.layers[1].pixels = [200, 50, 80, 130].repeat(16);
        app.doc.layers[1].opacity = 0.7;
        let expected = app.doc.composite();
        app.apply_layer_action(1, LayerAction::MergeDown);
        app.history.commit(&app.doc);
        assert_eq!(app.doc.layers.len(), 1);
        assert_eq!(app.doc.layers[0].opacity, 1.0);
        for (actual, expected) in app.doc.composite().iter().zip(&expected) {
            assert!(actual.abs_diff(*expected) <= 1);
        }
        app.undo();
        assert_eq!(app.doc.layers.len(), 2);
        assert_eq!(app.doc.composite(), expected);
        app.redo();
        assert_eq!(app.doc.layers.len(), 1);
        app.apply_layer_action(0, LayerAction::Delete);
        assert_eq!(app.doc.layers.len(), 1);
        app.apply_layer_action(0, LayerAction::Rename);
        assert!(app.rename_layer_focus);
    }

    #[test]
    fn merge_rejects_invisible_layers_and_backdrop_dependent_modes() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(4, 4, true), None, false);
        app.doc.layers[1].blend_mode = crate::blend::BlendMode::Multiply;
        app.apply_layer_action(1, LayerAction::MergeDown);
        assert_eq!(app.doc.layers.len(), 2);
        app.doc.layers[1].blend_mode = crate::blend::BlendMode::Normal;
        app.doc.layers[0].visible = false;
        app.apply_layer_action(1, LayerAction::MergeDown);
        assert_eq!(app.doc.layers.len(), 2);
    }
    #[test]
    fn multiselection_group_duplicate_merge_and_history_keep_hierarchy() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(4, 4, true), None, false);
        app.doc.layers[1].pixels = [200, 20, 50, 160].repeat(16);
        app.doc.add_layer();
        app.doc.layers[2].pixels = [10, 80, 230, 130].repeat(16);
        let expected = app.doc.composite();
        app.select_layer(1, false, false);
        app.select_layer(2, true, false);
        assert_eq!(app.selected_layer_indices(), vec![1, 2]);
        app.apply_layer_action(2, LayerAction::Group);
        assert_eq!(app.doc.layers.len(), 4);
        assert!(
            app.doc
                .composite()
                .iter()
                .zip(&expected)
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
        let grouped = app.doc.composite();
        app.history.commit(&app.doc);
        app.apply_layer_action(3, LayerAction::Label(Some([20, 80, 220])));
        app.apply_layer_action(3, LayerAction::Duplicate);
        app.history.commit(&app.doc);
        assert_eq!(app.doc.layers.len(), 7);
        assert!(app.doc.validate_groups().is_ok());
        assert_ne!(app.doc.layers[3].group, app.doc.layers[6].group);
        assert_eq!(app.doc.layers[6].label_color, Some([20, 80, 220]));
        app.undo();
        assert_eq!(app.doc.layers.len(), 4);
        assert_eq!(app.doc.composite(), grouped);
        app.undo();
        app.undo();
        assert_eq!(app.doc.layers.len(), 3);
        app.select_layer(1, false, false);
        app.select_layer(2, true, false);
        app.apply_layer_action(2, LayerAction::MergeSelected);
        app.history.commit(&app.doc);
        assert_eq!(app.doc.layers.len(), 2);
        assert!(
            app.doc
                .composite()
                .iter()
                .zip(&expected)
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
        app.undo();
        assert_eq!(app.doc.layers.len(), 3);
        app.redo();
        assert_eq!(app.doc.layers.len(), 2);
    }
    #[test]
    fn moving_a_group_keeps_children_and_inserting_a_layer_adopts_group() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(4, 4, true), None, false);
        app.doc.group_layers(&[1]);
        let id = app.doc.layers[2].group;
        app.doc.add_layer();
        assert_eq!(app.doc.layers[2].parent, id);
        assert!(app.doc.validate_groups().is_ok());
        app.reorder_layer(3, 0);
        assert!(app.doc.validate_groups().is_ok());
        assert_eq!(app.doc.layers[2].group, id);
        assert_eq!(app.doc.layers[0].parent, id);
    }
    #[test]
    fn child_pixel_damage_refreshes_group_composite_and_both_gpu_textures() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, false);
        let group = app.doc.group_layers(&[0]).unwrap();
        app.doc.active = 0;
        let _ = ctx.run(Default::default(), |ctx| app.sync_layer_textures(ctx));
        let child_texture = app.textures[0].id();
        let group_texture = app.textures[group].id();
        app.doc.layers[0].pixels[..4].copy_from_slice(&[200, 20, 40, 255]);
        app.doc.damage = Some([0, 0, 1, 1]);
        app.refresh = true;
        let output = ctx.run(Default::default(), |ctx| app.sync_layer_textures(ctx));
        assert_eq!(&app.doc.layers[group].pixels[..4], &[200, 20, 40, 255]);
        for id in [child_texture, group_texture] {
            assert!(
                output
                    .textures_delta
                    .set
                    .iter()
                    .any(|(texture, delta)| *texture == id
                        && delta.pos == Some([0, 0])
                        && delta.image.size() == [1, 1])
            );
        }
    }
    #[test]
    fn undo_restores_labels_on_every_selected_layer() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(4, 4, true), None, false);
        let pixels = app.doc.layers[0].pixels.as_ptr();
        app.select_layer(0, false, false);
        app.select_layer(1, true, false);
        app.apply_layer_action(1, LayerAction::Label(Some([20, 80, 220])));
        assert!(
            app.doc
                .layers
                .iter()
                .all(|l| l.label_color == Some([20, 80, 220]))
        );
        app.undo();
        assert!(app.doc.layers.iter().all(|l| l.label_color.is_none()));
        app.redo();
        assert!(
            app.doc
                .layers
                .iter()
                .all(|l| l.label_color == Some([20, 80, 220]))
        );
        assert_eq!(app.doc.layers[0].pixels.as_ptr(), pixels);
    }
}
