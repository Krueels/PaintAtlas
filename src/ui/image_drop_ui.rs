use super::*;
impl PaintAtlas {
    pub(super) fn handle_image_drops(&mut self, ctx: &egui::Context) {
        let files = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        if !files.is_empty() {
            if self.image_import.is_some() {
                self.status =
                    "An import is in progress. Finish or cancel it before dropping more images."
                        .into();
            } else if (self.gesture.is_some() || self.retouch.busy())
                || self.selection.floating.is_some()
                || self.creating_selection()
                || self.pending.is_some()
                || self.brush_editor.is_some()
                || self.image_editor.is_some()
                || self.preferences.capturing_shortcut()
                || self.new_dialog && !self.startup_canvas_required
            {
                self.status = "Finish the current edit or dialog before dropping an image.".into();
            } else {
                let target = self.documents.get(self.active_document).map(|t| t.id);
                match crate::image_drop::Job::start(files, target, ctx.clone()) {
                    Ok(job) => {
                        self.image_import = Some(job);
                        self.status = "Loading dropped image…".into();
                    }
                    Err(e) => self.status = format!("Could not import: {e}"),
                }
            }
        }
        let Some(job) = &self.image_import else {
            return;
        };
        if let Some(target) = job.target {
            if !self.documents.iter().any(|t| t.id == target) {
                self.image_import = None;
                self.status = "Import canceled: the destination canvas was closed.".into();
                return;
            }
            if self
                .documents
                .get(self.active_document)
                .is_none_or(|t| t.id != target)
            {
                return;
            }
        }
        if (self.gesture.is_some() || self.retouch.busy())
            || self.selection.floating.is_some()
            || self.creating_selection()
            || self.pending.is_some()
            || self.brush_editor.is_some()
            || self.image_editor.is_some()
            || self.preferences.capturing_shortcut()
            || self.new_dialog && !self.startup_canvas_required
        {
            return;
        }
        let Some(result) = self.image_import.as_mut().and_then(|job| job.poll()) else {
            return;
        };
        let job = self.image_import.as_ref().unwrap();
        let progress = (job.received, job.total);
        // Retain the release position across asynchronous browser downloads.
        let origin = job.origin.unwrap_or_else(|| {
            self.last_canvas_bounds
                .unwrap_or(ctx.content_rect())
                .center()
        });
        if progress.0 >= progress.1 {
            self.image_import = None;
        }
        match result {
            Ok(loaded) => {
                if self.documents.is_empty() || self.startup_canvas_required {
                    let transition = super::startup_ui::Transition::from_drop(
                        ctx,
                        Vec2::new(loaded.image.size[0] as f32, loaded.image.size[1] as f32),
                        origin,
                    );
                    let mut doc = match Document::try_from_rgba(
                        loaded.image.size[0] as u32,
                        loaded.image.size[1] as u32,
                        loaded.image.pixels,
                    ) {
                        Ok(doc) => doc,
                        Err(error) => {
                            self.status = error;
                            return;
                        }
                    };
                    doc.layers[0].name = loaded.name;
                    self.add_document(doc, None, true);
                    self.begin_startup_transition(ctx, transition);
                    self.color = Color32::BLACK;
                    self.status = "Image opened in a new canvas.".into();
                } else {
                    let preview = (!crate::theme::reduced_motion(ctx))
                        .then(|| super::clipboard_motion::thumbnail(ctx, &loaded.image));
                    if let Err(e) = self.paste_image(ctx, loaded.image) {
                        self.status = e;
                    } else {
                        if let Some(f) = &mut self.selection.floating {
                            f.name = Some(loaded.name);
                        }
                        if let Some(preview) = preview {
                            let target = self
                                .selection
                                .floating
                                .as_ref()
                                .and_then(|f| self.clipboard_quad(f.bounds));
                            self.clipboard_motion.dropped(ctx, preview, origin, target);
                        }
                    }
                }
                if let Some(job) = &mut self.image_import
                    && job.target.is_none()
                {
                    job.target = self.documents.get(self.active_document).map(|t| t.id);
                }
                if progress.1 > 1 {
                    self.status
                        .push_str(&format!(" · {}/{} images", progress.0, progress.1));
                }
            }
            Err(e) => self.status = format!("Could not import: {e}"),
        }
    }
    pub(super) fn image_drop_overlay(&mut self, ctx: &egui::Context) {
        let hovered = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovered && !self.references.drop_target(ctx) {
            let rect = self
                .last_canvas_bounds
                .unwrap_or(ctx.content_rect())
                .shrink(8.0);
            let p = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("image_drop_hint"),
            ));
            p.rect_filled(rect, 12.0, crate::theme::accent(ctx).gamma_multiply(0.09));
            p.rect_stroke(
                rect,
                12.0,
                Stroke::new(2.0_f32, crate::theme::accent(ctx)),
                egui::StrokeKind::Inside,
            );
            p.text(
                rect.center_top() + Vec2::new(0.0, 30.0),
                egui::Align2::CENTER_CENTER,
                crate::i18n::ui(
                    ctx,
                    if self.documents.is_empty() {
                        "Drop the image to open a canvas"
                    } else {
                        "Drop the image to insert it into the current canvas"
                    },
                ),
                egui::FontId::proportional(17.0),
                Color32::WHITE,
            );
        }
        if let Some(job) = &self.image_import {
            let target = job
                .target
                .and_then(|id| self.documents.iter().position(|t| t.id == id));
            let inactive = target.is_some_and(|i| i != self.active_document);
            let waiting = self.selection.floating.is_some();
            let position =
                self.last_canvas_bounds.unwrap_or(ctx.content_rect()).min + Vec2::splat(14.0);
            let mut cancel = false;
            let mut focus = false;
            egui::Area::new(egui::Id::new("image_import_progress"))
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.horizontal_wrapped(|ui| {
                            if !inactive && !waiting {
                                ui.spinner();
                            }
                            ui.label(crate::i18n::ui(
                                ui.ctx(),
                                if inactive {
                                    "Import waiting for the destination canvas"
                                } else if waiting {
                                    "Apply or cancel the current image to import the next one"
                                } else {
                                    "Loading image…"
                                },
                            ));
                            if inactive {
                                focus = ui
                                    .button(crate::i18n::ui(ui.ctx(), "Go to canvas"))
                                    .clicked();
                            }
                            cancel = ui
                                .button(crate::i18n::ui(
                                    ui.ctx(),
                                    if waiting {
                                        "Cancel remaining images"
                                    } else {
                                        "Cancel import"
                                    },
                                ))
                                .clicked();
                        });
                    });
                });
            if cancel {
                self.image_import = None;
                self.status = if waiting {
                    "Remaining images canceled. Apply or cancel the current image."
                } else {
                    "Import canceled."
                }
                .into();
            } else if focus && let Some(index) = target {
                self.switch_document(index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(name: &str) -> egui::DroppedFile {
        let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([180, 50, 20, 128]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        egui::DroppedFile {
            name: name.into(),
            mime: "image/png".into(),
            bytes: Some(bytes.into_inner().into()),
            ..Default::default()
        }
    }
    fn step(app: &mut PaintAtlas, ctx: &egui::Context, files: Vec<egui::DroppedFile>) {
        let _ = ctx.run(
            egui::RawInput {
                dropped_files: files,
                ..Default::default()
            },
            |ctx| app.handle_image_drops(ctx),
        );
    }
    fn wait(app: &mut PaintAtlas, ctx: &egui::Context) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.image_import.is_some()
            && app.selection.floating.is_none()
            && std::time::Instant::now() < end
        {
            step(app, ctx, Vec::new());
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    fn dropped_images_preview_cancel_and_apply_as_one_undoable_layer() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(32, 32, true), None, false);
        let original = app.doc.composite();
        step(&mut app, &ctx, vec![file("first.png"), file("second.png")]);
        wait(&mut app, &ctx);
        assert!(app.selection.floating.is_some());
        assert!(app.image_import.is_some());
        assert_eq!(app.doc.composite(), original);
        app.cancel_transform();
        wait(&mut app, &ctx);
        assert!(app.selection.floating.is_some());
        assert!(app.image_import.is_none());
        assert_eq!(app.doc.layers.len(), 2);
        app.apply_transform();
        assert_eq!(app.doc.layers.len(), 3);
        assert!(app.dirty);
        assert_ne!(app.doc.composite(), original);
        app.undo();
        assert_eq!(app.doc.layers.len(), 2);
        assert_eq!(app.doc.composite(), original);
        app.redo();
        assert_eq!(app.doc.layers.len(), 3);
        assert!(app.clipboard.read_image().is_err());
    }
    #[test]
    fn startup_drop_opens_image_without_an_extra_blank_canvas() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.require_initial_canvas();
        step(&mut app, &ctx, vec![file("image.png")]);
        wait(&mut app, &ctx);
        assert_eq!(app.documents.len(), 1);
        assert!(!app.new_dialog && !app.startup_canvas_required);
        assert_eq!((app.doc.width, app.doc.height), (3, 2));
        assert!(app.dirty && app.path.is_none());
        assert_eq!(&app.doc.layers[0].pixels[..4], &[180, 50, 20, 128]);
    }
    #[test]
    fn closing_destination_cancels_import_and_drops_during_transform_do_not_replace_it() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(32, 32, false), None, false);
        step(&mut app, &ctx, vec![file("one.png")]);
        app.close_active_document();
        step(&mut app, &ctx, Vec::new());
        assert!(app.image_import.is_none() && app.documents.is_empty());
        app.add_document(Document::new(32, 32, false), None, false);
        step(&mut app, &ctx, vec![file("one.png")]);
        wait(&mut app, &ctx);
        let pixels = app.selection.floating.as_ref().unwrap().pixels.clone();
        step(&mut app, &ctx, vec![file("two.png")]);
        assert!(app.image_import.is_none());
        assert_eq!(app.selection.floating.as_ref().unwrap().pixels, pixels);
    }
}
