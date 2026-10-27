//! Sub View references persist only source paths and view transforms, never pixels.
use eframe::egui::{self, Color32, Pos2, Rect, Sense, TextureHandle, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub path: PathBuf,
    pub zoom: f32,
    pub pan: [f32; 2],
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            zoom: 1.0,
            pan: [0.0; 2],
        }
    }
}
impl Entry {
    fn zoom_at(&mut self, factor: f32, anchor: Vec2) {
        let zoom = (self.zoom * factor).clamp(0.01, 256.0);
        let pan = anchor - (anchor - Vec2::from(self.pan)) * (zoom / self.zoom);
        self.pan = pan.clamp(Vec2::splat(-1e6), Vec2::splat(1e6)).into();
        self.zoom = zoom;
    }
    fn fit(&mut self) {
        self.zoom = 1.0;
        self.pan = [0.0; 2];
    }
}
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub images: Vec<Entry>,
    pub selected: usize,
}
impl Library {
    pub fn valid(&self) -> bool {
        (self.images.is_empty() && self.selected == 0 || self.selected < self.images.len())
            && self.images.iter().all(|e| {
                !e.path.as_os_str().is_empty()
                    && e.zoom.is_finite()
                    && (0.01..=256.0).contains(&e.zoom)
                    && e.pan.iter().all(|p| p.is_finite() && p.abs() <= 1e6)
            })
    }
    pub fn add(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> bool {
        let mut changed = false;
        for path in paths {
            if !supported(&path) {
                continue;
            }
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if let Some(i) = self.images.iter().position(|e| e.path == path) {
                changed |= self.selected != i;
                self.selected = i;
            } else {
                self.selected = self.images.len();
                self.images.push(Entry {
                    path,
                    ..Default::default()
                });
                changed = true;
            }
        }
        changed
    }
    fn remove(&mut self) {
        if self.selected < self.images.len() {
            self.images.remove(self.selected);
        }
        self.selected = self.selected.min(self.images.len().saturating_sub(1));
    }
}
fn supported(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
        )
    })
}
fn picker() -> rfd::FileDialog {
    rfd::FileDialog::new()
        .set_title("Reference images — link originals")
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
}
struct Loaded {
    image: egui::ColorImage,
    size: Vec2,
}
fn load(path: &Path) -> Result<Loaded, String> {
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| format!("Original unavailable: {e}"))?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| format!("Could not display the original: {e}"))?;
    let size = Vec2::new(image.width() as f32, image.height() as f32);
    let preview = if image.width().max(image.height()) > 4096 {
        image.thumbnail(4096, 4096)
    } else {
        image
    };
    let pixels = preview.into_rgba8();
    Ok(Loaded {
        image: egui::ColorImage::from_rgba_unmultiplied(
            [pixels.width() as usize, pixels.height() as usize],
            &pixels,
        ),
        size,
    })
}
type Preview = Result<(TextureHandle, Vec2), String>;
type LoadJob = (PathBuf, Receiver<Result<Loaded, String>>);
enum Confirmation {
    Remove { index: usize, path: PathBuf },
    Clear,
}
#[derive(Default)]
pub struct State {
    cached: Option<(PathBuf, Preview)>,
    job: Option<LoadJob>,
    bounds: Option<(egui::ViewportId, Rect, egui::LayerId, u64)>,
    notice: String,
    confirmation: Option<Confirmation>,
}
impl State {
    pub fn ready(&self) -> bool {
        self.cached.is_some() && self.job.is_none()
    }
    pub fn drop_target(&self, ctx: &egui::Context) -> bool {
        self.bounds.is_some_and(|(viewport, bounds, layer, frame)| {
            viewport == ctx.viewport_id()
                && ctx.cumulative_frame_nr().saturating_sub(frame) <= 1
                && ctx
                    .input(|i| i.pointer.latest_pos())
                    .is_some_and(|p| bounds.contains(p) && ctx.layer_id_at(p) == Some(layer))
        })
    }
    pub fn take_drop(&mut self, ctx: &egui::Context, library: &mut Library) -> bool {
        if !self.drop_target(ctx) {
            return false;
        }
        let files = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        if files.is_empty() {
            return false;
        }
        let paths: Vec<_> = files
            .into_iter()
            .filter_map(|f| f.path)
            .filter(|p| supported(p))
            .collect();
        self.notice = if paths.is_empty() {
            "Save the image to your computer and link the original file.".into()
        } else {
            String::new()
        };
        library.add(paths)
    }
    fn poll(&mut self, ctx: &egui::Context, path: Option<&Path>) {
        if self
            .cached
            .as_ref()
            .is_some_and(|(p, _)| Some(p.as_path()) != path)
        {
            self.cached = None;
        }
        if let Some((job_path, rx)) = &self.job {
            match rx.try_recv() {
                Ok(result) => {
                    if Some(job_path.as_path()) == path {
                        self.cached = Some((
                            job_path.clone(),
                            result.map(|l| {
                                (
                                    ctx.load_texture(
                                        "reference_preview",
                                        l.image,
                                        egui::TextureOptions::LINEAR,
                                    ),
                                    l.size,
                                )
                            }),
                        ));
                    }
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    if Some(job_path.as_path()) == path {
                        self.cached = Some((
                            job_path.clone(),
                            Err("Could not load the reference. Try Reload.".into()),
                        ));
                    }
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.cached.is_none()
            && self.job.is_none()
            && let Some(path) = path
        {
            let path = path.to_owned();
            let worker_path = path.clone();
            let (tx, rx) = mpsc::channel();
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(load(&worker_path));
                repaint.request_repaint();
            });
            self.job = Some((path, rx));
        }
        if self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(25));
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, library: &mut Library) -> bool {
        self.bounds = Some((
            ui.ctx().viewport_id(),
            ui.max_rect(),
            ui.layer_id(),
            ui.ctx().cumulative_frame_nr(),
        ));
        let mut changed = self.take_drop(ui.ctx(), library);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(crate::i18n::ui(ui.ctx(), "Add…"))
                .on_hover_text(crate::i18n::ui(
                    ui.ctx(),
                    "Links local images without copying them into the project",
                ))
                .clicked()
                && let Some(paths) = picker().pick_files()
            {
                changed |= library.add(paths);
            }
            if ui
                .add_enabled(!library.images.is_empty(), egui::Button::new("‹"))
                .on_hover_text(crate::i18n::ui(ui.ctx(), "Previous reference"))
                .clicked()
            {
                library.selected =
                    (library.selected + library.images.len() - 1) % library.images.len();
                changed = true;
            }
            if ui
                .add_enabled(!library.images.is_empty(), egui::Button::new("›"))
                .on_hover_text(crate::i18n::ui(ui.ctx(), "Next reference"))
                .clicked()
            {
                library.selected = (library.selected + 1) % library.images.len();
                changed = true;
            }
            ui.menu_button("•••", |ui| {
                if ui
                    .button(crate::i18n::ui(ui.ctx(), "Reload original"))
                    .clicked()
                {
                    self.cached = None;
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !library.images.is_empty(),
                        egui::Button::new(crate::i18n::ui(ui.ctx(), "Locate original…")),
                    )
                    .clicked()
                {
                    if let Some(path) = picker().pick_file() {
                        let entry = &mut library.images[library.selected];
                        entry.path = path;
                        entry.fit();
                        self.cached = None;
                        changed = true;
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !library.images.is_empty(),
                        egui::Button::new(crate::i18n::ui(ui.ctx(), "Remove from list")),
                    )
                    .on_hover_text(crate::i18n::ui(
                        ui.ctx(),
                        "Does not delete the original file",
                    ))
                    .clicked()
                {
                    self.confirmation = Some(Confirmation::Remove {
                        index: library.selected,
                        path: library.images[library.selected].path.clone(),
                    });
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !library.images.is_empty(),
                        egui::Button::new(crate::i18n::ui(ui.ctx(), "Clear list")),
                    )
                    .clicked()
                {
                    self.confirmation = Some(Confirmation::Clear);
                    ui.close();
                }
            });
        });
        if !library.images.is_empty() {
            let selected = library.selected;
            egui::ComboBox::from_id_salt("reference_list")
                .wrap_mode(egui::TextWrapMode::Truncate)
                .width((ui.available_width() - 12.0).max(32.0))
                .selected_text(crate::i18n::ui(
                    ui.ctx(),
                    format!(
                        "{}/{} · {}",
                        selected + 1,
                        library.images.len(),
                        name(&library.images[selected].path)
                    ),
                ))
                .show_ui(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(240.0).show_rows(
                        ui,
                        ui.text_style_height(&egui::TextStyle::Body) + 6.0,
                        library.images.len(),
                        |ui, rows| {
                            for i in rows {
                                changed |= ui
                                    .selectable_value(
                                        &mut library.selected,
                                        i,
                                        name(&library.images[i].path),
                                    )
                                    .on_hover_text(library.images[i].path.display().to_string())
                                    .changed();
                            }
                        },
                    );
                });
        }
        self.poll(
            ui.ctx(),
            library
                .images
                .get(library.selected)
                .map(|e| e.path.as_path()),
        );
        let size = self
            .cached
            .as_ref()
            .and_then(|(_, r)| r.as_ref().ok())
            .map(|(_, size)| *size);
        let mut zoom_factor = 1.0;
        let mut fit = false;
        let mut actual = false;
        ui.horizontal_wrapped(|ui| {
            fit = ui.button(crate::i18n::ui(ui.ctx(), "Fit")).clicked();
            actual = ui.button("100%").clicked();
            if ui.button("−").clicked() {
                zoom_factor = 1.0 / 1.25;
            }
            if ui.button("+").clicked() {
                zoom_factor = 1.25;
            }
        });
        // Reserve status height: asynchronous loading never shifts view controls.
        ui.allocate_ui(Vec2::new(ui.available_width(), 22.0), |ui| {
            ui.horizontal(|ui| {
                if self.job.is_some() {
                    ui.spinner();
                    ui.label(crate::i18n::ui(ui.ctx(), "Loading original…"));
                } else if !self.notice.is_empty() {
                    ui.add(egui::Label::new(&self.notice).truncate());
                } else {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::i18n::ui(
                                ui.ctx(),
                                "Drag to pan · scroll to zoom",
                            ))
                            .weak(),
                        )
                        .truncate(),
                    );
                }
            });
        });
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size().max(Vec2::splat(1.0)),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, Color32::from_black_alpha(35));
        if let Some(entry) = library.images.get_mut(library.selected) {
            if let Some(size) = size {
                let fit_scale = ((rect.width() - 12.0).max(1.0) / size.x)
                    .min((rect.height() - 12.0).max(1.0) / size.y);
                let old = (entry.zoom, entry.pan);
                if fit || response.double_clicked() {
                    entry.fit();
                }
                if actual {
                    entry.zoom_at(1.0 / (fit_scale * entry.zoom), Vec2::ZERO);
                }
                if zoom_factor != 1.0 {
                    entry.zoom_at(zoom_factor, Vec2::ZERO);
                }
                if response.hovered() {
                    let scroll = ui.input_mut(|i| {
                        let d = i.raw_scroll_delta;
                        i.raw_scroll_delta = Vec2::ZERO;
                        i.smooth_scroll_delta = Vec2::ZERO;
                        d
                    });
                    if scroll != Vec2::ZERO {
                        let anchor = response.hover_pos().unwrap_or(rect.center()) - rect.center();
                        entry.zoom_at(((scroll.x + scroll.y) * 0.002).exp(), anchor);
                    }
                    ui.ctx().set_cursor_icon(if response.dragged() {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    });
                }
                if response.dragged() {
                    entry.pan = (Vec2::from(entry.pan) + ui.input(|i| i.pointer.delta()))
                        .clamp(Vec2::splat(-1e6), Vec2::splat(1e6))
                        .into();
                }
                changed |= old != (entry.zoom, entry.pan);
                if let Some((_, Ok((texture, _)))) = &self.cached {
                    let image_rect = Rect::from_center_size(
                        rect.center() + Vec2::from(entry.pan),
                        size * fit_scale * entry.zoom,
                    );
                    painter.rect_filled(image_rect.intersect(rect), 0.0, Color32::from_gray(100));
                    painter.image(
                        texture.id(),
                        image_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                painter.text(
                    rect.left_bottom() + Vec2::new(8.0, -8.0),
                    egui::Align2::LEFT_BOTTOM,
                    crate::i18n::ui(ui.ctx(), format!("{:.0}%", fit_scale * entry.zoom * 100.0)),
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                );
            } else if let Some((_, Err(error))) = &self.cached {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)));
                child.label(error);
                child.label(entry.path.display().to_string());
                child.weak("Use ••• → Locate original… or Reload original.");
            }
            response.on_hover_text(entry.path.display().to_string());
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::ui(
                    ui.ctx(),
                    "Add reference images\nor drop files onto this panel",
                ),
                egui::FontId::proportional(14.0),
                Color32::GRAY,
            );
        }
        changed |= self.confirmation_ui(ui.ctx(), library);
        changed
    }
    fn confirmation_ui(&mut self, ctx: &egui::Context, library: &mut Library) -> bool {
        let Some(action) = self.confirmation.take() else {
            return false;
        };
        let mut confirm = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("reference_remove_confirmation"))
            .backdrop_color(Color32::TRANSPARENT)
            .show(ctx, |ui| {
                ui.set_max_width(340.0);
                let (title, message, button) = match &action {
                    Confirmation::Remove { path, .. } => (
                        "Remove reference?",
                        format!("Remove “{}” from the list?", name(path)),
                        "Remove",
                    ),
                    Confirmation::Clear => (
                        "Clear reference list?",
                        format!(
                            "Remove all {} references from the list?",
                            library.images.len()
                        ),
                        "Clear list",
                    ),
                };
                ui.heading(crate::i18n::ui(ui.ctx(), title));
                ui.label(crate::i18n::ui(ui.ctx(), message));
                ui.weak(crate::i18n::ui(
                    ui.ctx(),
                    "Original files will not be deleted.",
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    cancel = ui.button(crate::i18n::ui(ui.ctx(), "Cancel")).clicked();
                    confirm = ui.button(button).clicked();
                });
            });
        if confirm {
            match action {
                Confirmation::Remove { index, path } => {
                    if library
                        .images
                        .get(index)
                        .is_some_and(|entry| entry.path == path)
                    {
                        library.selected = index;
                        library.remove();
                    } else {
                        return false;
                    }
                }
                Confirmation::Clear => {
                    library.images.clear();
                    library.selected = 0;
                }
            }
            true
        } else {
            if !cancel && !modal.should_close() {
                self.confirmation = Some(action);
            }
            false
        }
    }
}
fn name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linked_library_roundtrip_removal_and_large_lists_do_not_copy_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.png");
        image::RgbaImage::from_pixel(9, 7, image::Rgba([12, 34, 56, 255]))
            .save(&path)
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut settings = crate::presets::Settings::default();
        assert!(settings.references.add([path.clone(), path.clone()]));
        assert_eq!(settings.references.images.len(), 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        settings.references.images[0].zoom_at(2.0, Vec2::new(20.0, 10.0));
        let file = dir.path().join("settings.json");
        settings.save(&file).unwrap();
        let restored = crate::presets::Settings::load(&file).unwrap();
        assert!(restored.references == settings.references);
        let json = serde_json::to_value(&settings.references).unwrap();
        assert!(json["images"][0].get("pixels").is_none());
        assert_eq!(json["images"][0].as_object().unwrap().len(), 3);
        settings.references.remove();
        assert!(settings.references.valid());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        // A large path library must not hit the old 512 KB preferences limit.
        settings.references.images = (0..5000)
            .map(|i| Entry {
                path: dir.path().join(format!("reference-{i}.png")),
                ..Default::default()
            })
            .collect();
        settings.save(&file).unwrap();
        assert!(std::fs::metadata(&file).unwrap().len() > 512 * 1024);
        assert_eq!(
            crate::presets::Settings::load(&file)
                .unwrap()
                .references
                .images
                .len(),
            5000
        );
    }
    #[test]
    fn decoding_is_read_only_respects_preview_bounds_and_reports_missing_originals() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.png");
        image::RgbaImage::from_pixel(5000, 10, image::Rgba([90, 80, 70, 255]))
            .save(&path)
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.size, Vec2::new(5000.0, 10.0));
        assert_eq!(loaded.image.size[0], 4096);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(&path).unwrap();
        assert!(load(&path).is_err());
        let mut library = Library::default();
        library.add([path]);
        assert!(library.valid()); // Missing originals remain relinkable.
    }
    #[test]
    fn pointer_anchored_zoom_and_async_switch_do_not_keep_stale_images() {
        let mut entry = Entry::default();
        let anchor = Vec2::new(40.0, -25.0);
        let before = (anchor - Vec2::from(entry.pan)) / entry.zoom;
        entry.zoom_at(2.5, anchor);
        assert!(((anchor - Vec2::from(entry.pan)) / entry.zoom - before).length() < 0.001);
        entry.fit();
        assert_eq!(entry.pan, [0.0; 2]);
        assert_eq!(entry.zoom, 1.0);
        let ctx = egui::Context::default();
        let (tx, rx) = mpsc::channel();
        let mut state = State {
            job: Some((PathBuf::from("old.png"), rx)),
            ..Default::default()
        };
        tx.send(Err("old failure".into())).unwrap();
        state.poll(&ctx, None);
        assert!(state.cached.is_none());
        assert!(state.job.is_none());
        let mut library = Library::default();
        library.add([PathBuf::from("first.png"), PathBuf::from("second.png")]);
        library.selected = 1;
        library.remove();
        assert_eq!(library.selected, 0);
        assert!(library.valid());
    }
}
