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
fn dropped_name(file: &egui::DroppedFile) -> String {
    if !file.name.is_empty() {
        return file.name.clone();
    }
    if let Some(path) = &file.path {
        if let Some(text) = path.to_str() {
            if text.starts_with("data:image/") {
                return "browser-image.png".into();
            }
            if let Ok(url) = url::Url::parse(text) {
                if let Some(name) = url.path_segments().and_then(Iterator::last) {
                    if !name.is_empty() {
                        return name.to_owned();
                    }
                }
            }
            return path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned();
        }
    }
    "reference-image".into()
}
fn reference_assets_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && let Some(root) = dir
            .ancestors()
            .take(5)
            .find(|ancestor| ancestor.join("assets/default_brushes").is_dir())
    {
        return root.join("assets/references");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/references")
}
fn save_downloaded_image(
    image: &crate::clipboard::Image,
    original_name: &str,
    dir: &Path,
) -> Result<PathBuf, String> {
    image.validate()?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not create references folder: {e}"))?;
    let stem = Path::new(original_name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let mut safe = String::new();
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            safe.push(ch);
        } else if ch.is_whitespace() && !safe.ends_with('-') {
            safe.push('-');
        }
    }
    let safe = safe.trim_matches('-');
    let stem = if safe.is_empty() {
        "reference-image"
    } else {
        safe
    };
    let pixels = image::RgbaImage::from_raw(
        image.size[0] as u32,
        image.size[1] as u32,
        image.pixels.clone(),
    )
    .ok_or_else(|| "The downloaded image has invalid pixels.".to_owned())?;
    let mut index = 1usize;
    loop {
        let filename = if index == 1 {
            format!("{stem}.png")
        } else {
            format!("{stem}-{index}.png")
        };
        let path = dir.join(filename);
        if path.exists() {
            index += 1;
            continue;
        }
        pixels
            .save(&path)
            .map_err(|e| format!("Could not save downloaded reference: {e}"))?;
        return Ok(path);
    }
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
struct DownloadOrigin {
    rect: Rect,
    viewport: egui::ViewportId,
}
struct Arrival {
    origin: DownloadOrigin,
    path: PathBuf,
    elapsed: f32,
    button_elapsed: Option<f32>,
}
enum Confirmation {
    Remove {
        index: usize,
        path: PathBuf,
    },
    Clear,
    SaveDropped {
        file: egui::DroppedFile,
        name: String,
    },
}
#[derive(Default)]
pub struct State {
    cached: Option<(PathBuf, Preview)>,
    job: Option<LoadJob>,
    bounds: Option<(egui::ViewportId, Rect, egui::LayerId, u64)>,
    notice: String,
    confirmation: Option<Confirmation>,
    download: Option<(crate::image_drop::Job, String, DownloadOrigin)>,
    arrival: Option<Arrival>,
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
        self.handle_files(files, library)
    }
    fn handle_files(&mut self, files: Vec<egui::DroppedFile>, library: &mut Library) -> bool {
        let mut local_paths = Vec::new();
        let mut changed = false;
        for file in files {
            let uri_path = file.path.as_ref().is_some_and(|path| {
                path.to_str()
                    .is_some_and(|text| text.starts_with("data:") || text.contains("://"))
            });
            if let Some(path) = file.path.as_ref().filter(|_| !uri_path) {
                if supported(path) {
                    local_paths.push(path.clone());
                }
                continue;
            }
            let job_file = file.clone();
            match crate::image_drop::sources(vec![file]) {
                Ok(mut sources) => match sources.remove(0) {
                    Ok(
                        crate::image_drop::Source::Bytes(_)
                        | crate::image_drop::Source::Data(_)
                        | crate::image_drop::Source::Remote(_),
                    ) => {
                        // Keep the browser payload until the user confirms downloading it.
                        self.confirmation = Some(Confirmation::SaveDropped {
                            name: dropped_name(&job_file),
                            file: job_file,
                        });
                        self.notice = "Confirm to download and save the browser image.".into();
                    }
                    Ok(crate::image_drop::Source::File(path)) if supported(&path) => {
                        local_paths.push(path);
                    }
                    _ => self.notice = "Drop an image file onto the references panel.".into(),
                },
                Err(error) => self.notice = error,
            }
        }
        changed |= library.add(local_paths);
        changed
    }
    fn poll(&mut self, ctx: &egui::Context, path: Option<&Path>) {
        if self
            .arrival
            .as_ref()
            .is_some_and(|flight| Some(flight.path.as_path()) != path)
        {
            self.arrival = None;
        }
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
        if self
            .cached
            .as_ref()
            .is_some_and(|(_, preview)| preview.is_err())
        {
            self.arrival = None;
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
        changed |= self.poll_download(ui.ctx(), library);
        ui.horizontal_wrapped(|ui| {
            let add = ui
                .button(crate::i18n::ui(ui.ctx(), "Add…"))
                .on_hover_text(crate::i18n::ui(
                    ui.ctx(),
                    "Links local images without copying them into the project",
                ));
            if add.clicked()
                && let Some(paths) = picker().pick_files()
            {
                let has_image = paths.iter().any(|path| supported(path));
                changed |= library.add(paths);
                if has_image && let Some(entry) = library.images.get(library.selected) {
                    // Wait for the asynchronous preview before flying from Add to the view.
                    // Local references remain linked to their original files.
                    self.arrival = (!crate::theme::reduced_motion(ui.ctx())).then(|| Arrival {
                        origin: DownloadOrigin {
                            rect: add.rect,
                            viewport: ui.ctx().viewport_id(),
                        },
                        path: entry.path.clone(),
                        elapsed: 0.0,
                        button_elapsed: None,
                    });
                    ui.ctx().request_repaint();
                }
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
                    if self
                        .arrival
                        .as_ref()
                        .is_some_and(|flight| flight.path != entry.path)
                        || (crate::theme::reduced_motion(ui.ctx())
                            && self
                                .arrival
                                .as_ref()
                                .is_some_and(|flight| flight.button_elapsed.is_none()))
                    {
                        self.arrival = None;
                    }
                    if let Some(flight) = &mut self.arrival {
                        if flight.button_elapsed.is_none() {
                            flight.elapsed += ui.input(|i| i.stable_dt).min(0.05);
                            let t = (flight.elapsed / 0.65).clamp(0.0, 1.0);
                            let eased = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
                            let source_size = size
                                * (flight.origin.rect.height() / size.y)
                                    .min(flight.origin.rect.width() / size.x);
                            let arc = Vec2::new(0.0, -45.0 * (std::f32::consts::PI * t).sin());
                            let center =
                                flight.origin.rect.center().lerp(image_rect.center(), eased) + arc;
                            let flying_rect = Rect::from_center_size(
                                center,
                                source_size * (1.0 - eased) + image_rect.size() * eased,
                            );
                            ui.ctx()
                                .layer_painter(egui::LayerId::new(
                                    egui::Order::Foreground,
                                    egui::Id::new("reference-download-arrival"),
                                ))
                                .image(
                                    texture.id(),
                                    flying_rect,
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            ui.ctx().request_repaint();
                            if t >= 1.0 || flight.origin.viewport != ui.ctx().viewport_id() {
                                self.arrival = None;
                            }
                        }
                    } else {
                        painter.rect_filled(
                            image_rect.intersect(rect),
                            0.0,
                            Color32::from_gray(100),
                        );
                        painter.image(
                            texture.id(),
                            image_rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
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
        let mut button_rect = Rect::NOTHING;
        let busy = self.download.is_some()
            || self
                .arrival
                .as_ref()
                .is_some_and(|flight| flight.button_elapsed.is_some());
        let mut depart = false;
        let modal = egui::Modal::new(egui::Id::new("reference_remove_confirmation"))
            .backdrop_color(Color32::TRANSPARENT)
            .show(ctx, |ui| {
                ui.set_max_width(340.0);
                ui.set_min_width(340.0);
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
                    Confirmation::SaveDropped { name, .. } => (
                        "Save image to references?",
                        format!("Download “{name}” and save a copy in assets/references?"),
                        "Save image",
                    ),
                };
                ui.heading(crate::i18n::ui(ui.ctx(), title));
                ui.label(crate::i18n::ui(ui.ctx(), message));
                ui.weak(crate::i18n::ui(
                    ui.ctx(),
                    if matches!(&action, Confirmation::SaveDropped { .. }) {
                        "This saves a copy in assets/references and does not change the canvas or clipboard."
                    } else {
                        "Original files will not be deleted."
                    },
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    cancel = ui.add_enabled(
                        !self.arrival.as_ref().is_some_and(|flight| flight.button_elapsed.is_some()),
                        egui::Button::new(crate::i18n::ui(ui.ctx(), "Cancel")),
                    ).clicked();
                    let is_download = matches!(&action, Confirmation::SaveDropped { .. });
                    let label = if busy { String::new() } else { crate::i18n::ui(ui.ctx(), button) };
                    let mut widget = egui::Button::new(label);
                    if is_download {
                        widget = widget.min_size(Vec2::new(150.0, 64.0));
                    }
                    let response = ui.add_enabled(!busy, widget);
                    button_rect = response.rect;
                    confirm = response.clicked();
                    if busy && is_download {
                        let painter = ui.painter().with_clip_rect(response.rect.shrink(2.0));
                        if let Some(flight) = &mut self.arrival
                            && let Some(elapsed) = &mut flight.button_elapsed
                            && let Some((_, Ok((texture, _)))) = &self.cached
                        {
                            *elapsed += ui.input(|i| i.stable_dt).min(0.05);
                            let fade = (*elapsed / 0.25).clamp(0.0, 1.0);
                            let image_size = texture.size_vec2();
                            let fit = (response.rect.width() / image_size.x)
                                .min(response.rect.height() / image_size.y);
                            let preview_rect = Rect::from_center_size(response.rect.center(), image_size * fit);
                            painter.image(texture.id(), preview_rect,
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE.gamma_multiply(fade));
                            flight.origin.rect = response.rect;
                            if *elapsed >= 0.45 {
                                flight.button_elapsed = None;
                                depart = true;
                            }
                        } else {
                            let spinner_rect = Rect::from_center_size(response.rect.center(), Vec2::splat(22.0));
                            ui.put(spinner_rect, egui::Spinner::new());
                        }
                        if let Some((_, _, origin)) = &mut self.download {
                            origin.rect = response.rect;
                        }
                        ui.ctx().request_repaint();
                    }
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
                Confirmation::SaveDropped { file, name } => {
                    match crate::image_drop::Job::start(vec![file.clone()], None, ctx.clone()) {
                        Ok(job) => {
                            self.confirmation = Some(Confirmation::SaveDropped {
                                file: file.clone(),
                                name: name.clone(),
                            });
                            self.download = Some((
                                job,
                                name,
                                DownloadOrigin {
                                    rect: button_rect,
                                    viewport: ctx.viewport_id(),
                                },
                            ));
                            self.notice = "Downloading reference image…".into();
                        }
                        Err(error) => self.notice = error,
                    }
                }
            }
            true
        } else {
            if cancel || (!busy && modal.should_close()) {
                if matches!(&action, Confirmation::SaveDropped { .. }) {
                    self.notice.clear();
                    self.download = None;
                }
            }
            if !cancel && !depart && (busy || !modal.should_close()) {
                self.confirmation = Some(action);
            }
            false
        }
    }
    fn poll_download(&mut self, ctx: &egui::Context, library: &mut Library) -> bool {
        let result = self.download.as_mut().and_then(|(job, _, _)| job.poll());
        let Some(result) = result else {
            if self.download.is_some() {
                ctx.request_repaint_after(Duration::from_millis(25));
            }
            return false;
        };
        let Some((_, name, origin)) = self.download.take() else {
            return false;
        };
        match result {
            Ok(loaded) => {
                match save_downloaded_image(&loaded.image, &name, &reference_assets_dir()) {
                    Ok(path) => {
                        self.notice = format!("Saved reference: {}", path.display());
                        let changed = library.add([path]);
                        let path = library.images[library.selected].path.clone();
                        let size =
                            Vec2::new(loaded.image.size[0] as f32, loaded.image.size[1] as f32);
                        // Reuse the downloaded pixels immediately, avoiding a second disk load
                        // and an instant static preview underneath the flying image.
                        let pixels = image::RgbaImage::from_raw(
                            loaded.image.size[0] as u32,
                            loaded.image.size[1] as u32,
                            loaded.image.pixels,
                        )
                        .expect("validated downloaded image");
                        let preview = image::DynamicImage::ImageRgba8(pixels)
                            .thumbnail(4096, 4096)
                            .into_rgba8();
                        let texture = ctx.load_texture(
                            "reference_preview",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [preview.width() as usize, preview.height() as usize],
                                preview.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        );
                        self.job = None;
                        self.cached = Some((path.clone(), Ok((texture, size))));
                        self.arrival = Some(Arrival {
                            origin,
                            path,
                            elapsed: 0.0,
                            button_elapsed: Some(0.0),
                        });
                        ctx.request_repaint();
                        changed
                    }
                    Err(error) => {
                        self.notice = error;
                        false
                    }
                }
            }
            Err(error) => {
                self.notice = error;
                false
            }
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
    fn downloaded_reference_is_saved_as_a_unique_png_copy() {
        let dir = tempfile::tempdir().unwrap();
        let image = crate::clipboard::Image {
            size: [2, 1],
            pixels: vec![255, 20, 30, 255, 10, 40, 250, 128],
        };
        let first = save_downloaded_image(&image, "My reference.jpg", dir.path()).unwrap();
        let second = save_downloaded_image(&image, "My reference.jpg", dir.path()).unwrap();
        assert_eq!(first.file_name().unwrap(), "My-reference.png");
        assert_eq!(second.file_name().unwrap(), "My-reference-2.png");
        let decoded = image::open(first).unwrap().into_rgba8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.as_raw(), &image.pixels);
    }
    #[test]
    fn browser_image_drop_waits_for_confirmation_and_does_not_touch_clipboard() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("copied.png");
        image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]))
            .save(&file_path)
            .unwrap();
        let bytes: std::sync::Arc<[u8]> = std::fs::read(file_path).unwrap().into();
        let dropped = egui::DroppedFile {
            name: "copied.png".into(),
            mime: "image/png".into(),
            bytes: Some(bytes),
            ..Default::default()
        };
        let mut state = State::default();
        let mut library = Library::default();
        assert!(!state.handle_files(vec![dropped], &mut library));
        assert!(library.images.is_empty());
        assert!(matches!(
            state.confirmation,
            Some(Confirmation::SaveDropped { .. })
        ));
        assert!(state.download.is_none());
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
