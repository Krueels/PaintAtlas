use super::*;
impl PaintAtlas {
    pub(super) fn copy_selection_to_clipboard(&mut self, merged: bool) {
        match crate::clipboard::capture(&self.doc, self.selection.mask.as_deref(), merged)
            .and_then(|image| self.clipboard.write_image(&image))
        {
            Ok(()) => {
                self.status = format!(
                    "Imagem copiada. Troque de canvas e use {} para colar.",
                    self.settings.shortcuts.label(Shortcut::Paste)
                )
            }
            Err(error) => self.status = error,
        }
    }
    pub(super) fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        match self
            .clipboard
            .read_image()
            .and_then(|image| self.paste_image(ctx, image))
        {
            Ok(()) => {}
            Err(error) => self.status = format!("Erro ao colar: {error}"),
        }
    }
    pub(super) fn paste_image(
        &mut self,
        ctx: &egui::Context,
        image: crate::clipboard::Image,
    ) -> Result<(), String> {
        if self.startup_canvas_required || self.documents.is_empty() {
            image.validate()?;
            let mut doc = Document::new(image.size[0] as u32, image.size[1] as u32, false);
            doc.layers[0].pixels = image.pixels;
            doc.layers[0].name = "Imagem colada".into();
            self.add_document(doc, None, true);
            self.color = Color32::BLACK;
            self.status = "Canvas criado a partir da imagem copiada.".into();
            return Ok(());
        }
        if self.selection.floating.is_some() || self.gesture.is_some() {
            return Err("Aplique ou cancele a transformação atual antes de colar.".into());
        }
        image.validate()?;
        let size = Vec2::new(image.size[0] as f32, image.size[1] as f32);
        let mut scale = (self.doc.width as f32 / size.x)
            .min(self.doc.height as f32 / size.y)
            .min(1.0);
        let mut center = Pos2::new(self.doc.width as f32 / 2.0, self.doc.height as f32 / 2.0);
        if let Some(bounds) = self.last_canvas_bounds
            && !self.fit
        {
            let view = CanvasView {
                center: bounds.center() + self.pan,
                size: Vec2::new(self.doc.width as f32, self.doc.height as f32),
                zoom: self.zoom,
                angle: self.angle,
                mirrored: self.mirrored,
            };
            center = Pos2::from(view.document(bounds.center()));
            let (sin, cos) = self.angle.sin_cos();
            let screen_size = Vec2::new(
                size.x * cos.abs() + size.y * sin.abs(),
                size.x * sin.abs() + size.y * cos.abs(),
            ) * self.zoom;
            scale = scale
                .min(bounds.width() * 0.9 / screen_size.x)
                .min(bounds.height() * 0.9 / screen_size.y);
        }
        let fitted = (size * scale).min(Vec2::new(self.doc.width as f32, self.doc.height as f32));
        center.x = center
            .x
            .clamp(fitted.x / 2.0, self.doc.width as f32 - fitted.x / 2.0);
        center.y = center
            .y
            .clamp(fitted.y / 2.0, self.doc.height as f32 - fitted.y / 2.0);
        let mut min = (center.to_vec2() - fitted / 2.0).round().to_pos2();
        min.x = min.x.clamp(0.0, self.doc.width as f32 - fitted.x);
        min.y = min.y.clamp(0.0, self.doc.height as f32 - fitted.y);
        let bounds = egui::Rect::from_min_size(min, fitted);
        let floating =
            crate::selection::Floating::pasted(&self.doc, image.size, image.pixels, bounds)?;
        self.start_floating(ctx, floating);
        self.status = format!(
            "Imagem pronta para colar em nova camada. Arraste ou redimensione; {} aplica, {} cancela.",
            self.settings.shortcuts.label(Shortcut::Apply),
            self.settings.shortcuts.label(Shortcut::Cancel)
        );
        Ok(())
    }
    pub(super) fn clipboard_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if self.preferences.capturing_shortcut()
            || ctx.wants_keyboard_input()
            || self.pending.is_some()
            || (self.new_dialog && !self.startup_canvas_required)
            || self.gesture.is_some()
            || self.creating_selection()
        {
            return false;
        }
        let (copy, merged, paste) = ctx.input_mut(|input| {
            if self.startup_canvas_required {
                return (
                    false,
                    false,
                    self.settings.shortcuts.consume(input, Shortcut::Paste),
                );
            }
            let merged = self.settings.shortcuts.consume(input, Shortcut::CopyMerged);
            let copy = self.settings.shortcuts.consume(input, Shortcut::Copy);
            let paste = self.settings.shortcuts.consume(input, Shortcut::Paste);
            (copy || merged, merged, paste)
        });
        if !copy && !paste {
            return false;
        }
        if self.selection.floating.is_some() {
            self.status =
                "Aplique ou cancele a transformação atual antes de copiar ou colar.".into();
            return true;
        }
        if copy {
            self.copy_selection_to_clipboard(merged);
        }
        if paste {
            self.paste_from_clipboard(ctx);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(app: &mut PaintAtlas, ctx: &egui::Context, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
    }
    fn image() -> crate::clipboard::Image {
        crate::clipboard::Image {
            size: [2, 2],
            pixels: [200, 20, 40, 128].repeat(4),
        }
    }
    #[test]
    fn startup_paste_creates_one_canvas_with_original_pixels() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.require_initial_canvas();
        let defaults = (app.new_width, app.new_height, app.new_dpi, app.paper_color);
        app.clipboard.write_image(&image()).unwrap();
        command(&mut app, &ctx, vec![egui::Event::Paste(String::new())]);
        assert!(!app.startup_canvas_required && !app.new_dialog);
        assert_eq!(app.documents.len(), 1);
        assert_eq!((app.doc.width, app.doc.height), (2, 2));
        assert_eq!(app.doc.layers.len(), 1);
        assert_eq!(app.doc.layers[0].pixels, image().pixels);
        assert!(app.dirty && app.path.is_none());
        assert!(app.selection.floating.is_none());
        assert_eq!(app.color, Color32::BLACK);
        assert_eq!(
            defaults,
            (app.new_width, app.new_height, app.new_dpi, app.paper_color)
        );
    }
    #[test]
    fn startup_paste_without_image_keeps_creation_dialog() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.require_initial_canvas();
        command(&mut app, &ctx, vec![egui::Event::Paste(String::new())]);
        assert!(app.startup_canvas_required && app.new_dialog);
        assert!(app.documents.is_empty());
        assert!(app.status.starts_with("Erro ao colar:"));
    }
    #[test]
    fn startup_paste_uses_custom_binding() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.require_initial_canvas();
        app.clipboard.write_image(&image()).unwrap();
        let mut binding = crate::shortcuts::Binding::from_modifiers(egui::Modifiers::CTRL);
        binding.key = Some("J".into());
        assert!(
            app.settings
                .shortcuts
                .assign(Shortcut::Paste, 0, binding, true)
        );
        command(&mut app, &ctx, vec![egui::Event::Paste(String::new())]);
        assert!(app.startup_canvas_required);
        command(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::J,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
        );
        assert!(!app.startup_canvas_required);
    }
    #[test]
    fn copy_to_another_canvas_paste_confirm_undo_redo_preserves_source() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, false);
        app.doc.layers[0].pixels[(2 * 8 + 2) * 4..(2 * 8 + 2) * 4 + 4]
            .copy_from_slice(&[200, 20, 40, 128]);
        let mask = crate::selection::Mask::rectangle(
            8,
            8,
            egui::Rect::from_min_size(Pos2::new(2.0, 2.0), Vec2::splat(2.0)),
        )
        .unwrap();
        app.selection.mask = Some(std::sync::Arc::new(mask));
        app.doc.selection = app.selection.mask.clone();
        let original = app.doc.layers[0].pixels.clone();
        command(&mut app, &ctx, vec![egui::Event::Copy]);
        assert_eq!(app.clipboard.read_image().unwrap().size, [2, 2]);
        assert!(!app.dirty);
        app.add_document(Document::with_paper(8, 8, None, 72), None, false);
        command(&mut app, &ctx, vec![egui::Event::Paste(String::new())]);
        assert!(app.selection.floating.as_ref().unwrap().new_layer);
        assert_eq!(app.doc.layers.len(), 1);
        assert!(!app.dirty);
        assert!(app.doc.layers[0].pixels.iter().all(|&v| v == 0));
        command(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        app.history.commit(&app.doc);
        assert_eq!(app.doc.layers.len(), 2);
        assert_eq!(app.doc.layers[1].name, "Imagem colada");
        assert_eq!(
            &app.doc.layers[1].pixels[(3 * 8 + 3) * 4..(3 * 8 + 3) * 4 + 4],
            &[200, 20, 40, 128]
        );
        assert_eq!(app.doc.dpi, 72);
        assert_eq!(
            crate::clipboard::capture(&app.doc, app.selection.mask.as_deref(), false).unwrap(),
            app.clipboard.read_image().unwrap(),
            "copying a pasted translucent image must not apply alpha twice"
        );
        assert_eq!(app.history.counts(), (1, 0));
        app.undo();
        assert_eq!(app.doc.layers.len(), 1);
        app.redo();
        assert_eq!(app.doc.layers.len(), 2);
        app.switch_document(0);
        assert_eq!(app.doc.layers[0].pixels, original);
        assert!(app.selection.mask.is_some());
        assert!(!app.dirty);
    }
    #[test]
    fn cancel_paste_keeps_previous_selection_layers_and_history() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, false);
        app.selection.mask = crate::selection::Mask::rectangle(
            8,
            8,
            egui::Rect::from_min_size(Pos2::new(1.0, 1.0), Vec2::splat(2.0)),
        )
        .map(std::sync::Arc::new);
        app.doc.selection = app.selection.mask.clone();
        let mask = app.selection.mask.clone().unwrap();
        app.doc.layers[0].alpha_lock = true;
        app.doc.layers[0].visible = false;
        app.paste_image(&ctx, image()).unwrap();
        command(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(app.selection.floating.is_none());
        assert!(std::sync::Arc::ptr_eq(
            &mask,
            app.selection.mask.as_ref().unwrap()
        ));
        assert_eq!(app.doc.layers.len(), 1);
        assert!(app.doc.layers[0].alpha_lock && !app.doc.layers[0].visible);
        assert_eq!(app.history.counts(), (0, 0));
        assert!(!app.dirty);
    }
    #[test]
    fn pasted_layer_preview_does_not_replace_existing_upper_layer() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, false);
        app.doc.add_layer();
        app.doc.active = 0;
        app.rename = app.doc.layers[0].name.clone();
        let _ = ctx.run(Default::default(), |ctx| app.sync_layer_textures(ctx));
        app.paste_image(&ctx, image()).unwrap();
        let view = CanvasView {
            center: Pos2::new(50.0, 50.0),
            size: Vec2::splat(8.0),
            zoom: 1.0,
            angle: 0.0,
            mirrored: false,
        };
        let shapes = app.canvas_document_shapes(egui::Rect::EVERYTHING, view);
        let ids: Vec<_> = shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Mesh(m) => Some(m.texture_id),
                _ => None,
            })
            .collect();
        assert_eq!(ids[1], app.textures[0].id());
        assert_eq!(ids.last(), Some(&app.textures[1].id()));
        assert_eq!(ids.len(), 5); // Checker, lower layer, empty base, pasted pixels, upper layer.
        app.apply_transform();
        assert_eq!(app.doc.layers.len(), 3);
        assert_eq!(app.doc.active, 1);
    }
    #[test]
    fn paste_fits_large_images_and_rejects_full_layer_stack_without_mutation() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, false);
        app.paste_image(
            &ctx,
            crate::clipboard::Image {
                size: [32, 16],
                pixels: vec![255; 32 * 16 * 4],
            },
        )
        .unwrap();
        assert_eq!(
            app.selection.floating.as_ref().unwrap().bounds.size(),
            Vec2::new(8.0, 4.0)
        );
        app.cancel_transform();
        for _ in 1..32 {
            app.doc.add_layer();
        }
        assert!(app.paste_image(&ctx, image()).is_err());
        assert_eq!(app.doc.layers.len(), 32);
        assert!(app.selection.floating.is_none());
        assert_eq!(app.history.counts(), (0, 0));
    }
    #[test]
    fn text_fields_keep_copy_and_paste_events_instead_of_pasting_images() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        let mut text = String::from("Nome");
        let id = egui::Id::new("clipboard_text_field");
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(id))
                    .request_focus();
            });
        });
        app.clipboard.write_image(&image()).unwrap();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Paste(String::new()), egui::Event::Copy],
                ..Default::default()
            },
            |ctx| {
                assert!(!app.clipboard_shortcuts(ctx));
                assert!(ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))));
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut text).id(id));
                });
            },
        );
        assert_eq!(text, "Nome");
        assert!(app.selection.floating.is_none());
    }
}
