//! Open documents own their pixels, undo history, selection and view.
use super::*;

pub(super) struct Tab {
    pub id: u64,
    pub name: String,
    pub session: Option<Session>,
}
pub(super) struct Session {
    doc: Document,
    history: History,
    selection: selection_ui::State,
    path: Option<PathBuf>,
    dirty: bool,
    zoom: f32,
    pan: Vec2,
    angle: f32,
    mirrored: bool,
    fit: bool,
    rename: String,
}
impl Session {
    fn memory_bytes(&self) -> usize {
        self.doc
            .memory_bytes()
            .saturating_add(self.history.memory_bytes())
            .saturating_add(self.selection.memory_bytes())
    }
}
impl PaintAtlas {
    /// Estimated retained CPU state across tabs, export snapshots/results, and
    /// admitted image workers. Renderer/GPU resources remain separate.
    pub(super) fn retained_document_memory_bytes(&self) -> usize {
        self.documents
            .iter()
            .filter_map(|tab| tab.session.as_ref())
            .fold(0usize, |total, session| {
                total.saturating_add(session.memory_bytes())
            })
            .saturating_add(self.doc.memory_bytes())
            .saturating_add(self.history.memory_bytes())
            .saturating_add(self.selection.memory_bytes())
            .saturating_add(self.picker.retained_cpu_memory_bytes())
            .saturating_add(self.retouch.memory_bytes())
            .saturating_add(
                self.export_dialog
                    .as_ref()
                    .map_or(0, export_ui::Export::retained_cpu_memory_bytes),
            )
            .saturating_add(crate::image_ops::active_image_job_memory_bytes())
    }
    pub(super) fn retained_export_preview_texture_bytes(&self) -> usize {
        self.export_dialog
            .as_ref()
            .map_or(0, export_ui::Export::preview_texture_estimate_bytes)
    }
    pub(super) fn retained_picker_texture_bytes(&self) -> usize {
        self.picker.texture_estimate_bytes()
    }
    pub(super) fn document_title(&self, index: usize) -> String {
        let tab = &self.documents[index];
        let (path, dirty) = if index == self.active_document {
            (&self.path, self.dirty)
        } else {
            let session = tab.session.as_ref().expect("inactive document session");
            (&session.path, session.dirty)
        };
        let name = path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| tab.name.clone());
        format!("{name}{}", if dirty { " •" } else { "" })
    }
    pub(super) fn document_change_allowed(&mut self) -> bool {
        if self.close_motion.active() {
            return false;
        }
        if self.quick_shape.as_ref().is_some_and(|e| e.editing) {
            self.status =
                "Apply QuickShape or restore the original stroke before switching canvases.".into();
            return false;
        }
        self.finish_quickshape(false);
        if self.image_editor.is_some() {
            self.status = "Apply or cancel the image operation before switching canvases.".into();
            return false;
        }
        if (self.gesture.is_some() || self.retouch.busy())
            || self.ruler_ui.editing()
            || self.selection.floating.is_some()
        {
            self.status =
                "Finish the stroke or apply/cancel the transform before switching canvases.".into();
            false
        } else {
            true
        }
    }
    fn stash_document(&mut self) {
        self.commit_name();
        self.history.commit(&self.doc);
        let session = Session {
            doc: std::mem::replace(&mut self.doc, Document::new(16, 16, false)),
            history: std::mem::replace(&mut self.history, History::new()),
            selection: std::mem::take(&mut self.selection),
            path: self.path.take(),
            dirty: self.dirty,
            zoom: self.zoom,
            pan: self.pan,
            angle: self.angle,
            mirrored: self.mirrored,
            fit: self.fit,
            rename: self.rename.clone(),
        };
        self.documents[self.active_document].session = Some(session);
    }
    fn load_document(&mut self, index: usize) {
        let session = self.documents[index]
            .session
            .take()
            .expect("stored document session");
        self.doc = session.doc;
        self.history = session.history;
        self.selection = session.selection;
        self.path = session.path;
        self.dirty = session.dirty;
        self.zoom = session.zoom;
        self.pan = session.pan;
        self.angle = session.angle;
        self.mirrored = session.mirrored;
        self.fit = session.fit;
        self.rename = session.rename;
        self.active_document = index;
        self.invalidate_document_view();
    }
    fn invalidate_document_view(&mut self) {
        self.retouch.clear();
        self.scrubby_zoom.clear();
        self.brush_hud.clear();
        self.quick_shape = None;
        self.quick_shape_toolbar = None;
        self.ruler_ui = super::ruler_ui::State::default();
        self.doc.ruler_copies.clear();
        self.clipboard_motion.cancel_flight();
        self.picker = super::picker_ui::State::default();
        self.fill_reveal.clear();
        // Rebuild GPU resources for the active document only; inactive tabs stay in RAM.
        self.textures.clear();
        self.composite.clear();
        self.doc.damage = None;
        self.refresh = true;
        self.groups_prepared = false;
        self.zoom_animation = None;
        self.fit_animation = None;
        self.smooth_fit = false;
        self.selected_layers.clear();
        self.collapsed_groups.clear();
        self.rotation_target = None;
        self.layer_motion = None;
        self.layer_row_slides.clear();
        self.opacity_edit = false;
    }
    pub(super) fn switch_document(&mut self, index: usize) {
        if index == self.active_document
            || index >= self.documents.len()
            || !self.document_change_allowed()
        {
            return;
        }
        self.stash_document();
        self.load_document(index);
    }
    pub(super) fn add_document(&mut self, doc: Document, path: Option<PathBuf>, dirty: bool) {
        if !self.documents.is_empty() {
            self.stash_document();
        }
        let id = self.next_document_id;
        self.next_document_id += 1;
        self.documents.push(Tab {
            id,
            name: format!("Untitled {id}"),
            session: None,
        });
        self.active_document = self.documents.len() - 1;
        self.reset(doc, path, dirty);
        self.invalidate_document_view();
    }
    pub(super) fn open_document(&mut self, path: PathBuf) -> Result<(), String> {
        if let Some(index) = self.documents.iter().enumerate().position(|(index, tab)| {
            if index == self.active_document {
                self.path.as_ref() == Some(&path)
            } else {
                tab.session
                    .as_ref()
                    .is_some_and(|s| s.path.as_ref() == Some(&path))
            }
        }) {
            self.switch_document(index);
            return Ok(());
        }
        let project = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("patlas"));
        let doc = if project {
            Document::open(&path)?
        } else {
            Document::import(&path)?
        };
        self.add_document(doc, project.then_some(path), !project);
        Ok(())
    }
    pub(super) fn any_dirty_document(&self) -> bool {
        (!self.documents.is_empty() && self.dirty)
            || self
                .documents
                .iter()
                .any(|tab| tab.session.as_ref().is_some_and(|s| s.dirty))
    }
    pub(super) fn close_active_document(&mut self) {
        if self.documents.is_empty() {
            return;
        }
        if self.close_motion.defer(self.dirty) {
            return;
        }
        self.documents.remove(self.active_document);
        // Drop active pixels, masks and history before restoring a neighbour.
        self.reset(Document::new(16, 16, false), None, false);
        self.invalidate_document_view();
        if self.documents.is_empty() {
            self.active_document = 0;
            self.require_initial_canvas();
        } else {
            self.load_document(self.active_document.min(self.documents.len() - 1));
        }
    }
    pub(super) fn continue_closing_documents(&mut self, exit: bool) {
        while !self.documents.is_empty() {
            self.commit_name();
            if self.dirty {
                self.pending = Some(if exit {
                    Action::Close
                } else {
                    Action::CloseAll
                });
                return;
            }
            self.close_active_document();
            if self.close_motion.pending.is_some() {
                self.close_motion.followup = Some(exit);
                return;
            }
        }
        if exit {
            self.allow_close = true;
        }
    }
    pub(super) fn document_canvas_ui(&mut self, ui: &mut egui::Ui) {
        let enabled = (self.gesture.is_none() && !self.retouch.busy())
            && self.selection.floating.is_none()
            && self.pending.is_none()
            && !self.close_motion.active()
            && !self.new_dialog;
        let mut switch = None;
        let mut close = None;
        let mut new = false;
        let header_top = ui.cursor().min.y;
        let header_background = ui.painter().add(egui::Shape::Noop);
        ui.add_enabled_ui(enabled, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("document_tabs")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for index in 0..self.documents.len() {
                            let id = self.documents[index].id;
                            ui.push_id(id, |ui| {
                                let response = ui.selectable_label(
                                    index == self.active_document,
                                    self.document_title(index),
                                );
                                if response.hovered() {
                                    response.clone().on_hover_text(crate::i18n::ui(ui.ctx(), format!(
                                        "Estimated CPU memory (documents, history, eyedropper, export, and image tasks): {:.1} MiB\nEyedropper texture (estimate; backend/GPU): {:.1} MiB\nExport preview texture (estimate; backend/GPU): {:.1} MiB",
                                        self.retained_document_memory_bytes() as f64 / (1024.0 * 1024.0),
                                        self.retained_picker_texture_bytes() as f64 / (1024.0 * 1024.0),
                                        self.retained_export_preview_texture_bytes() as f64 / (1024.0 * 1024.0)
                                    )));
                                }
                                if response.clicked() {
                                    switch = Some(index);
                                }
                                if ui
                                    .small_button("×")
                                    .on_hover_text(crate::i18n::ui(ui.ctx(), "Close canvas · Ctrl+W"))
                                    .clicked()
                                {
                                    close = Some(id);
                                }
                            });
                            ui.separator();
                        }
                        new = ui
                            .small_button("+")
                            .on_hover_text(crate::i18n::ui(ui.ctx(), "New canvas · Ctrl+N"))
                            .clicked();
                    });
                });
        });
        let header_rect = egui::Rect::from_min_max(
            Pos2::new(ui.max_rect().left(), header_top),
            Pos2::new(ui.max_rect().right(), ui.cursor().min.y),
        );
        ui.painter().set(
            header_background,
            egui::Shape::rect_filled(header_rect, 8, ui.visuals().window_fill()),
        );
        if let Some(index) = switch {
            self.switch_document(index);
        }
        if let Some(id) = close {
            self.request(Action::CloseTab(id));
        }
        if new {
            self.request(Action::New);
        }
        ui.separator();
        if !self.startup_canvas_required {
            let rect = ui.available_rect_before_wrap();
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                self.canvas_ui(ui)
            });
        } else {
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_preserves_pixels_history_selection_view_and_path() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(
            Document::new(8, 8, false),
            Some(PathBuf::from("a.patlas")),
            false,
        );
        app.checkpoint();
        app.doc.layers[0].pixels[..4].copy_from_slice(&[255, 0, 0, 255]);
        app.changed();
        let mask = crate::selection::Mask::rectangle(
            8,
            8,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(4.0, 4.0)),
        )
        .unwrap();
        app.selection.mask = Some(std::sync::Arc::new(mask));
        app.doc.selection = app.selection.mask.clone();
        app.zoom = 3.0;
        app.pan = Vec2::new(25.0, -8.0);
        app.angle = 0.7;
        app.mirrored = true;
        app.fit = false;
        app.add_document(
            Document::with_paper(12, 10, Some([200, 210, 220]), 600),
            Some(PathBuf::from("b.patlas")),
            false,
        );
        assert_eq!(app.documents.len(), 2);
        let document_bytes = app.doc.memory_bytes()
            + app.documents[0]
                .session
                .as_ref()
                .unwrap()
                .doc
                .memory_bytes();
        assert!(app.retained_document_memory_bytes() >= document_bytes);
        let retained_bytes = app.retained_document_memory_bytes();
        app.switch_document(0);
        assert!(app.retained_document_memory_bytes() >= document_bytes);
        assert!(retained_bytes > 0);
        app.switch_document(1);
        assert!(!app.dirty);
        assert!(app.selection.mask.is_none());
        assert_eq!(app.history.counts(), (0, 0));
        app.switch_document(0);
        assert_eq!(&app.doc.layers[0].pixels[..4], &[255, 0, 0, 255]);
        assert!(app.dirty && app.selection.mask.is_some());
        assert_eq!(app.path, Some(PathBuf::from("a.patlas")));
        assert_eq!(
            (app.zoom, app.pan, app.angle, app.mirrored, app.fit),
            (3.0, Vec2::new(25.0, -8.0), 0.7, true, false)
        );
        app.undo();
        assert_eq!(&app.doc.layers[0].pixels[..4], &[0, 0, 0, 0]);
        app.switch_document(1);
        assert_eq!((app.doc.width, app.doc.height, app.doc.dpi), (12, 10, 600));
        assert_eq!(app.history.counts(), (0, 0));
        assert!(!app.dirty);
        app.switch_document(0);
        app.redo();
        assert_eq!(&app.doc.layers[0].pixels[..4], &[255, 0, 0, 255]);
    }
    #[test]
    fn open_adds_tabs_and_failure_does_not_replace_existing_documents() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.patlas");
        Document::new(32, 24, true).save(&path).unwrap();
        app.dirty = true;
        app.open_document(path.clone()).unwrap();
        assert_eq!(app.documents.len(), 2);
        assert!(app.any_dirty_document());
        app.switch_document(0);
        app.open_document(path).unwrap();
        assert_eq!(app.documents.len(), 2);
        assert_eq!(app.active_document, 1);
        assert!(
            app.open_document(dir.path().join("missing.patlas"))
                .is_err()
        );
        assert_eq!(app.doc.width, 32);
        assert_eq!(app.documents.len(), 2);
        app.request(Action::New);
        assert!(app.new_dialog && app.pending.is_none());
    }
    #[test]
    fn closing_all_prompts_per_dirty_document_and_reopens_creation_after_last() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, true);
        let first_id = app.documents[0].id;
        app.add_document(Document::new(16, 16, false), None, false);
        app.request(Action::CloseAll);
        assert_eq!(app.documents.len(), 1);
        assert!(matches!(app.pending, Some(Action::CloseAll)));
        assert_eq!(app.documents[0].id, first_id);
        assert!(!app.startup_canvas_required);
        // Cancel leaves the unsaved document available.
        app.pending = None;
        assert!(app.dirty);
        app.request(Action::CloseTab(first_id));
        assert!(matches!(app.pending, Some(Action::CloseTab(id)) if id == first_id));
        let action = app.pending.take().unwrap();
        app.perform(action); // Explicit discard.
        assert!(app.documents.is_empty());
        assert!(app.startup_canvas_required && app.new_dialog);
        assert!(!app.allow_close);
        app.new_width = 64;
        app.new_height = 48;
        app.create_canvas(&ctx);
        assert_eq!(app.documents.len(), 1);
        assert!(!app.startup_canvas_required);
    }
    #[test]
    fn close_all_never_discards_other_unsaved_tabs_without_confirmation() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, true);
        app.add_document(Document::new(16, 16, false), None, true);
        app.request(Action::CloseAll);
        assert_eq!(app.documents.len(), 2);
        let action = app.pending.take().unwrap();
        app.perform(action);
        assert_eq!(app.documents.len(), 1);
        assert!(matches!(app.pending, Some(Action::CloseAll)));
        assert!(app.dirty && !app.startup_canvas_required);
        app.pending = None;
        assert_eq!(app.doc.width, 8);
        assert!(!app.allow_close);
    }

    #[test]
    fn quitting_prompts_for_inactive_documents_and_resumes_after_save() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("saved.patlas");
        app.reset(Document::new(8, 8, false), Some(path.clone()), true);
        app.add_document(Document::new(16, 16, false), None, false);
        app.request(Action::Close);
        assert!(matches!(app.pending, Some(Action::Close)));
        assert!(!app.allow_close);
        assert!(app.save(false));
        let action = app.pending.take().unwrap();
        app.perform(action);
        assert!(app.documents.is_empty() && app.allow_close);
        assert_eq!(Document::open(&path).unwrap().width, 8);
    }
}
