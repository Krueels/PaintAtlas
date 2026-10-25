//! Export a flattened snapshot without changing the editable document.
use super::*;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Png,
    Jpeg,
    Webp,
}
impl Format {
    fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webp => "WebP (sem perdas)",
        }
    }
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }
    fn image_format(self) -> image::ImageFormat {
        match self {
            Self::Png => image::ImageFormat::Png,
            Self::Jpeg => image::ImageFormat::Jpeg,
            Self::Webp => image::ImageFormat::WebP,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sampling {
    Nearest,
    Bilinear,
    Bicubic,
    Lanczos,
}
impl Sampling {
    fn label(self) -> &'static str {
        match self {
            Self::Nearest => "Vizinho mais próximo",
            Self::Bilinear => "Bilinear",
            Self::Bicubic => "Bicúbica",
            Self::Lanczos => "Lanczos (alta qualidade)",
        }
    }
    fn filter(self) -> image::imageops::FilterType {
        match self {
            Self::Nearest => image::imageops::FilterType::Nearest,
            Self::Bilinear => image::imageops::FilterType::Triangle,
            Self::Bicubic => image::imageops::FilterType::CatmullRom,
            Self::Lanczos => image::imageops::FilterType::Lanczos3,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Options {
    format: Format,
    width: u32,
    height: u32,
    transparent: bool,
    background: [u8; 3],
    quality: u8,
    sampling: Sampling,
    resolution: bool,
    dpi: u32,
}
struct Output {
    bytes: Vec<u8>,
    preview: egui::ColorImage,
}
struct Job {
    options: Options,
    receiver: mpsc::Receiver<Result<Output, String>>,
}
pub(super) struct Export {
    source: Arc<Vec<u8>>,
    original: [u32; 2],
    name: String,
    options: Options,
    proportions: bool,
    changed: Instant,
    job: Option<Job>,
    ready: Option<(Options, Vec<u8>)>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
}

fn encode(source: &[u8], original: [u32; 2], options: &Options) -> Result<Output, String> {
    use image::ImageEncoder;
    if options.width == 0
        || options.height == 0
        || options.width > MAX_SIDE
        || options.height > MAX_SIDE
    {
        return Err(format!(
            "Dimensions must be between 1 and {MAX_SIDE} pixels."
        ));
    }
    if source.len() != original[0] as usize * original[1] as usize * 4 {
        return Err("Invalid export snapshot.".into());
    }
    let mut pixels = if original == [options.width, options.height] {
        source.to_vec()
    } else {
        crate::image_ops::resized(
            source.to_vec(),
            original[0],
            original[1],
            options.width,
            options.height,
            options.sampling.filter(),
        )
    };
    if !options.transparent || options.format == Format::Jpeg {
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let a = pixel[3] as u32;
            for (channel, background) in pixel[..3].iter_mut().zip(options.background) {
                *channel =
                    ((*channel as u32 * a + background as u32 * (255 - a) + 127) / 255) as u8;
            }
            pixel[3] = 255;
        }
    }
    let mut bytes = Vec::new();
    match options.format {
        Format::Png => {
            let mut encoder = png::Encoder::new(&mut bytes, options.width, options.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            if options.resolution {
                let ppm = (options.dpi as f64 / 0.0254).round() as u32;
                encoder.set_pixel_dims(Some(png::PixelDimensions {
                    xppu: ppm,
                    yppu: ppm,
                    unit: png::Unit::Meter,
                }));
            }
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            writer
                .write_image_data(&pixels)
                .map_err(|e| e.to_string())?;
            writer.finish().map_err(|e| e.to_string())?;
        }
        Format::Jpeg => {
            let rgb: Vec<u8> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| p[..3].iter().copied())
                .collect();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, options.quality);
            if options.resolution {
                encoder
                    .set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(options.dpi as u16));
            }
            encoder
                .encode(
                    &rgb,
                    options.width,
                    options.height,
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| e.to_string())?;
        }
        Format::Webp => image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
            .write_image(
                &pixels,
                options.width,
                options.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?,
    }
    // Show the actual compression artifacts for JPEG, not an uncompressed approximation.
    let image = if options.format == Format::Jpeg {
        image::load_from_memory_with_format(&bytes, options.format.image_format())
            .map_err(|e| e.to_string())?
            .into_rgba8()
    } else {
        image::RgbaImage::from_raw(options.width, options.height, pixels).unwrap()
    };
    let preview = if image.width() > 560 || image.height() > 560 {
        image::DynamicImage::ImageRgba8(image)
            .thumbnail(560, 560)
            .into_rgba8()
    } else {
        image
    };
    Ok(Output {
        bytes,
        preview: egui::ColorImage::from_rgba_unmultiplied(
            [preview.width() as usize, preview.height() as usize],
            preview.as_raw(),
        ),
    })
}

impl Export {
    fn new(doc: &Document, name: String) -> Self {
        Self {
            source: Arc::new(doc.composite()),
            original: [doc.width, doc.height],
            name,
            options: Options {
                format: Format::Png,
                width: doc.width,
                height: doc.height,
                transparent: true,
                background: [255; 3],
                quality: 90,
                sampling: Sampling::Bicubic,
                resolution: true,
                dpi: doc.dpi,
            },
            proportions: true,
            changed: Instant::now() - Duration::from_secs(1),
            job: None,
            ready: None,
            texture: None,
            error: None,
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.job {
            match job.receiver.try_recv() {
                Ok(result) => {
                    let options = job.options.clone();
                    self.job = None;
                    if options == self.options {
                        match result {
                            Ok(output) => {
                                self.texture = Some(ctx.load_texture(
                                    "export-preview",
                                    output.preview,
                                    egui::TextureOptions::LINEAR,
                                ));
                                self.ready = Some((options, output.bytes));
                                self.error = None;
                            }
                            Err(e) => {
                                self.error = Some(e);
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    self.error = Some("O processamento de exportação foi interrompido.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.job.is_none()
            && self.error.is_none()
            && !self.is_ready()
            && self.changed.elapsed() >= Duration::from_millis(180)
        {
            let source = self.source.clone();
            let original = self.original;
            let options = self.options.clone();
            let (sender, receiver) = mpsc::channel();
            let ctx = ctx.clone();
            self.job = Some(Job {
                options: options.clone(),
                receiver,
            });
            std::thread::spawn(move || {
                let result = encode(&source, original, &options);
                let _ = sender.send(result);
                ctx.request_repaint();
            });
        }
        if !self.is_ready() && self.error.is_none() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
    fn is_ready(&self) -> bool {
        self.ready
            .as_ref()
            .is_some_and(|(options, _)| options == &self.options)
    }
    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.strong("Configurações do arquivo");
        egui::ComboBox::from_id_salt("export-format")
            .selected_text(self.options.format.label())
            .show_ui(ui, |ui| {
                for format in [Format::Png, Format::Jpeg, Format::Webp] {
                    ui.selectable_value(&mut self.options.format, format, format.label());
                }
            });
        ui.add_enabled(
            self.options.format != Format::Jpeg,
            egui::Checkbox::new(&mut self.options.transparent, "Transparência"),
        );
        if self.options.format == Format::Jpeg || !self.options.transparent {
            ui.horizontal(|ui| {
                ui.label("Cor do fundo");
                ui.color_edit_button_srgb(&mut self.options.background);
            });
        }
        if self.options.format == Format::Jpeg {
            ui.add(egui::Slider::new(&mut self.options.quality, 1..=100).text("Qualidade"));
        }
        ui.add_enabled(
            self.options.format != Format::Webp,
            egui::Checkbox::new(&mut self.options.resolution, "Incluir resolução (DPI)"),
        );
        crate::theme::caption(ui, "RGB de 8 bits · sem conversão de perfil ICC");
        ui.separator();
        ui.strong("Tamanho da imagem");
        let mut scale = self.options.width as f32 / self.original[0] as f32 * 100.0;
        ui.horizontal(|ui| {
            ui.label("Escala");
            let max_scale =
                (MAX_SIDE as f32 / self.original[0].max(self.original[1]) as f32 * 100.0).max(0.01);
            if ui
                .add(
                    egui::DragValue::new(&mut scale)
                        .range(0.01..=max_scale)
                        .speed(0.5)
                        .suffix(" %"),
                )
                .changed()
            {
                self.options.width = (self.original[0] as f32 * scale / 100.0)
                    .round()
                    .clamp(1.0, MAX_SIDE as f32) as u32;
                self.options.height = (self.original[1] as f32 * scale / 100.0)
                    .round()
                    .clamp(1.0, MAX_SIDE as f32) as u32;
            }
        });
        for width in [true, false] {
            ui.horizontal(|ui| {
                ui.label(if width { "Largura" } else { "Altura" });
                let dimension = if width {
                    &mut self.options.width
                } else {
                    &mut self.options.height
                };
                if ui
                    .add(
                        egui::DragValue::new(dimension)
                            .range(1..=MAX_SIDE)
                            .suffix(" px"),
                    )
                    .changed()
                    && self.proportions
                {
                    let ratio = self.original[0] as f64 / self.original[1] as f64;
                    if width {
                        self.options.height = (self.options.width as f64 / ratio)
                            .round()
                            .clamp(1.0, MAX_SIDE as f64)
                            as u32;
                    } else {
                        self.options.width = (self.options.height as f64 * ratio)
                            .round()
                            .clamp(1.0, MAX_SIDE as f64)
                            as u32;
                    }
                    // Clamp the driving dimension too, so the aspect ratio survives the size limit.
                    if width {
                        self.options.width = (self.options.height as f64 * ratio)
                            .round()
                            .clamp(1.0, MAX_SIDE as f64)
                            as u32;
                    } else {
                        self.options.height = (self.options.width as f64 / ratio)
                            .round()
                            .clamp(1.0, MAX_SIDE as f64)
                            as u32;
                    }
                }
            });
        }
        ui.checkbox(&mut self.proportions, "Restringir proporções");
        ui.label("Reamostrar");
        egui::ComboBox::from_id_salt("export-sampling")
            .selected_text(self.options.sampling.label())
            .show_ui(ui, |ui| {
                for sampling in [
                    Sampling::Nearest,
                    Sampling::Bilinear,
                    Sampling::Bicubic,
                    Sampling::Lanczos,
                ] {
                    ui.selectable_value(&mut self.options.sampling, sampling, sampling.label());
                }
            });
        ui.horizontal(|ui| {
            ui.label("DPI");
            ui.add_enabled(
                self.options.resolution && self.options.format != Format::Webp,
                egui::DragValue::new(&mut self.options.dpi).range(1..=crate::document::MAX_DPI),
            );
        });
        crate::theme::caption(
            ui,
            format!("Original: {} × {} px", self.original[0], self.original[1]),
        );
        if ui.button("Restaurar tamanho original").clicked() {
            self.options.width = self.original[0];
            self.options.height = self.original[1];
        }
    }
    fn preview(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(100.0), 340.0),
            Sense::hover(),
        );
        let painter = ui.painter().with_clip_rect(rect);
        let checker = 12.0;
        for y in 0..(rect.height() / checker).ceil() as i32 {
            for x in 0..(rect.width() / checker).ceil() as i32 {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        rect.min + Vec2::new(x as f32 * checker, y as f32 * checker),
                        Vec2::splat(checker),
                    ),
                    0.0,
                    Color32::from_gray(if (x + y) % 2 == 0 { 190 } else { 235 }),
                );
            }
        }
        if let Some(texture) = &self.texture {
            let size = texture.size_vec2();
            let factor = (rect.width() / size.x).min(rect.height() / size.y);
            painter.image(
                texture.id(),
                egui::Rect::from_center_size(rect.center(), size * factor),
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let (status, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
        ui.scope_builder(egui::UiBuilder::new().max_rect(status), |ui| {
            ui.horizontal(|ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                } else if self.is_ready() {
                    let bytes = self.ready.as_ref().unwrap().1.len();
                    ui.label(format!(
                        "{} · {:.2} MiB · {} × {} px",
                        self.options.format.label(),
                        bytes as f64 / 1048576.0,
                        self.options.width,
                        self.options.height
                    ));
                } else {
                    ui.spinner();
                    ui.label("Atualizando prévia e tamanho do arquivo…");
                }
            });
        });
    }
}
impl PaintAtlas {
    pub(super) fn open_export_as(&mut self) {
        if self.image_editor.is_some()
            || self.selection.floating.is_some()
            || self.gesture.is_some()
        {
            self.status = "Finalize a operação atual antes de exportar.".into();
            return;
        }
        if self.export_dialog.is_none() {
            let name = self
                .path
                .as_ref()
                .and_then(|p| p.file_stem())
                .and_then(|s| s.to_str())
                .unwrap_or("desenho")
                .to_owned();
            self.export_dialog = Some(Export::new(&self.doc, name));
        }
    }
    pub(super) fn export_as_ui(&mut self, ctx: &egui::Context) {
        let Some(mut export) = self.export_dialog.take() else {
            return;
        };
        export.poll(ctx);
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new(format!("Exportar como — {}", export.name))
            .id(egui::Id::new("export-as-window"))
            .open(&mut open)
            .default_size([780.0, 490.0])
            .default_pos(ctx.content_rect().center() - Vec2::new(390.0, 245.0))
            .min_width(640.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                let before = export.options.clone();
                ui.horizontal_top(|ui| {
                    ui.set_height(405.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(265.0, 405.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("export-controls")
                                .max_height(405.0)
                                .show(ui, |ui| export.controls(ui));
                        },
                    );
                    ui.add_sized([1.0, 405.0], egui::Separator::default().vertical());
                    ui.vertical(|ui| export.preview(ui));
                });
                if before != export.options {
                    export.changed = Instant::now();
                    export.error = None;
                }
                ui.separator();
                ui.horizontal(|ui| {
                    crate::theme::caption(ui, "Exporta uma cópia; o projeto permanece editável.");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        cancel = ui.button("Cancelar").clicked();
                        save = ui
                            .add_enabled(export.is_ready(), egui::Button::new("Exportar"))
                            .clicked();
                    });
                });
            });
        if save && let Some((options, bytes)) = &export.ready {
            let extension = options.format.extension();
            if let Some(mut path) = rfd::FileDialog::new()
                .add_filter(options.format.label(), &[extension])
                .set_file_name(format!("{}.{}", export.name, extension))
                .save_file()
            {
                path.set_extension(extension);
                match crate::document::atomic_write(&path, bytes) {
                    Ok(()) => {
                        self.status = format!("Imagem exportada: {}", path.display());
                        open = false;
                    }
                    Err(error) => {
                        export.error = Some(format!("Erro ao salvar: {error}"));
                    }
                }
            }
        }
        if open && !cancel {
            self.export_dialog = Some(export);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options(format: Format) -> Options {
        Options {
            format,
            width: 2,
            height: 1,
            transparent: true,
            background: [255; 3],
            quality: 95,
            sampling: Sampling::Bicubic,
            resolution: true,
            dpi: 300,
        }
    }
    #[test]
    fn export_window_keeps_controls_and_footer_within_a_compact_window() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.export_dialog = Some(Export::new(&Document::new(2, 1, false), "test".into()));
        for _ in 0..4 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(1280.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ctx| app.export_as_ui(ctx),
            );
        }
        let bounds = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("export-as-window")))
            .unwrap();
        assert!(bounds.width() < 1000.0, "{bounds:?}");
        assert!(bounds.height() < 650.0, "{bounds:?}");
    }
    #[test]
    fn png_and_webp_preserve_exact_alpha_and_png_resolution_is_optional() {
        let source = [100, 50, 200, 128, 10, 20, 30, 255];
        for format in [Format::Png, Format::Webp] {
            let output = encode(&source, [2, 1], &options(format)).unwrap();
            let decoded = image::load_from_memory_with_format(&output.bytes, format.image_format())
                .unwrap()
                .into_rgba8();
            assert_eq!(decoded.dimensions(), (2, 1));
            assert_eq!(decoded.as_raw(), &source);
        }
        for resolution in [true, false] {
            let mut settings = options(Format::Png);
            settings.resolution = resolution;
            let output = encode(&source, [2, 1], &settings).unwrap();
            let reader = png::Decoder::new(std::io::Cursor::new(output.bytes))
                .read_info()
                .unwrap();
            assert_eq!(reader.info().pixel_dims.is_some(), resolution);
            if resolution {
                assert_eq!(reader.info().pixel_dims.unwrap().xppu, 11811);
            }
        }
    }
    #[test]
    fn background_mattes_transparency_and_jpeg_is_opaque() {
        let source = [0, 100, 200, 128, 0, 0, 0, 0];
        let mut settings = options(Format::Png);
        settings.transparent = false;
        settings.background = [200, 100, 0];
        let output = encode(&source, [2, 1], &settings).unwrap();
        let decoded = image::load_from_memory(&output.bytes).unwrap().into_rgba8();
        assert_eq!(decoded.as_raw(), &[100, 100, 100, 255, 200, 100, 0, 255]);
        settings.format = Format::Jpeg;
        settings.transparent = true;
        let output = encode(&source, [2, 1], &settings).unwrap();
        let decoded = image::load_from_memory(&output.bytes).unwrap().into_rgba8();
        assert!(decoded.pixels().all(|p| p[3] == 255));
        assert_eq!(output.preview.size, [2, 1]);
    }
    #[test]
    fn resizing_preserves_transparent_edge_colors_and_does_not_modify_document() {
        let mut doc = Document::new(2, 1, false);
        doc.layers[0]
            .pixels
            .copy_from_slice(&[240, 80, 20, 255, 0, 0, 0, 0]);
        let original = doc.layers[0].pixels.clone();
        let export = Export::new(&doc, "test".into());
        let mut settings = export.options.clone();
        settings.width = 8;
        let output = encode(&export.source, export.original, &settings).unwrap();
        let decoded = image::load_from_memory(&output.bytes).unwrap().into_rgba8();
        assert_eq!(decoded.dimensions(), (8, 1));
        for p in decoded.pixels().filter(|p| p[3] > 0) {
            assert_eq!(&p.0[..3], &[240, 80, 20]);
        }
        assert_eq!(doc.layers[0].pixels, original);
        settings.width = MAX_SIDE + 1;
        assert!(encode(&export.source, export.original, &settings).is_err());
    }
}
