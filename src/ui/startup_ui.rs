use super::*;

const TRANSITION_DURATION: f32 = 0.68;
const IMAGE_FADE_DURATION: f32 = 0.32;

pub(super) struct Transition {
    source: egui::Rect,
    elapsed: f32,
    fade_image: bool,
    dropped: bool,
}

impl Transition {
    fn from_preview(ctx: &egui::Context, preview: egui::Rect, size: Vec2) -> Option<Self> {
        if crate::theme::reduced_motion(ctx) {
            return None;
        }
        let scale = (preview.width() / size.x).min(preview.height() / size.y);
        let mut transition =
            Self::new(egui::Rect::from_center_size(preview.center(), size * scale));
        transition.fade_image = true;
        Some(transition)
    }
    pub(super) fn from_drop(ctx: &egui::Context, size: Vec2, origin: Pos2) -> Option<Self> {
        if crate::theme::reduced_motion(ctx) {
            return None;
        }
        let source = egui::Rect::from_center_size(origin, size * (80.0 / size.max_elem()));
        let mut transition = Self::new(source);
        transition.fade_image = true;
        transition.dropped = true;
        Some(transition)
    }
    pub(super) fn new(source: egui::Rect) -> Self {
        Self {
            source,
            elapsed: 0.0,
            fade_image: false,
            dropped: false,
        }
    }

    fn progress(&self) -> f32 {
        let duration = if self.dropped {
            0.54
        } else {
            TRANSITION_DURATION
        };
        let t = (self.elapsed / duration).clamp(0.0, 1.0);
        if self.dropped {
            1.0 - (1.0 - t).powi(3)
        } else {
            t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
        }
    }

    fn opacity(&self) -> f32 {
        if !self.fade_image {
            return 1.0;
        }
        let t = (self.elapsed / IMAGE_FADE_DURATION).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn view(&self, size: Vec2, destination: egui::Rect, zoom: f32) -> CanvasView {
        let t = self.progress();
        CanvasView {
            center: self.source.center().lerp(destination.center(), t),
            size,
            zoom: egui::lerp(self.source.width() / size.x..=zoom, t),
            angle: 0.0,
            mirrored: false,
        }
    }
}

impl PaintAtlas {
    pub(super) fn startup_paste_transition(
        &self,
        ctx: &egui::Context,
        size: Vec2,
    ) -> Option<Transition> {
        if !self.startup_canvas_required || crate::theme::reduced_motion(ctx) {
            return None;
        }
        Transition::from_preview(ctx, self.startup_preview?, size)
    }

    pub(super) fn open_startup_file(&mut self, ctx: &egui::Context) {
        // Capture the splash geometry before opening resets document/view state.
        let preview = self
            .startup_canvas_required
            .then_some(self.startup_preview)
            .flatten();
        self.perform(Action::Open);
        if let Some(preview) = preview
            && !self.startup_canvas_required
        {
            // Animate the active document only after a successful open. A canceled
            // picker or failed import leaves the splash and its preview intact.
            let size = Vec2::new(self.doc.width as f32, self.doc.height as f32);
            self.begin_startup_transition(ctx, Transition::from_preview(ctx, preview, size));
        }
    }

    pub(super) fn begin_startup_transition(
        &mut self,
        ctx: &egui::Context,
        transition: Option<Transition>,
    ) {
        self.startup_transition = transition;
        if self.startup_transition.is_some() {
            // Resolve a fresh destination even if another panel shares the canvas dock.
            self.last_canvas_bounds = None;
            self.workspace.layout.apply(crate::workspace::Action::Open(
                crate::workspace::Panel::Canvas,
            ));
        }
        ctx.request_repaint();
    }

    pub(super) fn startup_transition_ui(&mut self, ctx: &egui::Context) {
        let Some(transition) = &self.startup_transition else {
            return;
        };
        let Some(destination) = self.last_canvas_bounds else {
            ctx.request_repaint();
            return;
        };
        let progress = transition.progress();
        let opacity = transition.opacity();
        let view = transition.view(
            Vec2::new(self.doc.width as f32, self.doc.height as f32),
            destination,
            self.zoom,
        );
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("startup_canvas_handoff"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .constrain(false)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                ui.set_max_size(screen.size());
                // Capture pointer input above the editor until the canvas lands.
                ui.interact(
                    screen,
                    ui.id().with("input_shield"),
                    Sense::click_and_drag(),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(screen), |ui| {
                    ui.visuals_mut().disabled_alpha = 1.0;
                    ui.disable();
                    ui.set_opacity(1.0 - progress);
                    self.startup_contents(ctx, ui);
                });
                // Reuse the real GPU layer textures, rather than rasterizing or copying pixels.
                let mut painter = ui.painter().clone();
                painter.multiply_opacity(opacity);
                painter.extend(self.canvas_document_shapes(screen, view));
            });
        if progress >= 1.0 || crate::theme::reduced_motion(ctx) {
            self.startup_transition = None;
        } else if let Some(transition) = &mut self.startup_transition {
            transition.elapsed += ctx.input(|i| i.stable_dt).clamp(0.0, 1.0 / 30.0);
        }
        ctx.request_repaint();
    }

    pub(super) fn startup_splash(&mut self, ctx: &egui::Context) {
        if self.splash_background.is_none() {
            let image = image::load_from_memory(include_bytes!("../../assets/background.jpg"))
                .expect("Invalid splash background image")
                .into_rgba8();
            // Cache a small, blurred copy once; resizing the window only changes UVs.
            let small = image::imageops::thumbnail(&image, 640, 640);
            let blurred = image::imageops::blur(&small, 14.0);
            let load = |name, image: &image::RgbaImage| {
                ctx.load_texture(
                    name,
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                )
            };
            self.splash_background = Some(load("startup_background", &image));
            self.splash_blur = Some(load("startup_blur", &blurred));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| self.startup_contents(ctx, ui));
        if self.screenshot.is_some()
            && self.frames == 6
            && let Ok(scene) = std::env::var("PAINTATLAS_SMOKE_SCENE")
        {
            if scene.starts_with("startup-transition") {
                self.create_canvas(ctx);
            } else if scene.starts_with("startup-paste") {
                // Exercise the paste path without touching the desktop clipboard.
                let image = if let Some(path) = std::env::var_os("PAINTATLAS_SMOKE_IMAGE") {
                    image::open(path).expect("Invalid smoke image")
                } else {
                    image::load_from_memory(include_bytes!("../../assets/background.jpg"))
                        .expect("Invalid smoke image")
                };
                let image = image.thumbnail(1280, 1280).into_rgba8();
                self.paste_image(
                    ctx,
                    crate::clipboard::Image {
                        size: [image.width() as usize, image.height() as usize],
                        pixels: image.into_raw(),
                    },
                )
                .expect("Startup paste smoke failed");
            }
        }
    }

    fn startup_contents(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let texture = self.splash_background.as_ref().unwrap();
        let texture_id = texture.id();
        let image_size = texture.size_vec2();
        let blur_id = self.splash_blur.as_ref().unwrap().id();
        let bounds = ui.max_rect();
        let scale = (bounds.width() / image_size.x).max(bounds.height() / image_size.y);
        let uv_size = bounds.size() / (image_size * scale);
        let uv = egui::Rect::from_center_size(Pos2::new(0.5, 0.5), uv_size);
        ui.painter().image(texture_id, bounds, uv, Color32::WHITE);

        let width = (bounds.width() * 0.28)
            .clamp(340.0, 480.0)
            .min(bounds.width());
        let sidebar = egui::Rect::from_min_size(bounds.min, Vec2::new(width, bounds.height()));
        let sidebar_uv = egui::Rect::from_min_max(
            uv.min,
            Pos2::new(uv.min.x + uv.width() * width / bounds.width(), uv.max.y),
        );
        ui.painter()
            .image(blur_id, sidebar, sidebar_uv, Color32::WHITE);
        ui.painter()
            .rect_filled(sidebar, 0.0, Color32::from_black_alpha(162));
        ui.painter().vline(
            sidebar.right(),
            sidebar.y_range(),
            Stroke::new(1.0_f32, Color32::from_white_alpha(24)),
        );
        let padding = (width * 0.085).clamp(20.0, 40.0);
        let inner = sidebar.shrink2(Vec2::new(padding, 26.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.set_width(inner.width());
            egui::ScrollArea::vertical()
                .id_salt("startup_form_scroll")
                .max_height(inner.height())
                .auto_shrink([false, false])
                .show(ui, |ui| self.startup_form(ctx, ui));
        });
    }

    fn startup_form(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.close_motion.active() {
            ui.visuals_mut().disabled_alpha = 1.0;
            ui.disable();
        }
        ui.spacing_mut().item_spacing = Vec2::new(12.0, 16.0);
        ui.label(
            egui::RichText::new("PAINT ATLAS")
                .size(15.0)
                .strong()
                .color(Color32::from_gray(220)),
        );
        ui.add_space(2.0);
        let button_width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        ui.horizontal(|ui| {
            if ui
                .add_sized(
                    [button_width, 44.0],
                    egui::Button::new(crate::i18n::ui(ctx, "Open file…")),
                )
                .clicked()
            {
                self.open_startup_file(ctx);
            }
            if ui
                .add_sized(
                    [button_width, 44.0],
                    egui::Button::new(crate::i18n::ui(ctx, "Paste image")),
                )
                .on_hover_text(self.settings.shortcuts.label(Shortcut::Paste))
                .clicked()
            {
                self.paste_from_clipboard(ctx);
            }
        });
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(crate::i18n::ui(ctx, "New canvas"))
                .size(30.0)
                .strong(),
        );
        ui.add_space(6.0);
        let field_width = (ui.available_width() - 124.0).clamp(90.0, 180.0);
        egui::Grid::new("startup_dimensions")
            .num_columns(2)
            .spacing([20.0, 16.0])
            .min_row_height(38.0)
            .show(ui, |ui| {
                ui.label(crate::i18n::ui(ctx, "Width"));
                ui.add_sized(
                    [field_width, 38.0],
                    egui::DragValue::new(&mut self.new_width)
                        .range(16..=MAX_SIDE)
                        .suffix(" px"),
                );
                ui.end_row();
                ui.label(crate::i18n::ui(ctx, "Height"));
                ui.add_sized(
                    [field_width, 38.0],
                    egui::DragValue::new(&mut self.new_height)
                        .range(16..=MAX_SIDE)
                        .suffix(" px"),
                );
                ui.end_row();
                ui.label(crate::i18n::ui(ctx, "Resolution"));
                ui.add_sized(
                    [field_width, 38.0],
                    egui::DragValue::new(&mut self.new_dpi)
                        .range(1..=crate::document::MAX_DPI)
                        .suffix(" DPI"),
                );
                ui.end_row();
            });
        ui.add_space(2.0);
        crate::theme::toggle(
            ui,
            &mut self.white,
            &crate::i18n::ui(ctx, "Add paper layer"),
        );
        ui.add_enabled_ui(self.white, |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::ui(ctx, "Paper color"));
                ui.color_edit_button_srgb(&mut self.paper_color);
            });
        });
        ui.label(
            egui::RichText::new(crate::i18n::ui(
                ctx,
                format!(
                    "Print size: {:.1} × {:.1} cm · DPI does not change the pixel count",
                    self.new_width as f64 / self.new_dpi.max(1) as f64 * 2.54,
                    self.new_height as f64 / self.new_dpi.max(1) as f64 * 2.54,
                ),
            ))
            .size(12.0)
            .color(Color32::from_gray(180)),
        );
        ui.add_space(4.0);
        let accent = crate::theme::accent(ctx);
        let create_clicked = ui
            .add_sized(
                [ui.available_width(), 46.0],
                egui::Button::new(
                    egui::RichText::new(crate::i18n::ui(ctx, "Create"))
                        .size(16.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(accent)
                .corner_radius(12),
            )
            .clicked();
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(crate::i18n::ui(ctx, "Canvas preview"))
                .size(13.0)
                .color(Color32::from_gray(205)),
        );
        self.startup_canvas_preview(ui);
        ui.label(
            egui::RichText::new(crate::i18n::ui(
                ctx,
                format!(
                    "Image copied? Use {} to open it at its original size.",
                    self.settings.shortcuts.label(Shortcut::Paste)
                ),
            ))
            .size(11.0)
            .color(Color32::from_gray(175)),
        );
        if self.status.starts_with("Could not open:")
            || self.status.starts_with("Could not import:")
            || self.status.starts_with("Could not paste:")
        {
            ui.colored_label(
                ui.visuals().error_fg_color,
                crate::i18n::ui(ctx, &self.status),
            );
        }
        // Read the preview after laying out this frame (including any edited dimensions).
        if create_clicked {
            self.create_canvas(ctx);
        }
    }

    fn startup_canvas_preview(&mut self, ui: &mut egui::Ui) {
        let height = (ui.ctx().content_rect().height() * 0.23).clamp(140.0, 240.0);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 16, Color32::from_black_alpha(95));
        let mut available = rect.shrink2(Vec2::new(24.0, 34.0));
        available.max.x -= 52.0;
        let size = Vec2::new(self.new_width as f32, self.new_height as f32);
        let scale = (available.width() / size.x).min(available.height() / size.y);
        let paper = egui::Rect::from_center_size(available.center(), size * scale);
        if self.startup_transition.is_none() {
            self.startup_preview = ui.clip_rect().contains_rect(paper).then_some(paper);
        }
        if self
            .startup_transition
            .as_ref()
            .is_some_and(|t| !t.fade_image)
        {
            // The moving document is painted once, above the fading splash.
        } else if self.white {
            painter.rect_filled(
                paper,
                0.0,
                Color32::from_rgb(
                    self.paper_color[0],
                    self.paper_color[1],
                    self.paper_color[2],
                ),
            );
        } else {
            let cell = 8.0;
            for y in 0..(paper.height() / cell).ceil() as usize {
                for x in 0..(paper.width() / cell).ceil() as usize {
                    let tile = egui::Rect::from_min_size(
                        paper.min + Vec2::new(x as f32 * cell, y as f32 * cell),
                        Vec2::splat(cell),
                    )
                    .intersect(paper);
                    painter.rect_filled(
                        tile,
                        0.0,
                        Color32::from_gray(if (x + y) % 2 == 0 { 185 } else { 225 }),
                    );
                }
            }
        }
        painter.rect_stroke(
            paper.expand(6.0),
            0,
            Stroke::new(1.0_f32, Color32::from_white_alpha(100)),
            egui::StrokeKind::Outside,
        );
        let font = egui::FontId::proportional(11.0);
        let color = Color32::from_gray(185);
        painter.text(
            Pos2::new(paper.center().x, rect.top() + 14.0),
            egui::Align2::CENTER_TOP,
            format!("{} px", self.new_width),
            font.clone(),
            color,
        );
        painter.text(
            Pos2::new(paper.right() + 12.0, paper.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{} px", self.new_height),
            font,
            color,
        );
    }
}
