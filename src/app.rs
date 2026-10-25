use crate::shortcuts::Action as Shortcut;
#[path = "canvas_backdrop.rs"]
mod canvas_backdrop;
#[path = "clipboard_motion.rs"]
mod clipboard_motion;
#[path = "clipboard_ui.rs"]
mod clipboard_ui;
#[path = "documents_ui.rs"]
mod documents_ui;
#[path = "export_ui.rs"]
mod export_ui;
#[path = "fill_reveal.rs"]
mod fill_reveal;
#[path = "filter_ui.rs"]
mod filter_ui;
#[path = "image_drop_ui.rs"]
mod image_drop_ui;
#[path = "image_menu.rs"]
mod image_menu;
#[path = "layer_menu.rs"]
mod layer_menu;
#[path = "picker_ui.rs"]
mod picker_ui;
#[path = "selection_ui.rs"]
mod selection_ui;
use crate::document::{Brush, Document, History, MAX_SIDE, Tool};
use crate::{
    painting::{MAX_BRUSH_SIZE, Stabilizer, pressure_size},
    presets::{Preset, Settings},
    view::CanvasView,
};
use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use std::path::PathBuf;

const ACCENT: Color32 = Color32::from_rgb(116, 202, 174);
#[derive(Clone, Copy)]
enum Action {
    New,
    Open,
    Close,
    CloseTab(u64),
    CloseAll,
}
struct Gesture {
    start: [f32; 2],
    last: [f32; 2],
    base: Option<Vec<u8>>,
    tool: Tool,
    filter: Stabilizer,
    sample_time: f64,
    raw_last: [f32; 2],
    last_size: f32,
    distance_to_dab: f32,
}

#[derive(Clone, Copy)]
struct LayerDrag(usize);

pub struct PaintAtlas {
    image_import: Option<crate::image_drop::Job>,
    image_editor: Option<image_menu::Editor>,
    export_dialog: Option<export_ui::Export>,
    last_filter: Option<crate::filters::Settings>,
    clipboard: crate::clipboard::SystemClipboard,
    clipboard_motion: clipboard_motion::State,
    documents: Vec<documents_ui::Tab>,
    active_document: usize,
    next_document_id: u64,
    canvas_backdrop_slot: Option<(egui::Painter, egui::layers::ShapeIdx)>,
    last_canvas_bounds: Option<egui::Rect>,
    root_window_origin: Pos2,
    root_pixels_per_point: f32,
    doc: Document,
    selection: selection_ui::State,
    picker: picker_ui::State,
    fill_reveal: fill_reveal::State,
    blend_fallback: Option<egui::TextureHandle>,
    blend_fallback_dirty: bool,
    blend_shader: Option<std::sync::Arc<crate::blend_gpu::Shader>>,
    selection_shader: Option<std::sync::Arc<crate::selection_gpu::Shader>>,
    glass_shader: Option<std::sync::Arc<crate::glass_gpu::Shader>>,
    history: History,
    textures: Vec<egui::TextureHandle>,
    tablet: crate::tablet::Tablet,
    pressure: f32,
    composite: Vec<u8>,
    refresh: bool,
    groups_prepared: bool,
    dirty: bool,
    path: Option<PathBuf>,
    tool: Tool,
    color: Color32,
    color_triangle: crate::color_selector::TriangleSelector,
    size: f32,
    opacity: f32,
    hardness: f32,
    tip_id: Option<String>,
    active_tip: Option<std::sync::Arc<crate::brush_tip::BrushTip>>,
    tip_library: crate::brush_tip::Library,
    spacing: f32,
    tolerance: u8,
    stabilization: f32,
    pressure_enabled: bool,
    pressure_gamma: f32,
    pressure_minimum: f32,
    settings: Settings,
    preferences: crate::preferences::Preferences,
    settings_path: Option<PathBuf>,
    settings_pending: bool,
    settings_edited: std::time::Instant,
    preview: Option<(Preset, egui::TextureHandle)>,
    brush_previews: Vec<Option<(Preset, egui::TextureHandle)>>,
    brush_editor: Option<Preset>,
    brush_editor_error: String,
    checker: Option<egui::TextureHandle>,
    angle: f32,
    rotation_target: Option<f32>,
    mirrored: bool,
    zoom: f32,
    zoom_animation: Option<(f32, Vec2)>,
    pan: Vec2,
    fit: bool,
    smooth_fit: bool,
    fit_animation: Option<f32>,
    selected_layers: std::collections::BTreeSet<usize>,
    collapsed_groups: std::collections::BTreeSet<u64>,
    gesture: Option<Gesture>,
    status: String,
    new_dialog: bool,
    startup_canvas_required: bool,
    splash_background: Option<egui::TextureHandle>,
    new_width: u32,
    new_height: u32,
    white: bool,
    paper_color: [u8; 3],
    new_dpi: u32,
    pending: Option<Action>,
    allow_close: bool,
    workspace: crate::workspace::Workspace,
    workspace_actions: Vec<crate::workspace::Action>,
    size_palette: crate::brush_sizes::BrushSizes,
    rename: String,
    rename_layer_focus: bool,
    opacity_edit: bool,
    layer_motion: Option<crate::layer_motion::LayerMotion>,
    layer_row_slides: Vec<(usize, f32)>,
    floating_panels: Vec<crate::workspace::Panel>,
    screenshot: Option<PathBuf>,
    frames: u32,
    maximize_on_start: bool,
}
impl PaintAtlas {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::from_context(&cc.egui_ctx);
        if app.screenshot.is_none()
            || std::env::var("PAINTATLAS_SMOKE_SCENE").is_ok_and(|scene| scene == "startup")
        {
            app.require_initial_canvas();
        }
        if let Some(gl) = &cc.gl {
            match crate::glass_gpu::Shader::new(gl.clone()) {
                Ok(shader) => {
                    cc.egui_ctx
                        .data_mut(|d| d.insert_temp(egui::Id::new("glass_shader"), shader.clone()));
                    app.glass_shader = Some(shader);
                }
                Err(e) => eprintln!("Material translúcido: desfoque indisponível: {e}"),
            }
            match crate::blend_gpu::Shader::new(gl.clone()) {
                Ok(shader) => app.blend_shader = Some(shader),
                Err(e) => eprintln!("Mesclagem de camadas OpenGL indisponível: {e}"),
            }
            match crate::selection_gpu::Shader::new(gl.clone()) {
                Ok(shader) => app.selection_shader = Some(shader),
                Err(e) => eprintln!("Prévia de seleção: composição OpenGL indisponível: {e}"),
            }
            use eframe::glow::HasContext;
            // GL is current on the creation thread.
            app.preferences.renderer = unsafe {
                format!(
                    "{} · {} · OpenGL {}",
                    gl.get_parameter_string(eframe::glow::VENDOR),
                    gl.get_parameter_string(eframe::glow::RENDERER),
                    gl.get_parameter_string(eframe::glow::VERSION)
                )
            };
        }
        app
    }
    fn from_context(ctx: &egui::Context) -> Self {
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let settings_path = crate::presets::settings_path();
        let settings = settings_path
            .as_ref()
            .and_then(|p| Settings::load(p).ok())
            .unwrap_or_default();
        crate::theme::install_fonts(ctx);
        crate::theme::apply(ctx, &settings.appearance);
        let workspace_path = std::env::var_os("PAINTATLAS_WORKSPACE")
            .map(PathBuf::from)
            .or_else(|| {
                settings_path
                    .as_ref()
                    .map(|p| p.with_file_name("workspace.bin"))
            });
        let canvas = settings.new_canvas.clone();
        let mut app = Self {
            image_import: None,
            image_editor: None,
            export_dialog: None,
            last_filter: None,
            clipboard: Default::default(),
            clipboard_motion: clipboard_motion::State::default(),
            documents: vec![documents_ui::Tab {
                id: 0,
                name: "Sem título".into(),
                session: None,
            }],
            active_document: 0,
            next_document_id: 1,
            canvas_backdrop_slot: None,
            last_canvas_bounds: None,
            root_window_origin: Pos2::ZERO,
            root_pixels_per_point: 1.0,
            selection: selection_ui::State::default(),
            picker: picker_ui::State::default(),
            fill_reveal: fill_reveal::State::default(),
            blend_fallback: None,
            blend_fallback_dirty: false,
            blend_shader: None,
            selection_shader: None,
            glass_shader: None,
            doc: Document::new(1280, 800, true),
            history: History::new(),
            textures: Vec::new(),
            #[cfg(not(test))]
            tablet: crate::tablet::Tablet::new(ctx.clone()),
            #[cfg(test)]
            tablet: crate::tablet::Tablet::without_hardware(ctx.clone()),
            pressure: 1.0,
            composite: Vec::new(),
            refresh: true,
            groups_prepared: false,
            dirty: false,
            path: None,
            tool: Tool::Brush,
            color: Color32::BLACK,
            color_triangle: crate::color_selector::TriangleSelector::default(),
            size: 12.0,
            opacity: 1.0,
            hardness: 0.9,
            tip_id: None,
            active_tip: None,
            tip_library: crate::brush_tip::Library::new(),
            spacing: 0.15,
            tolerance: 12,
            stabilization: 0.0,
            pressure_enabled: true,
            pressure_gamma: 0.5,
            pressure_minimum: 0.05,
            preferences: crate::preferences::Preferences::new(settings.render_gpu.clone()),
            settings,
            settings_path,
            settings_pending: false,
            settings_edited: std::time::Instant::now(),
            preview: None,
            brush_previews: Vec::new(),
            brush_editor: None,
            brush_editor_error: String::new(),
            checker: None,
            angle: 0.0,
            rotation_target: None,
            mirrored: false,
            zoom: 1.0,
            zoom_animation: None,
            pan: Vec2::ZERO,
            fit: true,
            smooth_fit: false,
            fit_animation: None,
            selected_layers: Default::default(),
            collapsed_groups: Default::default(),
            gesture: None,
            status: "Pronto para desenhar".into(),
            new_dialog: false,
            startup_canvas_required: false,
            splash_background: None,
            new_width: canvas.width,
            new_height: canvas.height,
            white: canvas.paper,
            paper_color: canvas.paper_color,
            new_dpi: canvas.dpi,
            pending: None,
            allow_close: false,
            workspace: crate::workspace::Workspace::new(workspace_path),
            workspace_actions: Vec::new(),
            size_palette: crate::brush_sizes::BrushSizes::default(),
            rename: "Camada 1".into(),
            rename_layer_focus: false,
            opacity_edit: false,
            layer_motion: None,
            layer_row_slides: Vec::new(),
            floating_panels: Vec::new(),
            screenshot: std::env::var_os("PAINTATLAS_SCREENSHOT")
                .or_else(|| std::env::var_os("PAINTATLAS_SIZES_SCREENSHOT"))
                .map(PathBuf::from),
            frames: 0,
            maximize_on_start: true,
        };
        app.apply_preset(app.settings.selected);
        if std::env::var_os("PAINTATLAS_SIZES_SCREENSHOT").is_some()
            || std::env::args().any(|arg| arg == "--brush-sizes")
        {
            app.workspace.layout.apply(crate::workspace::Action::Open(
                crate::workspace::Panel::BrushSizes,
            ));
        }
        if app.screenshot.is_some()
            && let Ok(scene) = std::env::var("PAINTATLAS_SMOKE_SCENE")
        {
            if scene == "appearance" {
                app.preferences.appearance_tab();
                app.workspace.layout.apply(crate::workspace::Action::Open(
                    crate::workspace::Panel::Settings,
                ));
            }
            if scene == "image-drop"
                && let Some(path) = &app.screenshot
            {
                let _ = std::fs::write(path.with_extension("pid"), std::process::id().to_string());
            }
            if scene == "shortcuts" {
                app.preferences.shortcuts_tab();
                app.workspace.layout.apply(crate::workspace::Action::Float(
                    crate::workspace::Panel::Settings,
                ));
                let surface = app
                    .workspace
                    .layout
                    .dock
                    .find_tab(&crate::workspace::Panel::Settings)
                    .unwrap()
                    .0;
                app.workspace
                    .layout
                    .dock
                    .get_window_state_mut(surface)
                    .unwrap()
                    .set_position(Pos2::new(350.0, 100.0))
                    .set_size(Vec2::new(650.0, 760.0));
            }
            if scene == "settings" {
                app.workspace.layout.apply(crate::workspace::Action::Open(
                    crate::workspace::Panel::Settings,
                ));
            }
            app.doc = Document::new(640, 480, true);
            let b = Brush {
                color: [71, 139, 121, 255],
                size: 8.0,
                opacity: 1.0,
                hardness: 1.0,
                tip: None,
                spacing: 0.15,
            };
            app.doc
                .shape(Tool::Ellipse, [120.0, 100.0], [480.0, 370.0], b.clone());
            app.doc.shape(
                Tool::Line,
                [120.0, 100.0],
                [470.0, 350.0],
                Brush {
                    color: [198, 103, 104, 255],
                    ..b
                },
            );
            app.doc.layers[1].alpha_lock = true;
            if scene == "brushes" || scene == "brush-editor" {
                use crate::workspace::{Action as DockAction, Panel};
                app.workspace
                    .layout
                    .apply(DockAction::Float(Panel::Brushes));
                let surface = app
                    .workspace
                    .layout
                    .dock
                    .find_tab(&Panel::Brushes)
                    .unwrap()
                    .0;
                app.workspace
                    .layout
                    .dock
                    .get_window_state_mut(surface)
                    .unwrap()
                    .set_position(Pos2::new(250.0, 100.0))
                    .set_size(Vec2::new(300.0, 650.0));
                app.size = 48.0;
                if scene == "brush-editor" {
                    app.open_brush_editor();
                }
            }
            if scene == "textures" || scene == "textures-editor" {
                app.doc = Document::new(800, 600, true);
                for (row, (file, name)) in [
                    ("hard-square.png", "Ponta quadrada"),
                    ("hexagon.png", "Hexágono"),
                    ("grass-detailer.png", "Grama"),
                ]
                .into_iter()
                .enumerate()
                {
                    let mut preset = app.current_preset();
                    preset.name = name.into();
                    preset.texture = Some(format!("default:{file}"));
                    preset.size = 72.0;
                    preset.pressure = false;
                    preset.spacing = 0.8;
                    preset.opacity = 1.0;
                    preset.hardness = 1.0;
                    app.create_brush(preset).unwrap();
                    let mut brush = app.brush();
                    brush.color = [54, 130, 111, 255];
                    let y = 120.0 + row as f32 * 155.0;
                    app.doc.stamp([80.0, y], brush.clone(), false);
                    let mut last = [170.0, y];
                    let mut remaining = 0.0;
                    for x in 171..700 {
                        let point = [
                            x as f32,
                            y + ((x - 170) as f32 / 530.0 * std::f32::consts::TAU).sin() * 30.0,
                        ];
                        app.doc.stroke_segment(
                            last,
                            point,
                            brush.size,
                            brush.clone(),
                            false,
                            &mut remaining,
                        );
                        last = point;
                    }
                }
                use crate::workspace::{Action as DockAction, Panel};
                app.workspace
                    .layout
                    .apply(DockAction::Float(Panel::Brushes));
                let surface = app
                    .workspace
                    .layout
                    .dock
                    .find_tab(&Panel::Brushes)
                    .unwrap()
                    .0;
                app.workspace
                    .layout
                    .dock
                    .get_window_state_mut(surface)
                    .unwrap()
                    .set_position(Pos2::new(240.0, 80.0))
                    .set_size(Vec2::new(300.0, 800.0));
                if scene == "textures-editor" {
                    app.open_brush_editor();
                }
            }
            if scene == "stabilizer" || scene == "stabilizer-live" {
                app.doc = Document::new(800, 600, true);
                app.size = 5.0;
                app.opacity = 1.0;
                app.hardness = 1.0;
                app.spacing = 0.15;
                app.pressure_enabled = false;
                app.stabilization = 0.0;
                app.tip_id = None;
                app.active_tip = None;
            }
            if scene.starts_with("selection") {
                app.doc.layers[app.doc.active].alpha_lock = false;
                app.tool = Tool::SelectRect;
                app.selection.mask = crate::selection::Mask::rectangle(
                    app.doc.width,
                    app.doc.height,
                    egui::Rect::from_min_size(Pos2::new(140.0, 90.0), Vec2::new(250.0, 250.0)),
                )
                .map(std::sync::Arc::new);
                app.doc.selection = app.selection.mask.clone();
                if scene.starts_with("selection-transform") {
                    let alpha = scene.contains("alpha");
                    if alpha {
                        app.doc.layers[app.doc.active].opacity = 0.4;
                    }
                    app.begin_transform(ctx, alpha);
                    if !alpha && let Some(f) = &mut app.selection.floating {
                        f.bounds = egui::Rect::from_min_size(
                            Pos2::new(245.0, 130.0),
                            Vec2::new(210.0, 210.0),
                        );
                    }
                    if scene.contains("rotation")
                        && let Some(f) = &mut app.selection.floating
                    {
                        f.angle = 30.0_f32.to_radians();
                    }
                    if scene.ends_with("applied") {
                        app.apply_transform();
                    }
                }
            }
            if scene.starts_with("blend-modes") {
                app.doc = Document::new(600, 360, false);
                app.doc.layers[0].pixels = if scene.contains("alpha") {
                    [128, 64, 192, 80].repeat(600 * 360)
                } else {
                    [128, 64, 192, 255].repeat(600 * 360)
                };
                for (i, mode) in crate::blend::BlendMode::ALL.into_iter().enumerate() {
                    app.doc.add_layer();
                    let layer = &mut app.doc.layers[app.doc.active];
                    layer.name = mode.label().into();
                    layer.blend_mode = mode;
                    layer.opacity = 0.65;
                    for y in i / 6 * 120..(i / 6 + 1) * 120 {
                        for x in i % 6 * 100..(i % 6 + 1) * 100 {
                            let index = (y * 600 + x) * 4;
                            let rgba = if scene.contains("alpha") && x % 100 >= 50 {
                                [240, 100, 20, 100]
                            } else {
                                [64, 192, 128, 200]
                            };
                            layer.pixels[index..index + 4].copy_from_slice(&rgba);
                        }
                    }
                }
                app.rename = app.doc.layers[app.doc.active].name.clone();
                if scene.contains("selection") {
                    app.selection.mask = crate::selection::Mask::rectangle(
                        600,
                        360,
                        egui::Rect::from_min_size(Pos2::new(510.0, 250.0), Vec2::new(60.0, 60.0)),
                    )
                    .map(std::sync::Arc::new);
                    app.doc.selection = app.selection.mask.clone();
                    app.begin_transform(ctx, false);
                    if let Some(f) = &mut app.selection.floating {
                        f.angle = 30.0_f32.to_radians();
                        f.bounds = f.bounds.translate(Vec2::new(-30.0, -10.0));
                    }
                    if scene.ends_with("applied") {
                        app.apply_transform();
                    }
                }
            }
            if scene == "triangle-color" {
                app.settings.triangular_color_selector = true;
                app.color = Color32::from_rgb(134, 72, 191);
            }
            if scene == "native" {
                app.workspace
                    .layout
                    .apply(crate::workspace::Action::External(
                        crate::workspace::Panel::Colors,
                    ));
            }
            if scene.starts_with("documents") {
                app.path = Some(PathBuf::from("Estudo.patlas"));
                app.dirty = scene == "documents-unsaved";
                app.add_document(
                    Document::with_paper(800, 600, Some([240, 220, 185]), 300),
                    Some(PathBuf::from("Paisagem.patlas")),
                    false,
                );
                app.add_document(Document::new(480, 640, false), None, false);
                app.switch_document(0);
                if scene != "documents" {
                    app.request(Action::CloseAll);
                }
            }
            if scene.starts_with("clipboard-paste") {
                let mut pixels = vec![0; 192 * 128 * 4];
                for y in 0..128 {
                    for x in 0..192 {
                        let dx = (x as f32 - 96.0) / 88.0;
                        let dy = (y as f32 - 64.0) / 56.0;
                        if dx * dx + dy * dy < 1.0 {
                            let i = (y * 192 + x) * 4;
                            pixels[i..i + 4].copy_from_slice(&[40, 130, 230, 180]);
                        }
                    }
                }
                app.paste_image(
                    ctx,
                    crate::clipboard::Image {
                        size: [192, 128],
                        pixels,
                    },
                )
                .unwrap();
                if scene.ends_with("applied") {
                    app.apply_transform();
                }
            }
            if scene.starts_with("glass-canvas") {
                app.doc = Document::new(512, 512, false);
                let colors = [
                    [235, 100, 115, 255],
                    [75, 160, 230, 255],
                    [245, 190, 75, 255],
                    [90, 190, 150, 255],
                ];
                for y in 0..512 {
                    for x in 0..512 {
                        let i = (y * 512 + x) * 4;
                        app.doc.layers[0].pixels[i..i + 4]
                            .copy_from_slice(&colors[(x / 64 + y / 64) % 4]);
                    }
                }
                app.zoom = 4.0;
                app.fit = false;
                if scene.ends_with("native") {
                    app.workspace
                        .layout
                        .apply(crate::workspace::Action::External(
                            crate::workspace::Panel::Colors,
                        ));
                    if let Some(panel) = app.workspace.layout.external.last_mut() {
                        panel.position = Some(Pos2::new(180.0, 140.0));
                        panel.size = Vec2::new(380.0, 620.0);
                    }
                }
                if scene.ends_with("floating") {
                    app.workspace.layout.apply(crate::workspace::Action::Open(
                        crate::workspace::Panel::Settings,
                    ));
                    app.preferences.appearance_tab();
                }
            }
            if scene == "filter-gallery" {
                app.open_filter(ctx, crate::filters::Kind::GaussianBlur, true, false);
            }
            if scene == "layer-groups" || scene == "group-effects" {
                app.doc.add_layer();
                app.doc.layers[2].name = "Detalhes".into();
                app.doc.group_layers(&[1, 2]);
                app.doc.layers[app.doc.active].name = "Personagem".into();
                app.rename = app.doc.layers[app.doc.active].name.clone();
                app.doc.layers[app.doc.active].label_color = Some([96, 177, 227]);
                app.changed();
                if scene == "group-effects" {
                    app.open_adjustment(ctx, crate::image_ops::Adjustment::Invert);
                }
            }
            if scene == "export-as" {
                app.open_export_as();
            }
            if scene == "layer-transform" {
                app.begin_transform(ctx, false);
            }
            if scene == "image-adjustment" {
                app.open_adjustment(ctx, crate::image_ops::Adjustment::Curves);
            }
            if scene == "rotated" {
                app.angle = 30.0_f32.to_radians();
                app.mirrored = true;
            }
        }
        app
    }
    fn current_preset(&self) -> Preset {
        let mut p = self.settings.brushes[self.settings.selected].clone();
        p.size = self.size;
        p.opacity = self.opacity;
        p.hardness = self.hardness;
        p.texture = self.tip_id.clone();
        p.spacing = self.spacing;
        p.stabilization = self.stabilization;
        p.pressure = self.pressure_enabled;
        p.gamma = self.pressure_gamma;
        p.minimum = self.pressure_minimum;
        p
    }
    fn remember_preset(&mut self) {
        let p = self.current_preset();
        if p != self.settings.brushes[self.settings.selected] {
            self.settings.brushes[self.settings.selected] = p;
            self.settings_pending = true;
            self.settings_edited = std::time::Instant::now();
        }
    }
    fn apply_preset(&mut self, index: usize) {
        self.settings.selected = index;
        if self.settings.brushes[index].eraser {
            self.settings.last_eraser = Some(index);
        } else {
            self.settings.last_brush = Some(index);
        }
        let p = &self.settings.brushes[index];
        self.size = p.size;
        self.opacity = p.opacity;
        self.hardness = p.hardness;
        self.stabilization = p.stabilization;
        self.pressure_enabled = p.pressure;
        self.pressure_gamma = p.gamma;
        self.pressure_minimum = p.minimum;
        self.spacing = p.spacing;
        self.tip_id = p.texture.clone();
        self.active_tip = self
            .tip_id
            .as_deref()
            .and_then(|id| match self.tip_library.get(id) {
                Ok(tip) => Some(tip),
                Err(e) => {
                    self.status = format!("Ponta indisponível; usando círculo: {e}");
                    None
                }
            });
        self.tool = if p.eraser { Tool::Eraser } else { Tool::Brush };
    }
    fn select_preset(&mut self, index: usize) {
        self.remember_preset();
        self.apply_preset(index);
        self.settings_pending = true;
        self.settings_edited = std::time::Instant::now();
    }
    fn select_tool(&mut self, tool: Tool) {
        if self.selection.floating.is_some() {
            self.status = format!(
                "Aplique ({}) ou cancele ({}) a transformação antes de trocar de ferramenta.",
                self.settings.shortcuts.label(Shortcut::Apply),
                self.settings.shortcuts.label(Shortcut::Cancel)
            );
            return;
        }
        if matches!(tool, Tool::Brush | Tool::Eraser)
            && self.settings.brushes[self.settings.selected].eraser != (tool == Tool::Eraser)
            && let Some(index) = self.settings.last_for_tool(tool == Tool::Eraser)
        {
            self.select_preset(index);
        }
        self.tool = tool;
    }
    fn persist_presets(&mut self, ctx: &egui::Context, force: bool) {
        self.remember_preset();
        if self.settings_pending {
            if force || self.settings_edited.elapsed() >= std::time::Duration::from_millis(500) {
                if let Some(path) = &self.settings_path
                    && let Err(e) = self.settings.save(path)
                {
                    self.status = format!("Erro ao salvar configurações: {e}");
                }
                self.settings_pending = false;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
    }
    fn preset_texture(
        ctx: &egui::Context,
        preset: &Preset,
        tip: Option<std::sync::Arc<crate::brush_tip::BrushTip>>,
    ) -> egui::TextureHandle {
        let mut d = Document::new(180, 48, true);
        let mut last = [10.0, 30.0];
        let mut last_size = 1.0;
        let mut distance_to_dab = 0.0;
        for i in 1..=160 {
            let t = i as f32 / 160.0;
            let point = [
                10.0 + i as f32,
                24.0 + (t * std::f32::consts::TAU).sin() * 10.0,
            ];
            let factor = if preset.pressure {
                pressure_size(
                    (t * std::f32::consts::PI).sin().max(0.0),
                    preset.gamma,
                    preset.minimum,
                )
            } else {
                1.0
            };
            let b = Brush {
                color: [30, 39, 48, 255],
                size: (preset.size * 0.35).clamp(1.0, 22.0) * factor,
                opacity: preset.opacity,
                hardness: preset.hardness,
                tip: tip.clone(),
                spacing: preset.spacing,
            };
            let end_size = b.size;
            d.stroke_segment(last, point, last_size, b, false, &mut distance_to_dab);
            last = point;
            last_size = end_size;
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([180, 48], &d.composite());
        ctx.load_texture("brush_preview", img, egui::TextureOptions::LINEAR)
    }
    fn brush_preview(&mut self, ui: &mut egui::Ui, preset: &Preset) {
        if self.preview.as_ref().is_none_or(|(p, _)| p != preset) {
            let tip = preset
                .texture
                .as_deref()
                .and_then(|id| self.tip_library.get(id).ok());
            self.preview = Some((preset.clone(), Self::preset_texture(ui.ctx(), preset, tip)));
        }
        if let Some((_, texture)) = &self.preview {
            let width = ui.available_width().min(360.0);
            ui.image((texture.id(), Vec2::new(width, width * 48.0 / 180.0)));
        }
    }
    fn preset_modified(&self) -> bool {
        self.current_preset() != self.settings.originals[self.settings.selected]
    }
    fn open_brush_editor(&mut self) {
        let mut p = self.current_preset();
        let base = format!("{} — cópia", p.name.chars().take(80).collect::<String>());
        p.name = base.clone();
        let mut n = 2;
        while self.settings.brushes.iter().any(|b| b.name == p.name) {
            p.name = format!("{base} {n}");
            n += 1;
        }
        self.brush_editor = Some(p);
        self.brush_editor_error.clear();
    }
    fn create_brush(&mut self, preset: Preset) -> Result<(), String> {
        self.remember_preset();
        let i = self.settings.add(preset)?;
        self.apply_preset(i);
        self.settings_pending = true;
        self.settings_edited = std::time::Instant::now();
        self.status = format!("Pincel criado: {}", self.settings.brushes[i].name);
        Ok(())
    }
    fn changed(&mut self) {
        self.fill_reveal.clear();
        self.doc.rebuild_groups(self.doc.damage);
        self.groups_prepared = true;
        self.refresh = true;
        self.dirty = true;
    }
    fn set_layer_opacity(&mut self, opacity: f32) {
        if self.doc.layers[self.doc.active].opacity == opacity {
            return;
        }
        if !self.opacity_edit {
            self.history.push_layer_properties(&self.doc);
            self.opacity_edit = true;
        }
        self.doc.layers[self.doc.active].opacity = opacity;
        if self.doc.layers[self.doc.active].parent.is_some() {
            self.doc.rebuild_groups(None);
            self.groups_prepared = true;
            self.doc.damage = None;
            self.refresh = true;
        }
        self.dirty = true;
        // Opacity is a draw parameter, not a pixel edit. Reuse GPU textures.
        // Only the CPU blend fallback needs its cached composite rebuilt.
        if self.blend_shader.is_none()
            && self
                .doc
                .layers
                .iter()
                .any(|layer| layer.visible && layer.blend_mode != crate::blend::BlendMode::Normal)
        {
            self.blend_fallback_dirty = true;
        }
    }
    fn checkpoint(&mut self) {
        self.doc.damage = None;
        self.history.push(&self.doc);
    }
    fn brush(&self) -> Brush {
        Brush {
            color: self.color.to_array(),
            size: (self.size
                * if self.pressure_enabled {
                    pressure_size(self.pressure, self.pressure_gamma, self.pressure_minimum)
                } else {
                    1.0
                })
            .max(1.0),
            opacity: self.opacity,
            hardness: self.hardness,
            tip: self.active_tip.clone(),
            spacing: self.spacing,
        }
    }
    fn reset(&mut self, doc: Document, path: Option<PathBuf>, dirty: bool) {
        self.clipboard_motion.cancel_flight();
        self.picker = picker_ui::State::default();
        self.fill_reveal.clear();
        self.clear_selection();
        self.startup_canvas_required = false;
        self.new_dialog = false;
        self.layer_motion = None;
        self.layer_row_slides.clear();
        self.doc = doc;
        self.groups_prepared = false;
        self.doc.damage = None;
        self.path = path;
        self.history = History::new();
        self.dirty = dirty;
        self.refresh = true;
        self.gesture = None;
        self.fit = true;
        self.smooth_fit = false;
        self.fit_animation = None;
        self.selected_layers.clear();
        self.collapsed_groups.clear();
        self.zoom_animation = None;
        self.angle = 0.0;
        self.rotation_target = None;
        self.mirrored = false;
        self.rename = self.doc.layers[self.doc.active].name.clone();
    }
    fn request(&mut self, action: Action) {
        if !self.document_change_allowed() {
            return;
        }
        match action {
            Action::New | Action::Open => self.perform(action),
            Action::CloseTab(id) => {
                let Some(index) = self.documents.iter().position(|tab| tab.id == id) else {
                    return;
                };
                self.switch_document(index);
                self.commit_name();
                if self.dirty {
                    self.pending = Some(action);
                } else {
                    self.close_active_document();
                }
            }
            Action::Close | Action::CloseAll => {
                self.continue_closing_documents(matches!(action, Action::Close));
            }
        }
    }
    fn perform(&mut self, action: Action) {
        match action {
            Action::New => self.new_dialog = true,
            Action::CloseTab(id) => {
                if self
                    .documents
                    .get(self.active_document)
                    .is_some_and(|tab| tab.id == id)
                {
                    self.close_active_document();
                }
            }
            Action::Close | Action::CloseAll => {
                self.close_active_document();
                self.continue_closing_documents(matches!(action, Action::Close));
            }
            Action::Open => {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter(
                        "Projetos ou imagens",
                        &["patlas", "png", "jpg", "jpeg", "webp", "gif", "bmp"],
                    )
                    .pick_files()
                {
                    let mut errors = Vec::new();
                    for path in paths {
                        if let Err(error) = self.open_document(path.clone()) {
                            errors.push(format!("{}: {error}", path.display()));
                        }
                    }
                    self.status = if errors.is_empty() {
                        "Arquivos abertos em abas".into()
                    } else {
                        format!("Erro ao abrir: {}", errors.join("; "))
                    };
                }
            }
        }
    }
    fn commit_name(&mut self) {
        let name: String = self.rename.trim().chars().take(100).collect();
        if !name.is_empty() && name != self.doc.layers[self.doc.active].name {
            self.checkpoint();
            self.doc.layers[self.doc.active].name = name;
            self.changed();
        }
    }
    fn save(&mut self, save_as: bool) -> bool {
        if self.image_editor.is_some() {
            self.status = "Aplique ou cancele a operação de imagem antes de salvar.".into();
            return false;
        }
        if self.selection.floating.is_some() {
            self.status = "Aplique ou cancele a transformação antes de salvar.".into();
            return false;
        }
        self.commit_name();
        let path = if save_as || self.path.is_none() {
            rfd::FileDialog::new()
                .add_filter("Projeto PaintAtlas", &["patlas"])
                .set_file_name("Sem título.patlas")
                .save_file()
                .map(|mut p| {
                    p.set_extension("patlas");
                    p
                })
        } else {
            self.path.clone()
        };
        let Some(path) = path else { return false };
        match self.doc.save(&path) {
            Ok(()) => {
                self.status = format!("Projeto salvo: {}", path.display());
                self.path = Some(path);
                self.dirty = false;
                true
            }
            Err(e) => {
                self.status = format!("Erro ao salvar: {e}");
                false
            }
        }
    }
    fn export(&mut self) {
        if self.image_editor.is_some() {
            self.status = "Aplique ou cancele a operação de imagem antes de exportar.".into();
            return;
        }
        if let Some(mut path) = rfd::FileDialog::new()
            .add_filter("Imagem PNG", &["png"])
            .set_file_name("desenho.png")
            .save_file()
        {
            path.set_extension("png");
            self.status = match self.doc.export(&path) {
                Ok(()) => format!("PNG exportado: {}", path.display()),
                Err(e) => format!("Erro ao exportar: {e}"),
            };
        }
    }
    fn undo(&mut self) {
        self.clipboard_motion.cancel_flight();
        self.fill_reveal.clear();
        self.selected_layers.clear();
        self.layer_motion = None;
        self.layer_row_slides.clear();
        if self.history.undo(&mut self.doc) {
            self.clear_selection();
            self.doc.damage = None;
            self.changed();
            self.rename = self.doc.layers[self.doc.active].name.clone();
            let (undo, redo) = self.history.counts();
            self.status = format!("Desfeito · {undo} para desfazer · {redo} para refazer");
        } else {
            self.status = "Não há mais ações para desfazer.".into();
        }
    }
    fn redo(&mut self) {
        self.clipboard_motion.cancel_flight();
        self.fill_reveal.clear();
        self.selected_layers.clear();
        self.layer_motion = None;
        self.layer_row_slides.clear();
        if self.history.redo(&mut self.doc) {
            self.clear_selection();
            self.doc.damage = None;
            self.changed();
            self.rename = self.doc.layers[self.doc.active].name.clone();
            let (undo, redo) = self.history.counts();
            self.status = format!("Refeito · {undo} para desfazer · {redo} para refazer");
        } else {
            self.status = "Não há ações para refazer.".into();
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.preferences.capturing_shortcut() || self.image_editor.is_some() {
            return;
        }
        if self.clipboard_shortcuts(ctx) {
            return;
        }
        if self.selection.floating.is_some() {
            self.selection_shortcuts(ctx);
            return;
        }
        if ctx.wants_keyboard_input()
            || self.pending.is_some()
            || self.new_dialog
            || self.gesture.is_some()
        {
            return;
        }
        if self.selection_shortcuts(ctx) {
            return;
        }
        let actions = ctx.input_mut(|i| {
            Shortcut::ALL
                .iter()
                .copied()
                .filter(|a| {
                    !a.is_held()
                        && a.scope() & 1 != 0
                        && !matches!(
                            a,
                            Shortcut::ZoomIn | Shortcut::ZoomOut | Shortcut::ResetZoom
                        )
                        && self.settings.shortcuts.consume(i, *a)
                })
                .collect::<Vec<_>>()
        });
        for action in actions {
            if self.image_shortcut(ctx, action) {
                continue;
            }
            if self.image_editor.is_some() {
                break;
            }
            match action {
                Shortcut::LayerGroup => {
                    self.apply_layer_action(self.doc.active, layer_menu::LayerAction::Group)
                }
                Shortcut::LayerMerge => {
                    self.apply_layer_action(self.doc.active, layer_menu::LayerAction::MergeSelected)
                }
                Shortcut::New => self.request(Action::New),
                Shortcut::Open => self.request(Action::Open),
                Shortcut::Save => {
                    self.save(false);
                }
                Shortcut::SaveAs => {
                    self.save(true);
                }
                Shortcut::Export => self.export(),
                Shortcut::ExportAs => self.open_export_as(),
                Shortcut::CloseTab => {
                    if let Some(tab) = self.documents.get(self.active_document) {
                        self.request(Action::CloseTab(tab.id));
                    }
                }
                Shortcut::CloseAll => self.request(Action::CloseAll),
                Shortcut::Settings => self.workspace_actions.push(crate::workspace::Action::Open(
                    crate::workspace::Panel::Settings,
                )),
                Shortcut::Undo => self.undo(),
                Shortcut::Redo => self.redo(),
                Shortcut::Brush => self.select_tool(Tool::Brush),
                Shortcut::Eraser => self.select_tool(Tool::Eraser),
                Shortcut::Fill => self.select_tool(Tool::Fill),
                Shortcut::Picker => self.select_tool(Tool::Picker),
                Shortcut::Line => self.select_tool(Tool::Line),
                Shortcut::Rectangle => self.select_tool(Tool::Rectangle),
                Shortcut::Ellipse => self.select_tool(Tool::Ellipse),
                Shortcut::Hand => self.select_tool(Tool::Hand),
                Shortcut::SelectRect => self.select_tool(Tool::SelectRect),
                Shortcut::Lasso => self.select_tool(Tool::Lasso),
                Shortcut::SmallerBrush => self.size = (self.size - 2.0).max(1.0),
                Shortcut::LargerBrush => self.size = (self.size + 2.0).min(MAX_BRUSH_SIZE),
                Shortcut::BrushSizes => self.workspace_actions.push(
                    crate::workspace::Action::Open(crate::workspace::Panel::BrushSizes),
                ),
                Shortcut::RotateLeft => self.request_rotation(-15.0_f32.to_radians()),
                Shortcut::RotateRight => self.request_rotation(15.0_f32.to_radians()),
                Shortcut::Mirror => self.mirrored = !self.mirrored,
                Shortcut::Fit => {
                    self.fit = true;
                    self.smooth_fit = true;
                }
                _ => {}
            }
        }
    }
    fn top(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::side_top_panel(&ctx.style()).corner_radius(8))
            .show(ctx, |ui| {
                if self.image_editor.is_some() {
                    ui.visuals_mut().disabled_alpha = 1.0;
                }
                if self.gesture.is_some()
                    || self.selection.floating.is_some()
                    || self.pending.is_some()
                    || self.new_dialog
                    || self.image_editor.is_some()
                {
                    ui.disable();
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("PAINT")
                            .strong()
                            .size(20.0)
                            .color(crate::theme::accent(ctx)),
                    );
                    ui.label(egui::RichText::new("ATLAS").size(20.0));
                    ui.separator();
                    ui.menu_button("Arquivo", |ui| {
                        if crate::theme::button(
                            ui,
                            self.settings.shortcuts.menu(Shortcut::New, "Novo…"),
                        )
                        .clicked()
                        {
                            self.request(Action::New);
                            ui.close();
                        }
                        if crate::theme::button(
                            ui,
                            self.settings.shortcuts.menu(Shortcut::Open, "Abrir…"),
                        )
                        .clicked()
                        {
                            self.request(Action::Open);
                            ui.close();
                        }
                        if crate::theme::button(
                            ui,
                            self.settings
                                .shortcuts
                                .menu(Shortcut::CloseTab, "Fechar tela"),
                        )
                        .clicked()
                        {
                            if let Some(tab) = self.documents.get(self.active_document) {
                                self.request(Action::CloseTab(tab.id));
                            }
                            ui.close();
                        }
                        if crate::theme::button(
                            ui,
                            self.settings
                                .shortcuts
                                .menu(Shortcut::CloseAll, "Fechar todas as telas"),
                        )
                        .clicked()
                        {
                            self.request(Action::CloseAll);
                            ui.close();
                        }
                        if crate::theme::button(
                            ui,
                            self.settings
                                .shortcuts
                                .menu(Shortcut::Save, "Salvar projeto"),
                        )
                        .clicked()
                        {
                            self.save(false);
                            ui.close();
                        }
                        if crate::theme::button(
                            ui,
                            self.settings
                                .shortcuts
                                .menu(Shortcut::SaveAs, "Salvar como…"),
                        )
                        .clicked()
                        {
                            self.save(true);
                            ui.close();
                        }
                        if crate::theme::button(
                            ui,
                            self.settings
                                .shortcuts
                                .menu(Shortcut::Export, "Exportar PNG rápido…"),
                        )
                        .clicked()
                        {
                            self.export();
                            ui.close();
                        }
                        if ui
                            .button(
                                self.settings
                                    .shortcuts
                                    .menu(Shortcut::ExportAs, "Exportar como…"),
                            )
                            .clicked()
                        {
                            self.open_export_as();
                            ui.close();
                        }
                    });
                    ui.menu_button("Editar", |ui| {
                        if ui
                            .button(
                                self.settings
                                    .shortcuts
                                    .menu(Shortcut::Copy, "Copiar camada/seleção"),
                            )
                            .clicked()
                        {
                            self.copy_selection_to_clipboard(ctx, false);
                            ui.close();
                        }
                        if ui
                            .button(
                                self.settings
                                    .shortcuts
                                    .menu(Shortcut::CopyMerged, "Copiar mesclado"),
                            )
                            .clicked()
                        {
                            self.copy_selection_to_clipboard(ctx, true);
                            ui.close();
                        }
                        if ui
                            .button(
                                self.settings
                                    .shortcuts
                                    .menu(Shortcut::Paste, "Colar imagem"),
                            )
                            .clicked()
                        {
                            self.paste_from_clipboard(ctx);
                            ui.close();
                        }
                    });
                    self.image_menu(ui);
                    self.filter_menu(ui);
                    if crate::theme::button(ui, "Configurações").clicked() {
                        self.workspace_actions.push(crate::workspace::Action::Open(
                            crate::workspace::Panel::Settings,
                        ));
                    }
                    ui.menu_button("Janela", |ui| {
                        for panel in crate::workspace::Panel::ALL {
                            let present = self.workspace.layout.contains(panel);
                            if ui.selectable_label(present, panel.title()).clicked() {
                                self.workspace_actions
                                    .push(crate::workspace::Action::Open(panel));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if crate::theme::button(ui, "Restaurar layout padrão").clicked() {
                            self.workspace_actions.push(crate::workspace::Action::Reset);
                            ui.close();
                        }
                        crate::theme::caption(
                            ui,
                            "Arraste as abas para acoplar. Botão direito para mais opções.",
                        );
                    });
                    let (undo_count, redo_count) = self.history.counts();
                    if ui
                        .add_enabled(undo_count > 0, egui::Button::new("Desfazer"))
                        .on_hover_text(format!(
                            "{} · {undo_count} ações disponíveis",
                            self.settings.shortcuts.label(Shortcut::Undo)
                        ))
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(redo_count > 0, egui::Button::new("Refazer"))
                        .on_hover_text(format!(
                            "{} · {redo_count} ações disponíveis",
                            self.settings.shortcuts.label(Shortcut::Redo)
                        ))
                        .clicked()
                    {
                        self.redo();
                    }
                    ui.separator();
                    if !self.documents.is_empty() {
                        ui.label(self.document_title(self.active_document));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.clipboard_motion.badge_ui(ui) {
                            self.paste_from_clipboard(ctx);
                        }
                        if crate::theme::button(ui, "Exportar PNG rápido").clicked() {
                            self.export();
                        }
                        if crate::theme::button(ui, "Ajuda").clicked() {
                            self.workspace_actions.push(crate::workspace::Action::Open(
                                crate::workspace::Panel::Help,
                            ));
                        }
                    });
                });
                ui.add_space(4.0);
            });
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::side_top_panel(&ctx.style()).corner_radius(8))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&self.status).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!(
                            "{} × {} px   •   {} camadas",
                            self.doc.width,
                            self.doc.height,
                            self.doc.layers.len()
                        ));
                        ui.separator();
                        ui.label(format!("{}%", (self.zoom * 100.0).round()));
                    });
                });
            });
    }
    fn sync_layer_textures(&mut self, ctx: &egui::Context) {
        if self.group_transform_preview()
            || self.blend_shader.is_none()
                && self
                    .doc
                    .layers
                    .iter()
                    .any(|l| l.visible && l.blend_mode != crate::blend::BlendMode::Normal)
        {
            if self.refresh
                || self.blend_fallback_dirty
                || self.selection.floating.is_some()
                || self.blend_fallback.is_none()
            {
                let pixels = self.fallback_blend_composite();
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [self.doc.width as usize, self.doc.height as usize],
                    &pixels,
                );
                if let Some(texture) = &mut self.blend_fallback {
                    texture.set(image, egui::TextureOptions::NEAREST);
                } else {
                    self.blend_fallback = Some(ctx.load_texture(
                        "blend_fallback",
                        image,
                        egui::TextureOptions::NEAREST,
                    ));
                }
            }
        } else {
            self.blend_fallback = None;
        }
        self.blend_fallback_dirty = false;
        if self.refresh
            && self.doc.damage.is_some()
            && self.doc.layers.iter().any(|l| !l.effects.is_empty())
        {
            self.doc.damage = Some([0, 0, self.doc.width as usize, self.doc.height as usize]);
            self.doc.dirty_tiles.clear();
        }
        if self.refresh {
            if let Some(d) = self
                .doc
                .damage
                .take()
                .filter(|_| self.textures.len() == self.doc.layers.len())
            {
                // Merge adjacent dirty tiles into horizontal runs. Disconnected
                // marks never upload the untouched area between them.
                let tiles = std::mem::take(&mut self.doc.dirty_tiles);
                let mut runs: Vec<[usize; 4]> = Vec::with_capacity(tiles.len());
                for (ty, tx) in tiles {
                    let t = crate::document::PAINT_TILE;
                    let r = [
                        (tx * t).max(d[0]),
                        (ty * t).max(d[1]),
                        ((tx + 1) * t).min(d[2]),
                        ((ty + 1) * t).min(d[3]),
                    ];
                    if let Some(last) = runs.last_mut()
                        && last[1] == r[1]
                        && last[2] == r[0]
                    {
                        last[2] = r[2];
                    } else {
                        runs.push(r);
                    }
                }
                if runs.is_empty() {
                    runs.push(d);
                }
                // Child edits also invalidate their isolated ancestor composites.
                if !self.groups_prepared {
                    self.doc
                        .rebuild_groups_after(Some(d), Some(self.doc.active));
                }
                for index in 0..self.doc.layers.len() {
                    if self.doc.layers[index]
                        .group
                        .is_some_and(|id| self.doc.belongs_to(self.doc.active, id))
                    {
                        let pixels = &self.doc.layers[index].pixels;
                        let mut rgba = Vec::with_capacity((d[2] - d[0]) * (d[3] - d[1]) * 4);
                        for y in d[1]..d[3] {
                            rgba.extend_from_slice(
                                &pixels[(y * self.doc.width as usize + d[0]) * 4
                                    ..(y * self.doc.width as usize + d[2]) * 4],
                            );
                        }
                        self.textures[index].set_partial(
                            [d[0], d[1]],
                            egui::ColorImage::from_rgba_unmultiplied(
                                [d[2] - d[0], d[3] - d[1]],
                                &rgba,
                            ),
                            egui::TextureOptions::NEAREST,
                        );
                    }
                }
                let pixels = &self.doc.layers[self.doc.active].pixels;
                for r in runs {
                    let size = [r[2] - r[0], r[3] - r[1]];
                    // Build the upload image directly: no intermediate RGBA copy.
                    let mut colors = Vec::with_capacity(size[0] * size[1]);
                    for y in r[1]..r[3] {
                        let start = (y * self.doc.width as usize + r[0]) * 4;
                        colors.extend(
                            pixels[start..start + size[0] * 4]
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])),
                        );
                    }
                    self.textures[self.doc.active].set_partial(
                        [r[0], r[1]],
                        egui::ColorImage::new(size, colors),
                        egui::TextureOptions::NEAREST,
                    );
                }
            } else {
                self.textures.clear();
                for layer in &self.doc.layers {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [self.doc.width as usize, self.doc.height as usize],
                        &layer.pixels,
                    );
                    self.textures.push(ctx.load_texture(
                        "layer",
                        image,
                        egui::TextureOptions::NEAREST,
                    ));
                }
            }
            self.refresh = false;
            self.groups_prepared = false;
            ctx.request_repaint();
        }
        if self.checker.is_none() {
            let mut rgba = Vec::new();
            for y in 0..16 {
                for x in 0..16 {
                    let c = if (x / 8 + y / 8) % 2 == 0 { 205 } else { 235 };
                    rgba.extend_from_slice(&[c, c, c, 255]);
                }
            }
            self.checker = Some(ctx.load_texture(
                "checker",
                egui::ColorImage::from_rgba_unmultiplied([16, 16], &rgba),
                egui::TextureOptions {
                    wrap_mode: egui::TextureWrapMode::Repeat,
                    ..egui::TextureOptions::NEAREST
                },
            ));
        }
    }
    fn layer_preview_row(&mut self, ui: &mut egui::Ui, index: usize) -> egui::Response {
        let selected = self.doc.active == index || self.selected_layers.contains(&index);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::hover());
        let offset = self
            .layer_row_slides
            .iter()
            .find(|(i, _)| *i == index)
            .map_or(0.0, |(_, y)| *y);
        let rect = rect.translate(Vec2::new(0.0, offset));
        let response = ui.interact(
            rect,
            ui.id().with(("layer_row", index)),
            Sense::click_and_drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                ui.is_enabled(),
                selected,
                &self.doc.layers[index].name,
            )
        });
        let mut row_painter = ui.painter().clone();
        if let Some(motion) = &self.layer_motion
            && motion.index == index
        {
            row_painter.multiply_opacity(motion.row_opacity());
        }
        let visuals = crate::theme::row_visuals(ui, &response, selected);
        row_painter.rect(
            rect,
            10.0,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        if let Some([r, g, b]) = self.doc.layers[index].label_color {
            row_painter.rect_filled(
                egui::Rect::from_min_max(
                    Pos2::new(rect.right() - 6.0, rect.top() + 8.0),
                    Pos2::new(rect.right() - 3.0, rect.bottom() - 8.0),
                ),
                2.0,
                Color32::from_rgb(r, g, b),
            );
        }
        let preview_box =
            egui::Rect::from_min_size(rect.min + Vec2::new(5.0, 6.0), Vec2::new(48.0, 40.0));
        row_painter.rect_filled(preview_box, 2.0, Color32::from_gray(18));
        let aspect = self.doc.width as f32 / self.doc.height as f32;
        let size = if aspect >= 1.2 {
            Vec2::new(48.0, 48.0 / aspect)
        } else {
            Vec2::new(40.0 * aspect, 40.0)
        };
        let image_rect = egui::Rect::from_center_size(preview_box.center(), size);
        if let Some(checker) = &self.checker {
            row_painter.image(
                checker.id(),
                image_rect,
                egui::Rect::from_min_size(Pos2::ZERO, size / 8.0),
                Color32::WHITE,
            );
        }
        // Show the layer's own pixels, including when the layer is hidden. Reuse
        // the canvas texture so drawing/undo update the preview without readback.
        if let Some(texture) = self.textures.get(index) {
            row_painter.image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        row_painter.rect_stroke(
            preview_box,
            2.0,
            Stroke::new(1.0_f32, Color32::from_gray(80)),
            egui::StrokeKind::Inside,
        );
        let text_rect = egui::Rect::from_min_max(
            rect.min + Vec2::new(61.0, 0.0),
            rect.max - Vec2::new(4.0, 0.0),
        );
        let painter = row_painter.with_clip_rect(text_rect);
        let layer = &self.doc.layers[index];
        painter.text(
            Pos2::new(text_rect.min.x, rect.min.y + 10.0),
            egui::Align2::LEFT_TOP,
            if layer.group.is_some() {
                format!("▣ {}", layer.name)
            } else {
                layer.name.clone()
            },
            egui::FontId::proportional(12.0),
            visuals.text_color(),
        );
        let mode_info = if layer.blend_mode == crate::blend::BlendMode::Normal {
            String::new()
        } else {
            format!(" · {}", layer.blend_mode.label())
        };
        let info = format!(
            "{:.0}%{}{}{}",
            layer.opacity * 100.0,
            mode_info,
            if layer.alpha_lock {
                " · alfa bloqueado"
            } else {
                ""
            },
            if layer.effects.is_empty() {
                String::new()
            } else {
                format!(" · {} efeito(s)", layer.effects.len())
            }
        );
        painter.text(
            Pos2::new(text_rect.min.x, rect.min.y + 29.0),
            egui::Align2::LEFT_TOP,
            info,
            egui::FontId::proportional(10.0),
            Color32::from_gray(150),
        );
        crate::theme::feedback(ui, &response);
        let response = response.on_hover_text(format!("{}\nArraste para reordenar", layer.name));
        response.dnd_set_drag_payload(LayerDrag(index));
        if response.drag_started() {
            self.layer_row_slides.clear();
            let layer = &self.doc.layers[index];
            // A small independent snapshot prevents the ghost changing when
            // canvas textures are reassigned after reordering.
            let mut pixels = Vec::with_capacity(96 * 96 * 4);
            for y in 0..96 {
                for x in 0..96 {
                    let px = x * self.doc.width as usize / 96;
                    let py = y * self.doc.height as usize / 96;
                    let offset = (py * self.doc.width as usize + px) * 4;
                    pixels.extend_from_slice(&layer.pixels[offset..offset + 4]);
                }
            }
            let texture = ui.ctx().load_texture(
                "layer_drag_snapshot",
                egui::ColorImage::from_rgba_unmultiplied([96, 96], &pixels),
                egui::TextureOptions::LINEAR,
            );
            self.layer_motion = Some(crate::layer_motion::LayerMotion::new(
                index,
                rect,
                ui.input(|i| i.pointer.press_origin())
                    .unwrap_or(rect.center()),
                crate::layer_motion::Card {
                    texture: Some(texture),
                    checker: self.checker.clone(),
                    name: layer.name.clone(),
                    info: format!("{:.0}%", layer.opacity * 100.0),
                    aspect: self.doc.width as f32 / self.doc.height as f32,
                },
            ));
        }
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if response.clicked() {
            self.select_layer(
                index,
                ui.input(|i| {
                    self.settings
                        .shortcuts
                        .held(i, Shortcut::LayerToggleSelection)
                }),
                ui.input(|i| {
                    self.settings
                        .shortcuts
                        .held(i, Shortcut::LayerRangeSelection)
                }),
            );
        }
        response
    }
    #[cfg(test)]
    fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_gray(18)))
            .show(ctx, |ui| self.canvas_ui(ui));
    }
    fn request_zoom(&mut self, factor: f32, reset: bool, anchor: Vec2) {
        self.fit_animation = None;
        let target = if reset {
            1.0
        } else {
            (self.zoom_animation.map_or(self.zoom, |(target, _)| target) * factor).clamp(0.05, 16.0)
        };
        self.zoom_animation = Some((target, anchor));
    }

    fn animate_zoom(&mut self, dt: f32) {
        let Some((target, anchor)) = self.zoom_animation else {
            return;
        };
        let old = self.zoom;
        // Interpolate logarithmically so zooming in and out have the same pace.
        let remaining = (target / old).ln();
        if self.settings.appearance.reduced_motion || remaining.abs() < 0.0005 {
            self.zoom = target;
            self.zoom_animation = None;
        } else {
            let blend = 1.0 - (-dt.clamp(0.0, 0.05) / 0.055).exp();
            self.zoom = old * (remaining * blend).exp();
        }
        self.pan = (self.pan - anchor) * (self.zoom / old) + anchor;
    }

    fn request_rotation(&mut self, delta: f32) {
        self.rotation_target = Some(self.rotation_target.unwrap_or(self.angle) + delta);
    }

    fn set_rotation_target(&mut self, angle: f32) {
        // Absolute angles take the shortest path across the 0°/360° boundary.
        let delta = (angle - self.angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        self.rotation_target = Some(self.angle + delta);
    }

    fn animate_rotation(&mut self, dt: f32) {
        let Some(target) = self.rotation_target else {
            return;
        };
        let old_angle = self.angle;
        let remaining = target - self.angle;
        if self.settings.appearance.reduced_motion || remaining.abs() < 0.0005 {
            self.angle = target.rem_euclid(std::f32::consts::TAU);
            self.rotation_target = None;
        } else {
            let blend = 1.0 - (-dt.clamp(0.0, 0.05) / 0.055).exp();
            self.angle += remaining * blend;
        }
        // Pan is the canvas center relative to the viewport center. Rotate that
        // offset too, keeping the document point under the viewport center fixed.
        self.pan = egui::emath::Rot2::from_angle(self.angle - old_angle) * self.pan;
    }

    fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        let ctx = &context;
        let bounds = ui.max_rect();
        let response = ui.allocate_rect(bounds, Sense::click_and_drag());
        if self.fit {
            self.zoom_animation = None;
            let (sin, cos) = self.rotation_target.unwrap_or(self.angle).sin_cos();
            let width = self.doc.width as f32 * cos.abs() + self.doc.height as f32 * sin.abs();
            let height = self.doc.width as f32 * sin.abs() + self.doc.height as f32 * cos.abs();
            let target = ((bounds.width() - 72.0) / width)
                .min((bounds.height() - 72.0) / height)
                .clamp(0.05, 16.0);
            if self.smooth_fit && !self.settings.appearance.reduced_motion {
                self.fit_animation = Some(target);
            } else {
                self.zoom = target;
                self.pan = Vec2::ZERO;
                self.fit_animation = None;
            }
            self.smooth_fit = false;
            self.fit = false;
        }
        self.pressure = self.tablet.pressure().unwrap_or(if self.gesture.is_some() {
            self.pressure
        } else {
            1.0
        });
        if ctx.input(|i| i.focused) {
            ctx.request_repaint_after(std::time::Duration::from_millis(8));
        }
        let (pointer, down, pressed, space, alt, middle, delta, scroll) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.primary_down(),
                i.pointer.primary_pressed(),
                !self.preferences.capturing_shortcut()
                    && self.settings.shortcuts.held(i, Shortcut::Pan),
                !self.preferences.capturing_shortcut()
                    && self.settings.shortcuts.held(i, Shortcut::TemporaryPicker),
                i.pointer.middle_down(),
                i.pointer.delta(),
                if i.raw_scroll_delta.y != 0.0 {
                    i.raw_scroll_delta.y
                } else {
                    i.raw_scroll_delta.x
                },
            )
        });
        // Tablet dials may emit Ctrl +/- rather than wheel events. These
        // shortcuts belong to the document, never egui's global UI scale.
        let (keyboard_factor, reset_zoom) = if !self.preferences.capturing_shortcut()
            && !ctx.wants_keyboard_input()
            && self.pending.is_none()
            && !self.new_dialog
            && self.image_editor.is_none()
            && self.gesture.is_none()
        {
            ctx.input_mut(|i| {
                let steps = self.settings.shortcuts.count(i, Shortcut::ZoomIn) as i32
                    - self.settings.shortcuts.count(i, Shortcut::ZoomOut) as i32;
                let reset = self.settings.shortcuts.consume(i, Shortcut::ResetZoom);
                (1.25_f32.powi(steps.clamp(-32, 32)), reset)
            })
        } else {
            (1.0, false)
        };
        let wheel_factor = if response.hovered() && self.gesture.is_none() {
            (scroll * 0.002).exp()
        } else {
            1.0
        };
        let factor = keyboard_factor * wheel_factor;
        if down || middle || self.gesture.is_some() {
            // Keep the view still once a stroke or a drag starts.
            self.zoom_animation = None;
        }
        if factor != 1.0 || reset_zoom {
            let anchor = pointer
                .filter(|p| bounds.contains(*p))
                .unwrap_or(bounds.center());
            self.request_zoom(factor, reset_zoom, anchor - bounds.center());
        }
        if (down || middle) && response.hovered() || self.gesture.is_some() {
            self.fit_animation = None;
            self.rotation_target = None;
        }
        let dt = ctx.input(|i| i.stable_dt);
        self.animate_fit(dt);
        self.animate_zoom(dt);
        self.animate_rotation(dt);
        if self.zoom_animation.is_some()
            || self.rotation_target.is_some()
            || self.fit_animation.is_some()
        {
            ctx.request_repaint();
        }
        let panning = (space || self.tool == Tool::Hand) && down || middle;
        if panning && response.hovered() && self.gesture.is_none() {
            self.fit_animation = None;
            self.pan += delta;
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        let view = CanvasView {
            center: bounds.center() + self.pan,
            size: Vec2::new(self.doc.width as f32, self.doc.height as f32),
            zoom: self.zoom,
            angle: self.angle,
            mirrored: self.mirrored,
        };
        let to_doc = |p: Pos2| view.document(p);
        let selection_consumed =
            if self.pending.is_none() && !self.new_dialog && self.image_editor.is_none() {
                self.selection_input(ui, &response, view, panning)
            } else {
                true
            };
        if !selection_consumed
            && (!panning || self.gesture.is_some())
            && self.pending.is_none()
            && !self.new_dialog
            && self.image_editor.is_none()
        {
            if pressed
                && response.hovered()
                && let Some(p) = pointer
            {
                let p = ctx
                    .input(|i| {
                        i.events.iter().find_map(|e| match e {
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                ..
                            } => Some(*pos),
                            _ => None,
                        })
                    })
                    .unwrap_or(p);
                let point = to_doc(p);
                if self.doc.contains(point) {
                    if alt || self.tool == Tool::Picker {
                        self.picker.begin(&self.doc);
                    } else if self.doc.layers[self.doc.active].group.is_some() {
                        self.status = "Selecione uma camada dentro do grupo para desenhar.".into();
                    } else if !self.doc.layers[self.doc.active].visible {
                        self.status = "Ative a visibilidade da camada para desenhar.".into();
                    } else if self.tool == Tool::Eraser
                        && self.doc.layers[self.doc.active].alpha_lock
                    {
                        self.status =
                            "Borracha bloqueada: desative o bloqueio de transparência.".into();
                    } else {
                        if matches!(self.tool, Tool::Brush | Tool::Eraser) {
                            self.doc.damage = None;
                            self.history.push_stroke(&mut self.doc);
                        } else {
                            self.checkpoint();
                        }
                        let brush = self.brush();
                        if self.tool == Tool::Fill {
                            self.animated_fill(ctx, point, brush.color, brush.opacity);
                        } else {
                            let base = if self.tool.is_shape() {
                                Some(self.doc.layers[self.doc.active].pixels.clone())
                            } else {
                                None
                            };
                            if !self.tool.is_shape() {
                                self.doc
                                    .stamp(point, brush.clone(), self.tool == Tool::Eraser);
                            }
                            self.gesture = Some(Gesture {
                                start: point,
                                last: point,
                                base,
                                tool: self.tool,
                                filter: Stabilizer::new(point),
                                sample_time: ctx
                                    .input(|i| i.time - i.unstable_dt.clamp(0.0, 0.05) as f64),
                                raw_last: point,
                                last_size: brush.size,
                                distance_to_dab: (brush.size * brush.spacing).max(0.5),
                            });
                        }
                        if self.tool != Tool::Fill {
                            self.changed();
                        }
                    }
                }
            }
            if let Some(mut gesture) = self.gesture.take() {
                let mut samples = ctx.input(|i| {
                    let mut started = !pressed;
                    let mut points = Vec::new();
                    for event in &i.events {
                        match event {
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                ..
                            } => {
                                if !pressed && started && points.last() != Some(pos) {
                                    points.push(*pos);
                                }
                                started = *pressed;
                            }
                            egui::Event::PointerMoved(p) if started && points.last() != Some(p) => {
                                points.push(*p);
                            }
                            _ => {}
                        }
                    }
                    points
                });
                if down
                    && let Some(p) = pointer
                    && samples.last() != Some(&p)
                {
                    samples.push(p);
                }
                // An idle frame advances time once, not once per duplicate pointer event.
                let now = ctx.input(|i| i.time);
                let dt = ((now - gesture.sample_time).max(0.0) as f32).min(0.25)
                    / samples.len().max(1) as f32;
                gesture.sample_time = now;
                for p in samples {
                    let mut point = to_doc(p);
                    // Bound geometry so a pointer far outside the canvas cannot create huge loops.
                    let margin = self.size.max(256.0);
                    point[0] = point[0].clamp(-margin, self.doc.width as f32 + margin);
                    point[1] = point[1].clamp(-margin, self.doc.height as f32 + margin);
                    gesture.raw_last = point;
                    if matches!(gesture.tool, Tool::Brush | Tool::Eraser) {
                        point = gesture.filter.feed(point, self.stabilization, dt);
                    }
                    if point != gesture.last {
                        let brush = self.brush();
                        let end_size = brush.size;
                        if let Some(base) = &gesture.base {
                            self.doc.layers[self.doc.active].pixels.clone_from(base);
                            self.doc.shape(gesture.tool, gesture.start, point, brush);
                            self.doc.damage = None;
                        } else {
                            self.doc.stroke_segment(
                                gesture.last,
                                point,
                                gesture.last_size,
                                brush,
                                gesture.tool == Tool::Eraser,
                                &mut gesture.distance_to_dab,
                            );
                        }
                        gesture.last_size = end_size;
                        gesture.last = point;
                        self.changed();
                    }
                }
                if down {
                    self.gesture = Some(gesture);
                } else if matches!(gesture.tool, Tool::Brush | Tool::Eraser)
                    && gesture.last != gesture.raw_last
                {
                    for point in gesture.filter.finish(
                        gesture.raw_last,
                        self.stabilization,
                        (self.brush().size * self.spacing * 0.5).max(0.5),
                    ) {
                        self.doc.stroke_segment(
                            gesture.last,
                            point,
                            gesture.last_size,
                            self.brush(),
                            gesture.tool == Tool::Eraser,
                            &mut gesture.distance_to_dab,
                        );
                        gesture.last = point;
                        gesture.last_size = self.brush().size;
                    }
                    self.changed();
                }
            }
        }
        if self.screenshot.is_some()
            && std::env::var("PAINTATLAS_SMOKE_SCENE")
                .is_ok_and(|scene| scene.starts_with("fill-reveal"))
            && self.frames == 4
        {
            self.tool = Tool::Fill;
            self.color = Color32::from_rgb(55, 153, 245);
            self.doc.layers[self.doc.active].alpha_lock = false;
            if std::env::var("PAINTATLAS_SMOKE_SCENE").as_deref() == Ok("fill-reveal-solid") {
                self.doc.layers[self.doc.active].pixels.fill(0);
            }
            self.checkpoint();
            self.animated_fill(ctx, [300.0, 250.0], [55, 153, 245, 255], 1.0);
        }
        if self.screenshot.is_some()
            && std::env::var("PAINTATLAS_SMOKE_SCENE")
                .is_ok_and(|scene| scene.starts_with("clipboard-flight"))
            && self.frames == 4
        {
            let rect = egui::Rect::from_min_max(Pos2::new(110.0, 90.0), Pos2::new(490.0, 380.0));
            self.selection.mask =
                crate::selection::Mask::rectangle(self.doc.width, self.doc.height, rect)
                    .map(std::sync::Arc::new);
            self.doc.selection = self.selection.mask.clone();
            if let Ok(image) =
                crate::clipboard::capture(&self.doc, self.selection.mask.as_deref(), false)
            {
                // Native rendering QA never reads or overwrites the system clipboard.
                self.clipboard_motion
                    .copied(ctx, &image, self.clipboard_quad(rect));
                if std::env::var("PAINTATLAS_SMOKE_SCENE").as_deref()
                    == Ok("clipboard-flight-paste")
                {
                    let preview = clipboard_motion::thumbnail(ctx, &image);
                    if self.paste_image(ctx, image).is_ok() {
                        let target = self
                            .selection
                            .floating
                            .as_ref()
                            .and_then(|f| self.clipboard_quad(f.bounds));
                        self.clipboard_motion.pasted(ctx, preview, target);
                    }
                }
            }
        }
        self.fill_reveal.advance(ctx);
        self.sync_layer_textures(ctx);
        let painter = ui.painter().with_clip_rect(bounds);
        self.paint_canvas_document(ui, bounds, view);
        if self.screenshot.is_some()
            && std::env::var("PAINTATLAS_SMOKE_SCENE").is_ok_and(|s| s.starts_with("stabilizer"))
        {
            for (y, label) in [
                (90.0, "0% — sem estabilização"),
                (330.0, "100% — estabilização forte"),
            ] {
                painter.text(
                    view.screen([100.0, y]),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::proportional(17.0),
                    Color32::from_rgb(50, 60, 70),
                );
            }
        }
        if response.hovered()
            && let Some(p) = pointer
        {
            if self.selection.floating.is_none()
                && self.doc.contains(to_doc(p))
                && !panning
                && matches!(
                    self.tool,
                    Tool::Brush | Tool::Eraser | Tool::Line | Tool::Rectangle | Tool::Ellipse
                )
                && !alt
            {
                ctx.set_cursor_icon(egui::CursorIcon::None);
                let mut cursor = p;
                if self.stabilization > 0.0
                    && let Some(gesture) = &self.gesture
                    && matches!(gesture.tool, Tool::Brush | Tool::Eraser)
                {
                    cursor = view.screen(gesture.last);
                    if cursor.distance(p) > 1.0 {
                        painter.line_segment(
                            [cursor, p],
                            Stroke::new(2.0_f32, Color32::from_black_alpha(100)),
                        );
                        painter.line_segment(
                            [cursor, p],
                            Stroke::new(1.0_f32, Color32::from_white_alpha(110)),
                        );
                        painter.circle_filled(p, 2.0, Color32::from_black_alpha(180));
                        painter.circle_stroke(p, 2.0, Stroke::new(1.0_f32, Color32::WHITE));
                    }
                }
                self.paint_brush_cursor(&painter, view, cursor, &self.brush());
            } else if !panning {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        self.selection_overlay(ui, view);
        let (picker_pointer, picker_down) = if self.screenshot.is_some()
            && std::env::var("PAINTATLAS_SMOKE_SCENE").as_deref() == Ok("eyedropper")
        {
            if !self.picker.active {
                self.picker.begin(&self.doc);
            }
            (Some(view.screen([295.0, 225.0])), true)
        } else {
            (pointer, down)
        };
        if let Some(color) = self.picker.update(ctx, picker_pointer, picker_down, view) {
            self.color = color;
        }
        self.picker.paint(ctx);
    }
    fn paint_brush_cursor(
        &self,
        painter: &egui::Painter,
        view: CanvasView,
        p: Pos2,
        brush: &Brush,
    ) {
        if self.settings.cursor_style == crate::preferences::CursorStyle::Dot {
            painter.circle_filled(p, 3.0, Color32::BLACK);
            painter.circle_filled(p, 1.5, Color32::WHITE);
            return;
        }
        let cursor_size = if self.settings.cursor_style == crate::preferences::CursorStyle::Circle
            && !self.settings.circle_cursor_pressure
        {
            self.size
        } else {
            brush.size
        };
        if let Some(tip) = &brush.tip
            && self.settings.cursor_style == crate::preferences::CursorStyle::Brush
        {
            for contour in &tip.contours {
                let doc_point = view.document(p);
                let points: Vec<_> = contour
                    .iter()
                    .map(|q| {
                        view.screen([
                            doc_point[0] + q[0] * brush.size,
                            doc_point[1] + q[1] * brush.size,
                        ])
                    })
                    .collect();
                painter.add(egui::Shape::line(
                    points.clone(),
                    Stroke::new(2.5_f32, Color32::from_black_alpha(190)),
                ));
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(1.0_f32, Color32::WHITE),
                ));
            }
        } else {
            let r = cursor_size * view.zoom * 0.5;
            painter.circle_stroke(p, r, Stroke::new(2.5_f32, Color32::from_black_alpha(180)));
            painter.circle_stroke(p, r, Stroke::new(1.0_f32, Color32::WHITE));
        }
        if cursor_size * view.zoom < 5.0 {
            painter.line_segment(
                [p - Vec2::new(3.0, 0.0), p + Vec2::new(3.0, 0.0)],
                Stroke::new(1.0_f32, Color32::GRAY),
            );
            painter.line_segment(
                [p - Vec2::new(0.0, 3.0), p + Vec2::new(0.0, 3.0)],
                Stroke::new(1.0_f32, Color32::GRAY),
            );
        }
    }
    fn tools_ui(&mut self, ui: &mut egui::Ui) {
        // Use the current viewport width, never a previous scroll content width.
        let width = ui.available_width().min(ui.clip_rect().width()).max(1.0);
        let gap = ui.spacing().item_spacing.x;
        let compact = width < 94.0 * 2.0 + gap;
        let columns = if compact {
            (((width + gap) / (32.0 + gap)).floor() as usize).clamp(1, 10)
        } else {
            2
        };
        let button_width = if compact {
            ((width - gap * (columns - 1) as f32) / columns as f32).min(40.0)
        } else {
            ((width - gap) / 2.0).min(110.0)
        };
        let tools = [
            ("Pincel", Tool::Brush, Shortcut::Brush),
            ("Borracha", Tool::Eraser, Shortcut::Eraser),
            ("Preencher", Tool::Fill, Shortcut::Fill),
            ("Conta-gotas", Tool::Picker, Shortcut::Picker),
            ("Linha", Tool::Line, Shortcut::Line),
            ("Retângulo", Tool::Rectangle, Shortcut::Rectangle),
            ("Elipse", Tool::Ellipse, Shortcut::Ellipse),
            ("Mover tela", Tool::Hand, Shortcut::Hand),
            ("Seleção", Tool::SelectRect, Shortcut::SelectRect),
            ("Laço", Tool::Lasso, Shortcut::Lasso),
        ];
        let mut targets = Vec::with_capacity(tools.len());
        for row in tools.chunks(columns) {
            ui.horizontal(|ui| {
                for &(label, tool, shortcut) in row {
                    ui.push_id(label, |ui| {
                        let response = crate::theme::tool_button(
                            ui,
                            label,
                            tool,
                            self.tool == tool,
                            &self.settings.shortcuts.label(shortcut),
                            Vec2::new(button_width, 29.0),
                            compact,
                        );
                        targets.push((tool, response.rect));
                        if response.clicked() {
                            self.select_tool(tool);
                        }
                    });
                }
            });
        }
        crate::selection_motion::paint(
            ui,
            ui.id().with(("tools_selection", columns, compact)),
            targets
                .iter()
                .find(|(tool, _)| *tool == self.tool)
                .map(|(_, rect)| *rect),
        );
    }

    fn properties_ui(&mut self, ui: &mut egui::Ui) {
        section(ui, "PROPRIEDADES");
        crate::theme::caption(ui, &self.tablet.name);
        crate::theme::caption(ui, format!("Pressão: {:.0}%", self.tablet.current * 100.0));
        ui.label(self.tool.name());
        if matches!(self.tool, Tool::SelectRect | Tool::Lasso) {
            self.selection_properties(ui);
            return;
        }
        if self.preset_modified() {
            ui.separator();
            crate::theme::caption(ui, "Pincel modificado");
            if crate::theme::button(ui, "Salvar como novo preset…").clicked() {
                self.open_brush_editor();
            }
        }

        ui.add(
            egui::Slider::new(&mut self.size, 1.0..=MAX_BRUSH_SIZE)
                .text("px")
                .logarithmic(true),
        );
        if ui
            .button("Tamanhos de pincel…")
            .on_hover_text(
                self.settings
                    .shortcuts
                    .menu(Shortcut::BrushSizes, "Abrir a janela com 1.000 tamanhos"),
            )
            .clicked()
        {
            self.workspace_actions.push(crate::workspace::Action::Open(
                crate::workspace::Panel::BrushSizes,
            ));
        }
        ui.label("Opacidade");
        ui.add(egui::Slider::new(&mut self.opacity, 0.01..=1.0).show_value(true));
        ui.label("Dureza");
        ui.add(egui::Slider::new(&mut self.hardness, 0.0..=1.0));
        let mut tip = self.tip_id.clone();
        self.tip_selector_ui(ui, &mut tip);
        if tip != self.tip_id {
            self.tip_id = tip;
            self.active_tip = self
                .tip_id
                .as_deref()
                .and_then(|id| self.tip_library.get(id).ok());
        }
        ui.label("Espaçamento das carimbadas");
        ui.add(egui::Slider::new(&mut self.spacing, 0.05..=2.0).logarithmic(true));
        ui.label("Estabilização");
        ui.add(egui::Slider::new(&mut self.stabilization, 0.0..=100.0).integer().suffix(" %"))
            .on_hover_text("0%: traço direto. 100%: suavização forte, com cerca de 240 ms de atraso. A linha guia liga a caneta à ponta estabilizada.");
        ui.collapsing("Dinâmica da caneta", |ui| {
            crate::theme::toggle(ui, &mut self.pressure_enabled, "Pressão no tamanho");
            ui.add_enabled_ui(self.pressure_enabled, |ui| {
                ui.label("Curva: leve ← → firme");
                ui.add(egui::Slider::new(&mut self.pressure_gamma, 0.3..=3.0));
                ui.label("Tamanho mínimo");
                ui.add(egui::Slider::new(&mut self.pressure_minimum, 0.01..=1.0));
                let (rect, _) = ui.allocate_exact_size(Vec2::new(160.0, 64.0), Sense::hover());
                ui.painter().rect_filled(rect, 3.0, Color32::from_gray(18));
                let points = (0..=40)
                    .map(|i| {
                        let x = i as f32 / 40.0;
                        Pos2::new(
                            rect.min.x + x * rect.width(),
                            rect.max.y
                                - pressure_size(x, self.pressure_gamma, self.pressure_minimum)
                                    * rect.height(),
                        )
                    })
                    .collect();
                ui.painter().add(egui::Shape::line(
                    points,
                    Stroke::new(1.5_f32, crate::theme::accent(ui.ctx())),
                ));
            });
        });
        if self.tool == Tool::Fill {
            ui.label("Tolerância");
            ui.add(egui::Slider::new(&mut self.tolerance, 0..=100));
            crate::theme::caption(ui, "Preenche a camada ativa.");
        }
        ui.add_space(10.0);
    }
    fn brushes_ui(&mut self, ui: &mut egui::Ui) {
        section(ui, "PINCÉIS");
        self.brush_previews
            .resize_with(self.settings.brushes.len(), || None);
        let mut selected = self.settings.selected;
        for i in 0..self.settings.brushes.len() {
            let preset = if i == self.settings.selected {
                self.current_preset()
            } else {
                self.settings.brushes[i].clone()
            };
            let tip = preset
                .texture
                .as_deref()
                .and_then(|id| self.tip_library.get(id).ok());
            let cache = &mut self.brush_previews[i];
            if cache.as_ref().is_none_or(|(p, _)| p != &preset) {
                *cache = Some((preset.clone(), Self::preset_texture(ui.ctx(), &preset, tip)));
            }
            let width = ui.available_width().max(64.0);
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 76.0), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    selected == i,
                    &preset.name,
                )
            });
            let visuals = crate::theme::row_visuals(ui, &response, selected == i);
            ui.painter().rect(
                rect,
                10.0,
                visuals.bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
            let text_rect = egui::Rect::from_min_size(
                rect.min + Vec2::new(8.0, 5.0),
                Vec2::new((width - 16.0).max(1.0), 20.0),
            );
            ui.painter().with_clip_rect(text_rect).text(
                text_rect.min,
                egui::Align2::LEFT_TOP,
                &preset.name,
                egui::FontId::proportional(13.0),
                visuals.text_color(),
            );
            let image_rect = egui::Rect::from_min_max(
                rect.min + Vec2::new(6.0, 27.0),
                rect.max - Vec2::new(6.0, 6.0),
            );
            if let Some((_, texture)) = cache {
                ui.painter().image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            if response
                .on_hover_text(format!(
                    "{} · {:.0} px{}",
                    preset.name,
                    preset.size,
                    if preset.eraser { " · borracha" } else { "" }
                ))
                .clicked()
            {
                selected = i;
            }
        }
        if selected != self.settings.selected {
            self.select_preset(selected);
        }
        ui.add_space(6.0);
        if ui
            .add_enabled(
                self.settings.brushes.len() < 256,
                egui::Button::new("+ Adicionar pincel…"),
            )
            .clicked()
        {
            self.open_brush_editor();
        }
        if ui.small_button("Restaurar este pincel").clicked() {
            let i = self.settings.selected;
            self.settings.brushes[i] = self.settings.originals[i].clone();
            self.apply_preset(i);
            self.settings_pending = true;
            self.settings_edited = std::time::Instant::now();
        }
        crate::theme::caption(ui, "Ajustes salvos automaticamente");
    }
    fn tip_selector_ui(&mut self, ui: &mut egui::Ui, selected: &mut Option<String>) {
        ui.label("Ponta / textura");
        let name = selected
            .as_deref()
            .map_or("Circular (sem textura)", |id| self.tip_library.name(id))
            .to_owned();
        ui.menu_button(name, |ui| {
            ui.set_width(330.0);
            if ui
                .selectable_label(selected.is_none(), "Circular (sem textura)")
                .clicked()
            {
                *selected = None;
                ui.close();
            }
            crate::theme::caption(ui, "Escolha uma ponta do pack");
            egui::ScrollArea::vertical().max_height(330.0).show_rows(
                ui,
                91.0,
                self.tip_library.entries.len().div_ceil(3),
                |ui, rows| {
                    for row in rows {
                        ui.horizontal(|ui| {
                            for col in 0..3 {
                                let i = row * 3 + col;
                                let Some(entry) = self.tip_library.entries.get(i) else {
                                    break;
                                };
                                let id = entry.id.clone();
                                let name = entry.name.clone();
                                let (rect, response) =
                                    ui.allocate_exact_size(Vec2::new(100.0, 85.0), Sense::click());
                                let visuals = ui.style().interact_selectable(
                                    &response,
                                    selected.as_deref() == Some(&id),
                                );
                                ui.painter().rect(
                                    rect,
                                    3.0,
                                    visuals.bg_fill,
                                    visuals.bg_stroke,
                                    egui::StrokeKind::Inside,
                                );
                                match self.tip_library.thumbnail(ui.ctx(), &id) {
                                    Ok(texture) => {
                                        let tip = self.tip_library.get(&id).unwrap();
                                        let ratio = tip.proportions();
                                        let area = Vec2::new(ratio[0] * 54.0, ratio[1] * 54.0);
                                        let image = egui::Rect::from_center_size(
                                            rect.min + Vec2::new(50.0, 31.0),
                                            area,
                                        );
                                        ui.painter().image(
                                            texture,
                                            image,
                                            egui::Rect::from_min_max(
                                                Pos2::ZERO,
                                                Pos2::new(1.0, 1.0),
                                            ),
                                            Color32::WHITE,
                                        );
                                    }
                                    Err(_) => {
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "!",
                                            egui::FontId::proportional(16.0),
                                            Color32::LIGHT_RED,
                                        );
                                    }
                                }
                                let label = egui::Rect::from_min_max(
                                    rect.min + Vec2::new(3.0, 63.0),
                                    rect.max - Vec2::splat(3.0),
                                );
                                ui.painter().with_clip_rect(label).text(
                                    label.min,
                                    egui::Align2::LEFT_TOP,
                                    &name,
                                    egui::FontId::proportional(11.0),
                                    visuals.text_color(),
                                );
                                if response.on_hover_text(&name).clicked() {
                                    match self.tip_library.get(&id) {
                                        Ok(_) => {
                                            *selected = Some(id);
                                            ui.close();
                                        }
                                        Err(e) => self.status = e,
                                    }
                                }
                            }
                        });
                    }
                },
            );
        });
        if ui.small_button("Importar imagem de ponta…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Imagem de ponta", &["png", "jpg", "jpeg", "webp"])
                .pick_file()
        {
            match self.tip_library.import(&path) {
                Ok(id) => {
                    *selected = Some(id);
                    self.status = "Ponta importada e copiada para sua biblioteca.".into();
                }
                Err(e) => self.status = format!("Erro ao importar ponta: {e}"),
            }
        }
        if selected
            .as_deref()
            .is_some_and(|id| self.tip_library.get(id).is_err())
        {
            ui.colored_label(Color32::LIGHT_RED, "Ponta não encontrada; usando círculo.");
        }
    }
    fn brush_editor_ui(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.brush_editor.take() else {
            return;
        };
        let mut open = true;
        let mut create = false;
        let mut cancel = false;
        egui::Window::new("Criar pincel")
            .id(egui::Id::new("create_brush"))
            .open(&mut open)
            .resizable(true)
            .default_size([460.0, 880.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(840.0)
                    .show(ui, |ui| {
                        ui.label("Nome do preset");
                        ui.add(egui::TextEdit::singleline(&mut draft.name).char_limit(100));
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Tipo");
                            ui.selectable_value(&mut draft.eraser, false, "Pincel");
                            ui.selectable_value(&mut draft.eraser, true, "Borracha");
                        });
                        ui.label("Prévia do traço");
                        self.brush_preview(ui, &draft);
                        ui.separator();
                        ui.label("Tamanho");
                        ui.add(
                            egui::Slider::new(&mut draft.size, 1.0..=MAX_BRUSH_SIZE)
                                .logarithmic(true)
                                .suffix(" px"),
                        );
                        ui.label("Opacidade");
                        ui.add(egui::Slider::new(&mut draft.opacity, 0.01..=1.0));
                        ui.label("Dureza");
                        ui.add(egui::Slider::new(&mut draft.hardness, 0.0..=1.0));
                        self.tip_selector_ui(ui, &mut draft.texture);
                        ui.label("Espaçamento das carimbadas");
                        ui.add(egui::Slider::new(&mut draft.spacing, 0.05..=2.0).logarithmic(true));
                        ui.label("Estabilização");
                        ui.add(
                            egui::Slider::new(&mut draft.stabilization, 0.0..=100.0)
                                .integer()
                                .suffix(" %"),
                        );
                        crate::theme::toggle(
                            ui,
                            &mut draft.pressure,
                            "Pressão da caneta controla o tamanho",
                        );
                        ui.add_enabled_ui(draft.pressure, |ui| {
                            ui.label("Curva: leve ← → firme");
                            ui.add(egui::Slider::new(&mut draft.gamma, 0.3..=3.0));
                            ui.label("Tamanho mínimo com pressão");
                            ui.add(egui::Slider::new(&mut draft.minimum, 0.01..=1.0));
                        });
                        if !self.brush_editor_error.is_empty() {
                            ui.colored_label(Color32::LIGHT_RED, &self.brush_editor_error);
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            create = crate::theme::button(ui, "Criar preset").clicked();
                            cancel = crate::theme::button(ui, "Cancelar").clicked();
                        });
                    });
            });
        if create {
            match self.create_brush(draft.clone()) {
                Ok(()) => return,
                Err(e) => self.brush_editor_error = e,
            }
        }
        if open && !cancel {
            self.brush_editor = Some(draft);
        }
    }
    fn colors_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        section(ui, "COR");
        let old_mode = self.settings.triangular_color_selector;
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.settings.triangular_color_selector,
                false,
                "Retangular",
            );
            ui.selectable_value(
                &mut self.settings.triangular_color_selector,
                true,
                "Triangular",
            );
        });
        if old_mode != self.settings.triangular_color_selector {
            self.settings_pending = true;
            self.settings_edited = std::time::Instant::now();
        }
        if self.settings.triangular_color_selector {
            self.color_triangle.show(ui, &mut self.color);
            ui.horizontal_wrapped(|ui| {
                let [mut r, mut g, mut b] = [self.color.r(), self.color.g(), self.color.b()];
                let changed = ui
                    .add(egui::DragValue::new(&mut r).range(0..=255).prefix("R "))
                    .changed()
                    | ui.add(egui::DragValue::new(&mut g).range(0..=255).prefix("G "))
                        .changed()
                    | ui.add(egui::DragValue::new(&mut b).range(0..=255).prefix("B "))
                        .changed();
                if changed {
                    self.color = Color32::from_rgb(r, g, b);
                }
            });
            let (swatch, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
            ui.painter().rect_filled(swatch, 2.0, self.color);
        } else {
            ui.scope(|ui| {
                ui.spacing_mut().slider_width =
                    ui.available_width().min(ui.clip_rect().width()).max(1.0);
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.spacing_mut().button_padding.x = 4.0;
                ui.spacing_mut().interact_size.x = 24.0;
                egui::color_picker::color_picker_color32(
                    ui,
                    &mut self.color,
                    egui::color_picker::Alpha::Opaque,
                );
            });
        }
        ui.label(
            egui::RichText::new(format!(
                "#{:02X}{:02X}{:02X}",
                self.color.r(),
                self.color.g(),
                self.color.b()
            ))
            .monospace(),
        );
        ui.horizontal_wrapped(|ui| {
            for c in [
                Color32::from_rgb(42, 52, 68),
                Color32::WHITE,
                Color32::from_rgb(218, 93, 107),
                Color32::from_rgb(239, 166, 101),
                Color32::from_rgb(242, 213, 128),
                ACCENT,
                Color32::from_rgb(106, 165, 211),
                Color32::from_rgb(158, 136, 202),
            ] {
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(21.0), Sense::click());
                ui.painter().rect_filled(rect, 5.0, c);
                if response.clicked() {
                    self.color = c;
                }
            }
        });
    }
    fn layers_ui(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        let ctx = &context;
        if self.settings.appearance.reduced_motion {
            self.layer_row_slides.clear();
        }
        let decay = (-18.0 * ctx.input(|i| i.stable_dt).clamp(0.0, 0.05)).exp();
        for (_, offset) in &mut self.layer_row_slides {
            *offset *= decay;
        }
        self.layer_row_slides
            .retain(|(_, offset)| offset.abs() > 0.2);
        if !self.layer_row_slides.is_empty() {
            ctx.request_repaint();
        }
        section(ui, "CAMADAS");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.doc.can_add(), egui::Button::new("+"))
                .on_hover_text("Nova camada")
                .clicked()
            {
                self.checkpoint();
                self.doc.add_layer();
                self.selected_layers.clear();
                self.rename = self.doc.layers[self.doc.active].name.clone();
                self.changed();
            }
            if ui
                .add_enabled(self.doc.can_add(), egui::Button::new("Duplicar"))
                .clicked()
            {
                self.apply_layer_action(self.doc.active, layer_menu::LayerAction::Duplicate);
            }
            if ui
                .add_enabled(self.doc.layers.len() > 1, egui::Button::new("Excluir"))
                .clicked()
            {
                self.apply_layer_action(self.doc.active, layer_menu::LayerAction::Delete);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Agrupar")
                .on_hover_text(self.settings.shortcuts.label(Shortcut::LayerGroup))
                .clicked()
            {
                self.apply_layer_action(self.doc.active, layer_menu::LayerAction::Group);
            }
            if ui
                .add_enabled(
                    self.selected_layer_indices().len() > 1,
                    egui::Button::new("Mesclar"),
                )
                .on_hover_text(
                    self.settings
                        .shortcuts
                        .menu(Shortcut::LayerMerge, "Mesclar selecionadas"),
                )
                .clicked()
            {
                self.apply_layer_action(self.doc.active, layer_menu::LayerAction::MergeSelected);
            }
        });
        crate::theme::caption(
            ui,
            format!(
                "{}+clique: múltiplas · {}+clique: intervalo",
                self.settings
                    .shortcuts
                    .label(Shortcut::LayerToggleSelection),
                self.settings.shortcuts.label(Shortcut::LayerRangeSelection)
            ),
        );
        ui.label("Modo de mesclagem");
        let mut mode = self.doc.layers[self.doc.active].blend_mode;
        egui::ComboBox::from_id_salt("layer_blend_mode")
            .selected_text(mode.label())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for option in crate::blend::BlendMode::ALL {
                    ui.selectable_value(&mut mode, option, option.label());
                }
            });
        if mode != self.doc.layers[self.doc.active].blend_mode {
            self.checkpoint();
            self.doc.layers[self.doc.active].blend_mode = mode;
            if self.doc.layers[self.doc.active].parent.is_some() {
                self.changed();
            }
            if self.blend_shader.is_none() {
                self.refresh = true;
            }
            self.dirty = true;
            ctx.request_repaint();
        }
        ui.label("Opacidade da camada");
        let mut opacity = self.doc.layers[self.doc.active].opacity;
        let response = ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0));
        if response.changed() {
            self.set_layer_opacity(opacity);
            ctx.request_repaint();
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.opacity_edit = false;
        }
        ui.separator();
        self.sync_layer_textures(ctx);
        let mut drop = None;
        let mut rows = Vec::new();
        let mut layer_action = None;
        egui::ScrollArea::vertical()
            .id_salt("layer_list")
            .max_height(240.0)
            .show(ui, |ui| {
                for index in (0..self.doc.layers.len()).rev() {
                    if self
                        .collapsed_groups
                        .iter()
                        .any(|&id| self.doc.belongs_to(index, id))
                    {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(self.doc.depth(index) as f32 * 14.0);
                        if let Some(id) = self.doc.layers[index].group {
                            let arrow = if self.collapsed_groups.contains(&id) {
                                "▶"
                            } else {
                                "▼"
                            };
                            let toggle = ui.add(
                                egui::Button::new(arrow)
                                    .small()
                                    .min_size(Vec2::new(28.0, 18.0)),
                            );
                            if toggle.clicked() && !self.collapsed_groups.remove(&id) {
                                self.collapsed_groups.insert(id);
                            }
                        } else {
                            // Reserve the same disclosure column so children indent
                            // to the right of their parent rather than to its left.
                            ui.allocate_space(Vec2::new(28.0, 18.0));
                        }
                        let mut visible = self.doc.layers[index].visible;
                        if ui
                            .checkbox(&mut visible, "")
                            .on_hover_text("Visibilidade")
                            .changed()
                        {
                            self.checkpoint();
                            self.doc.layers[index].visible = visible;
                            self.changed();
                        }
                        let response = self.layer_preview_row(ui, index);
                        if response.secondary_clicked()
                            && self.selection.floating.is_none()
                            && self.gesture.is_none()
                        {
                            if !self.selected_layers.contains(&index) {
                                self.selected_layers.clear();
                            }
                            self.commit_name();
                            self.doc.active = index;
                            self.selected_layers.insert(index);
                            self.rename = self.doc.layers[index].name.clone();
                        }
                        response.context_menu(|ui| {
                            if let Some(action) = self.layer_context_menu(ui, index) {
                                layer_action = Some((index, action));
                            }
                        });
                        rows.push((index, response.rect));
                    });
                }
                if let Some(payload) = egui::DragAndDrop::payload::<LayerDrag>(ctx)
                    && let Some(pointer) = ctx.pointer_interact_pos()
                    && ui.clip_rect().contains(pointer)
                    && let Some((_, first)) = rows.first()
                    && pointer.x >= first.left()
                    && pointer.x <= first.right()
                {
                    let target = rows.iter().find(|(_, rect)| pointer.y < rect.center().y);
                    let (slot, y) = if let Some((index, rect)) = target {
                        (index + 1, rect.top())
                    } else {
                        (0, rows.last().unwrap().1.bottom())
                    };
                    let destination = slot - usize::from(payload.0 < slot);
                    if destination != payload.0 {
                        ui.painter().line_segment(
                            [Pos2::new(first.left(), y), Pos2::new(first.right(), y)],
                            Stroke::new(3.0_f32, crate::theme::accent(ctx)),
                        );
                    }
                    if ctx.input(|i| i.pointer.primary_released()) {
                        drop = Some((payload.0, slot));
                        egui::DragAndDrop::clear_payload(ctx);
                    }
                    if pointer.y < ui.clip_rect().top() + 20.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, 6.0));
                        ctx.request_repaint();
                    } else if pointer.y > ui.clip_rect().bottom() - 20.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, -6.0));
                        ctx.request_repaint();
                    }
                }
            });
        if self.screenshot.is_some() {
            ctx.data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("layer_motion_demo_rows"),
                    rows.iter().map(|(_, rect)| *rect).collect::<Vec<_>>(),
                )
            });
        }
        if let Some((source, slot)) = drop {
            let destination = slot - usize::from(source < slot);
            self.layer_row_slides.clear();
            for (index, rect) in &rows {
                let old_index = if source < destination && *index >= source && *index < destination
                {
                    Some(index + 1)
                } else if source > destination && *index > destination && *index <= source {
                    Some(index - 1)
                } else {
                    None
                };
                if let Some(old_index) = old_index
                    && let Some((_, old)) = rows.iter().find(|(i, _)| *i == old_index)
                {
                    self.layer_row_slides
                        .push((*index, old.center().y - rect.center().y));
                }
            }
            self.reorder_layer(source, slot);
            if let Some(motion) = &mut self.layer_motion {
                motion.release(self.doc.active);
            }
        }
        if let Some(mut motion) = self.layer_motion.take() {
            if !ctx.input(|i| i.pointer.primary_down())
                || egui::DragAndDrop::payload::<LayerDrag>(ctx).is_none()
            {
                motion.release(motion.index);
            }
            if let Some((_, target)) = rows.iter().find(|(index, _)| *index == motion.index)
                && motion.paint(ui, *target)
            {
                self.layer_motion = Some(motion);
            }
        }
        ui.separator();
        if let Some((index, action)) = layer_action {
            self.apply_layer_action(index, action);
            ctx.request_repaint();
        }
        ui.label("Nome da camada");
        let response = ui.add(egui::TextEdit::singleline(&mut self.rename).desired_width(208.0));
        if self.rename_layer_focus {
            response.request_focus();
            self.rename_layer_focus = false;
        }
        if response.lost_focus()
            && !self.rename.trim().is_empty()
            && self.rename != self.doc.layers[self.doc.active].name
        {
            self.checkpoint();
            self.doc.layers[self.doc.active].name = self.rename.chars().take(100).collect();
            self.changed();
        }
        let mut locked = self.doc.layers[self.doc.active].alpha_lock;
        if crate::theme::toggle(ui, &mut locked, "Bloquear transparência")
            .on_hover_text(
                "Pinta somente nas áreas já desenhadas. Borracha e limpeza ficam bloqueadas.",
            )
            .changed()
        {
            self.checkpoint();
            self.doc.layers[self.doc.active].alpha_lock = locked;
            self.changed();
        }
        if ui
            .add_enabled(
                !self.doc.layers[self.doc.active].alpha_lock
                    && self.doc.layers[self.doc.active].group.is_none(),
                egui::Button::new("Limpar camada"),
            )
            .clicked()
        {
            self.checkpoint();
            self.doc.layers[self.doc.active].pixels.fill(0);
            self.changed();
        }
        ui.add_space(12.0);
        ui.add(egui::Label::new(egui::RichText::new("As camadas superiores aparecem na frente. Salve em .patlas para continuar editando.").small()).wrap());
    }
    fn reorder_layer(&mut self, source: usize, slot: usize) {
        if source >= self.doc.layers.len() || slot > self.doc.layers.len() {
            return;
        }
        let start = self.doc.block_start(source);
        if (start..=source + 1).contains(&slot) {
            return;
        }
        // Keep a moved subtree intact and adopt the destination's parent.
        let parent = if slot < self.doc.layers.len() {
            self.doc.layers[slot].group.or(self.doc.layers[slot].parent)
        } else {
            None
        };
        if let Some(id) = self.doc.layers[source].group
            && parent.is_some_and(|p| p == id)
        {
            return;
        }
        self.commit_name();
        self.checkpoint();
        let mut block: Vec<_> = self.doc.layers.drain(start..=source).collect();
        block.last_mut().unwrap().parent = parent;
        let dest = if slot > source {
            slot - block.len()
        } else {
            slot
        };
        self.doc.active = dest + block.len() - 1;
        self.doc.layers.splice(dest..dest, block);
        self.selected_layers.clear();
        self.rename = self.doc.layers[self.doc.active].name.clone();
        self.changed();
    }
    fn animate_fit(&mut self, dt: f32) {
        let Some(target) = self.fit_animation else {
            return;
        };
        let blend = if self.settings.appearance.reduced_motion {
            1.0
        } else {
            1.0 - (-dt.clamp(0.0, 0.05) / 0.09).exp()
        };
        self.zoom *= ((target / self.zoom).ln() * blend).exp();
        self.pan *= 1.0 - blend;
        if (self.zoom / target).ln().abs() < 0.0005 && self.pan.length() < 0.1 {
            self.zoom = target;
            self.pan = Vec2::ZERO;
            self.fit_animation = None;
        }
    }
    fn navigation_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Vista");
            if ui
                .button("−15°")
                .on_hover_text("Girar para a esquerda (Q)")
                .clicked()
            {
                self.request_rotation(-15.0_f32.to_radians());
            }
            if ui
                .button("+15°")
                .on_hover_text("Girar para a direita (W)")
                .clicked()
            {
                self.request_rotation(15.0_f32.to_radians());
            }
            let mut degrees = self.rotation_target.unwrap_or(self.angle).to_degrees();
            if ui
                .add(egui::DragValue::new(&mut degrees).speed(1.0).suffix("°"))
                .changed()
            {
                self.set_rotation_target(degrees.rem_euclid(360.0).to_radians());
            }
            crate::theme::toggle(
                ui,
                &mut self.mirrored,
                &self.settings.shortcuts.menu(Shortcut::Mirror, "Espelhar"),
            );
            if crate::theme::button(ui, "Restaurar vista").clicked() {
                self.set_rotation_target(0.0);
                self.mirrored = false;
                self.pan = Vec2::ZERO;
                self.fit = true;
            }
            ui.separator();
            crate::theme::caption(ui, "A rotação e o espelho alteram só a visualização");
        });
        section(ui, "NAVEGAÇÃO");
        ui.horizontal(|ui| {
            if crate::theme::button(ui, "−").clicked() {
                self.request_zoom(1.0 / 1.25, false, Vec2::ZERO);
            }
            if crate::theme::button(ui, "Ajustar")
                .on_hover_text(self.settings.shortcuts.label(Shortcut::Fit))
                .clicked()
            {
                self.fit = true;
                self.smooth_fit = true;
            }
            if crate::theme::button(ui, "+").clicked() {
                self.request_zoom(1.25, false, Vec2::ZERO);
            }
        });
        if crate::theme::button(ui, "100%").clicked() {
            self.request_zoom(1.0, true, Vec2::ZERO);
            self.pan = Vec2::ZERO;
        }
        ui.add_space(8.0);
        crate::theme::caption(
            ui,
            format!(
                "Roda: zoom\n{} + arrastar: mover\n{} + clique: pegar cor",
                self.settings.shortcuts.label(Shortcut::Pan),
                self.settings.shortcuts.label(Shortcut::TemporaryPicker)
            ),
        );
    }
    fn help_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Um estúdio de desenho em Rust, feito para Linux.");
        ui.separator();
        ui.label("Roda: zoom · botão do meio: mover vista · abas: trocar canvas");
        crate::theme::caption(ui, "Personalize em Configurações > Atalhos.");
        for category in [
            "Arquivo",
            "Imagem",
            "Filtros",
            "Edição",
            "Ferramentas",
            "Pincel",
            "Canvas",
            "Seleção",
            "Transformação",
        ] {
            egui::CollapsingHeader::new(category)
                .default_open(true)
                .show(ui, |ui| {
                    for action in Shortcut::ALL.iter().filter(|a| a.category() == category) {
                        ui.label(format!(
                            "{} — {}",
                            self.settings.shortcuts.label(*action),
                            action.label()
                        ));
                    }
                });
        }
        ui.separator();
        ui.label("O projeto .patlas mantém as camadas. O PNG exporta\na composição visível, incluindo transparência.");
        crate::theme::caption(ui, "Protótipo raster • sem suporte a arquivos .clip.");
    }
    fn apply_workspace_actions(&mut self, ctx: &egui::Context) {
        for action in std::mem::take(&mut self.workspace_actions) {
            if let crate::workspace::Action::Open(panel) = action {
                if panel == crate::workspace::Panel::BrushSizes {
                    self.size_palette.locate_current();
                }
                if self
                    .workspace
                    .layout
                    .external
                    .iter()
                    .any(|p| p.panel == panel)
                {
                    ctx.send_viewport_cmd_to(
                        panel_viewport_id(panel),
                        egui::ViewportCommand::Focus,
                    );
                }
            }
            self.workspace.layout.apply(action);
        }
    }
    fn workspace_ui(&mut self, ctx: &egui::Context) {
        self.apply_workspace_actions(ctx);
        let mut dock = std::mem::replace(
            &mut self.workspace.layout.dock,
            egui_dock::DockState::new(Vec::new()),
        );
        self.floating_panels = dock
            .iter_all_tabs()
            .filter_map(|((surface, _), panel)| {
                (surface != egui_dock::SurfaceIndex::main()).then_some(*panel)
            })
            .collect();
        let dragging = self.gesture.is_none();
        let mut style = egui_dock::Style::from_egui(&ctx.style());
        crate::theme::dock(&mut style, ctx);
        // Resolve drops from the pointer position: edges split the canvas itself,
        // center groups tabs, and the intermediate zone keeps floating possible.
        // This is a separate limit from the minimum of floating windows.
        style.separator.extra = 64.0;
        style.overlay.overlay_type = egui_dock::OverlayType::HighlightedAreas;
        style.overlay.feel.center_drop_coverage = 0.25;
        style.overlay.feel.window_drop_coverage = 0.55;
        egui_dock::DockArea::new(&mut dock)
            .id(egui::Id::new("paintatlas_workspace"))
            .style(style)
            .draggable_tabs(dragging)
            .show_leaf_close_all_buttons(false)
            .show(ctx, &mut PanelViewer(self));
        self.workspace.layout.dock = dock;
        self.apply_workspace_actions(ctx);
        self.external_panels(ctx);
        self.apply_workspace_actions(ctx);
    }
    fn panel_ui(&mut self, ui: &mut egui::Ui, panel: crate::workspace::Panel) {
        if self.image_editor.is_some() {
            // Keep document mutations blocked during previews without dimming the canvas.
            ui.visuals_mut().disabled_alpha = 1.0;
            ui.disable();
        }
        use crate::workspace::Panel;
        if panel == Panel::Canvas {
            self.document_canvas_ui(ui);
            return;
        }
        ui.push_id(("panel_contents", panel), |ui| {
            ui.add_enabled_ui(
                self.gesture.is_none() && self.selection.floating.is_none(),
                |ui| {
                    if panel == Panel::BrushSizes {
                        self.size_palette.contents(ui, &mut self.size);
                        return;
                    }
                    if panel == Panel::Tools {
                        egui::ScrollArea::vertical()
                            .id_salt(panel)
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.tools_ui(ui));
                        return;
                    }
                    egui::ScrollArea::both()
                        .id_salt(panel)
                        .auto_shrink([false, false])
                        .show(ui, |ui| match panel {
                            Panel::Tools => self.tools_ui(ui),
                            Panel::Properties => self.properties_ui(ui),
                            Panel::Brushes => self.brushes_ui(ui),
                            Panel::Colors => self.colors_ui(ui),
                            Panel::Layers => self.layers_ui(ui),
                            Panel::Navigation => self.navigation_ui(ui),
                            Panel::Files => self.files_ui(ui),
                            Panel::Help => self.help_ui(ui),
                            Panel::Settings if self.preferences.show(ui, &mut self.settings) => {
                                self.settings_pending = true;
                                self.settings_edited = std::time::Instant::now();
                            }
                            _ => {}
                        });
                },
            );
        });
    }
    fn files_ui(&mut self, ui: &mut egui::Ui) {
        for (label, action) in [
            ("Novo…", 0),
            ("Abrir…", 1),
            ("Salvar projeto", 2),
            ("Salvar como…", 3),
            ("Exportar PNG rápido…", 4),
            ("Exportar como…", 5),
        ] {
            if ui
                .add_sized(
                    [ui.available_width().max(140.0), 32.0],
                    egui::Button::new(label),
                )
                .clicked()
            {
                match action {
                    0 => self.request(Action::New),
                    1 => self.request(Action::Open),
                    2 => {
                        self.save(false);
                    }
                    3 => {
                        self.save(true);
                    }
                    4 => self.export(),
                    _ => self.open_export_as(),
                }
            }
        }
    }
    fn external_panels(&mut self, ctx: &egui::Context) {
        for external in self.workspace.layout.external.clone() {
            let mut close = false;
            let mut position = external.position;
            let mut size = external.size;
            let mut builder = egui::ViewportBuilder::default()
                .with_title(format!("PaintAtlas — {}", external.panel.title()))
                .with_inner_size(size)
                .with_min_inner_size([140.0, 180.0]);
            if let Some(p) = position {
                builder = builder.with_position(p);
            }
            ctx.show_viewport_immediate(
                panel_viewport_id(external.panel),
                builder,
                |ctx, class| {
                    if class == egui::ViewportClass::Embedded {
                        let mut open = true;
                        egui::Window::new(external.panel.title())
                            .id(egui::Id::new(("native_fallback", external.panel)))
                            .open(&mut open)
                            .resizable(true)
                            .default_size(size)
                            .show(ctx, |ui| {
                                self.reattach_menu(ui, external.panel);
                                ui.separator();
                                self.panel_ui(ui, external.panel);
                            });
                        close = !open;
                    } else {
                        close = ctx.input(|i| i.viewport().close_requested());
                        self.paint_external_canvas_backdrop(ctx);
                        egui::CentralPanel::default()
                            .frame(egui::Frame::central_panel(&ctx.style()).corner_radius(12))
                            .show(ctx, |ui| {
                                self.reattach_menu(ui, external.panel);
                                ui.separator();
                                self.panel_ui(ui, external.panel);
                            });
                        crate::theme::finish_glass(ctx);
                        self.capture_external_glass_smoke(ctx);
                        ctx.input(|i| {
                            if let Some(rect) = i.viewport().outer_rect {
                                position = Some(rect.min);
                            }
                            if let Some(rect) = i.viewport().inner_rect {
                                size = rect.size();
                            }
                        });
                    }
                },
            );
            if let Some(state) = self
                .workspace
                .layout
                .external
                .iter_mut()
                .find(|p| p.panel == external.panel)
            {
                state.position = position;
                state.size = size;
            }
            if close {
                self.workspace_actions
                    .push(crate::workspace::Action::Hide(external.panel));
            }
        }
    }
    fn reattach_menu(&mut self, ui: &mut egui::Ui, panel: crate::workspace::Panel) {
        ui.horizontal_wrapped(|ui| {
            if crate::theme::button(ui, "Reacoplar").clicked() {
                self.workspace_actions.push(crate::workspace::Action::Dock(
                    panel,
                    crate::workspace::Placement::Right,
                ));
            }
            ui.menu_button("Posição…", |ui| {
                for placement in crate::workspace::Placement::ALL {
                    if crate::theme::button(ui, placement.title()).clicked() {
                        self.workspace_actions
                            .push(crate::workspace::Action::Dock(panel, placement));
                        ui.close();
                    }
                }
                if crate::theme::button(ui, "Flutuar dentro do editor").clicked() {
                    self.workspace_actions
                        .push(crate::workspace::Action::Float(panel));
                    ui.close();
                }
            });
        });
    }
    fn require_initial_canvas(&mut self) {
        self.documents.clear();
        self.active_document = 0;
        self.startup_canvas_required = true;
        self.new_dialog = true;
    }

    fn create_canvas(&mut self, ctx: &egui::Context) {
        self.new_width = self.new_width.clamp(16, MAX_SIDE);
        self.new_height = self.new_height.clamp(16, MAX_SIDE);
        self.new_dpi = self.new_dpi.clamp(1, crate::document::MAX_DPI);
        self.settings.new_canvas = crate::presets::NewCanvasSettings {
            width: self.new_width,
            height: self.new_height,
            dpi: self.new_dpi,
            paper: self.white,
            paper_color: self.paper_color,
        };
        self.add_document(
            Document::with_paper(
                self.new_width,
                self.new_height,
                self.white.then_some(self.paper_color),
                self.new_dpi,
            ),
            None,
            false,
        );
        self.color = Color32::BLACK;
        self.status = "Nova tela criada".into();
        self.settings_pending = true;
        self.persist_presets(ctx, true);
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(action) = self.pending {
            egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
                ui.heading("Salvar seu desenho?");
                if !self.documents.is_empty() {
                    ui.label(self.document_title(self.active_document));
                }
                ui.label("Há alterações que ainda não foram salvas neste projeto.");
                ui.horizontal(|ui| {
                    if crate::theme::button(ui, "Salvar e continuar").clicked() && self.save(false)
                    {
                        self.pending = None;
                        self.perform(action);
                    }
                    if crate::theme::button(ui, "Descartar alterações").clicked() {
                        self.pending = None;
                        self.perform(action);
                    }
                    if crate::theme::button(ui, "Cancelar").clicked() {
                        self.pending = None;
                    }
                });
            });
        }
        if self.new_dialog && !self.startup_canvas_required {
            egui::Modal::new(egui::Id::new("new")).show(ctx, |ui| {
                self.new_canvas_contents(ctx, ui);
            });
        }
    }

    fn startup_splash(&mut self, ctx: &egui::Context) {
        let texture = self.splash_background.get_or_insert_with(|| {
            let image = image::load_from_memory(include_bytes!("../assets/background.jpg"))
                .expect("Imagem de fundo da splash inválida")
                .into_rgba8();
            ctx.load_texture(
                "startup_background",
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            )
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let bounds = ui.max_rect();
                let size = texture.size_vec2();
                let scale = (bounds.width() / size.x).max(bounds.height() / size.y);
                let uv_size = bounds.size() / (size * scale);
                let uv = egui::Rect::from_center_size(Pos2::new(0.5, 0.5), uv_size);
                ui.painter().image(texture.id(), bounds, uv, Color32::WHITE);
                ui.painter()
                    .rect_filled(bounds, 0.0, Color32::from_black_alpha(38));
            });
        egui::Area::new(egui::Id::new("startup_splash"))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .movable(false)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 48.0).clamp(220.0, 440.0);
                ui.set_width(width);
                ui.set_max_height((ctx.content_rect().height() - 40.0).max(120.0));
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 40.0).max(120.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("PaintAtlas")
                                    .size(46.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                            ui.label(
                                egui::RichText::new("Um espaço para suas ideias.")
                                    .size(15.0)
                                    .color(Color32::from_gray(220)),
                            );
                        });
                        ui.add_space(24.0);
                        crate::theme::glass_frame(ui)
                            .inner_margin(24.0)
                            .corner_radius(24)
                            .show(ui, |ui| {
                                ui.set_width((width - 48.0).max(172.0));
                                self.new_canvas_contents(ctx, ui);
                            });
                    });
            });
    }

    fn new_canvas_contents(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Nova tela");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Largura");
            ui.add(
                egui::DragValue::new(&mut self.new_width)
                    .range(16..=MAX_SIDE)
                    .suffix(" px"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Altura");
            ui.add(
                egui::DragValue::new(&mut self.new_height)
                    .range(16..=MAX_SIDE)
                    .suffix(" px"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Resolução");
            ui.add(
                egui::DragValue::new(&mut self.new_dpi)
                    .range(1..=crate::document::MAX_DPI)
                    .suffix(" DPI"),
            );
        });
        crate::theme::caption(
            ui,
            format!(
                "Impressão: {:.1} × {:.1} cm · o DPI não altera a quantidade de pixels",
                self.new_width as f64 / self.new_dpi.max(1) as f64 * 2.54,
                self.new_height as f64 / self.new_dpi.max(1) as f64 * 2.54,
            ),
        );
        crate::theme::toggle(ui, &mut self.white, "Adicionar camada de papel");
        ui.add_enabled_ui(self.white, |ui| {
            ui.horizontal(|ui| {
                ui.label("Cor do papel");
                ui.color_edit_button_srgb(&mut self.paper_color);
            });
        });
        ui.horizontal_wrapped(|ui| {
            if crate::theme::button(ui, "Criar").clicked() {
                self.create_canvas(ctx);
            }
            if self.startup_canvas_required {
                if crate::theme::button(ui, "Abrir arquivo…")
                    .on_hover_text("Abrir projeto PaintAtlas ou imagem PNG, JPG, WebP, GIF ou BMP")
                    .clicked()
                {
                    self.perform(Action::Open);
                }
                if crate::theme::button(ui, "Colar imagem")
                    .on_hover_text(format!(
                        "{} · Criar canvas no tamanho da imagem copiada",
                        self.settings.shortcuts.label(Shortcut::Paste)
                    ))
                    .clicked()
                {
                    self.paste_from_clipboard(ctx);
                }
                if crate::theme::button(ui, "Sair").clicked() {
                    self.allow_close = true;
                }
            } else if crate::theme::button(ui, "Cancelar").clicked() {
                self.new_dialog = false;
            }
        });
        if self.startup_canvas_required {
            crate::theme::caption(
                ui,
                format!(
                    "Imagem copiada? Use {} para abrir no tamanho original.",
                    self.settings.shortcuts.label(Shortcut::Paste)
                ),
            );
        }
        if self.startup_canvas_required
            && (self.status.starts_with("Erro ao abrir:")
                || self.status.starts_with("Erro ao importar:")
                || self.status.starts_with("Erro ao colar:"))
        {
            ui.colored_label(ui.visuals().error_fg_color, &self.status);
        }
    }
}
impl eframe::App for PaintAtlas {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if !ctx.wants_keyboard_input() && !self.preferences.capturing_shortcut() {
            self.settings
                .shortcuts
                .normalize_zoom_events(&mut input.events);
        }
        if !ctx.wants_keyboard_input() || self.preferences.capturing_shortcut() {
            crate::shortcuts::deduplicate_clipboard_events(&mut input.events);
        }
        if self.screenshot.is_some()
            && std::env::var("PAINTATLAS_SMOKE_SCENE")
                .is_ok_and(|s| s == "image-menu" || s == "filter-menu")
        {
            input.time = Some(self.frames as f64 / 60.0);
            let filters = std::env::var("PAINTATLAS_SMOKE_SCENE").as_deref() == Ok("filter-menu");
            let id = match (filters, self.frames >= 10) {
                (true, true) => "filter_blur_menu_rect",
                (true, false) => "filter_menu_rect",
                (false, true) => "image_adjustments_rect",
                (false, false) => "image_menu_rect",
            };
            if let Some(rect) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(id))) {
                input.events.push(egui::Event::PointerMoved(rect.center()));
                if self.frames == 4 || self.frames == 5 {
                    input.events.push(egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: self.frames == 4,
                        modifiers: Default::default(),
                    });
                }
            }
        }
        // Native popup smoke test: inject events only into this opt-in test run.
        if self.screenshot.is_some()
            && std::env::var("PAINTATLAS_SMOKE_SCENE").as_deref() == Ok("layer-context-menu")
            && let Some(rows) =
                ctx.data(|d| d.get_temp::<Vec<egui::Rect>>(egui::Id::new("layer_motion_demo_rows")))
            && let Some(rect) = rows.first()
            && (self.frames == 4 || self.frames == 5)
        {
            input.events.push(egui::Event::PointerMoved(rect.center()));
            input.events.push(egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Secondary,
                pressed: self.frames == 4,
                modifiers: Default::default(),
            });
        }
        if self.screenshot.is_some()
            && ctx.viewport_id() == egui::ViewportId::ROOT
            && std::env::var("PAINTATLAS_SMOKE_SCENE").is_ok_and(|s| s.starts_with("layer-fluid"))
        {
            input
                .events
                .retain(|e| matches!(e, egui::Event::Screenshot { .. }));
            input.time = Some(self.frames as f64 / 60.0);
            if let Some(rows) =
                ctx.data(|d| d.get_temp::<Vec<egui::Rect>>(egui::Id::new("layer_motion_demo_rows")))
                && rows.len() >= 2
            {
                let start = rows[0].center();
                let end = rows[rows.len() - 1].center() + Vec2::new(0.0, 16.0);
                let t = ((self.frames as f32 - 5.0) / 14.0).clamp(0.0, 1.0);
                let p =
                    start.lerp(end, t) + Vec2::new((t * std::f32::consts::PI).sin() * -22.0, 0.0);
                input.events.push(egui::Event::PointerMoved(p));
                if self.frames == 4 || self.frames == 22 {
                    input.events.push(egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed: self.frames == 4,
                        modifiers: Default::default(),
                    });
                }
            }
            return;
        }
        // Replay the same noisy movement through the actual native canvas at 0/100%.
        // These events are confined to the explicit screenshot test instance.
        if self.screenshot.is_some()
            && ctx.viewport_id() == egui::ViewportId::ROOT
            && std::env::var("PAINTATLAS_SMOKE_SCENE").is_ok_and(|s| s.starts_with("stabilizer"))
        {
            input
                .events
                .retain(|e| matches!(e, egui::Event::Screenshot { .. }));
            input.time = Some(self.frames as f64 / 120.0);
            let phase = match self.frames {
                5..=66 => Some((5, 180.0, 0.0)),
                90..=151 => Some((90, 420.0, 100.0)),
                _ => None,
            };
            if let Some((start, y, strength)) = phase {
                self.stabilization = strength;
                let step = (self.frames - start).min(60);
                if let Some((surface, node, _)) = self
                    .workspace
                    .layout
                    .dock
                    .find_tab(&crate::workspace::Panel::Canvas)
                    && let Some(leaf) = self.workspace.layout.dock[surface][node].get_leaf()
                {
                    let view = CanvasView {
                        center: leaf.viewport.center() + self.pan,
                        size: Vec2::new(800.0, 600.0),
                        zoom: self.zoom,
                        angle: 0.0,
                        mirrored: false,
                    };
                    let p = view.screen([
                        100.0 + step as f32 * 8.0,
                        y + (step as f32 * 0.1).sin() * 40.0
                            + if step.is_multiple_of(2) { 18.0 } else { -18.0 },
                    ]);
                    input.events.push(egui::Event::PointerMoved(p));
                    if self.frames == start || self.frames == start + 61 {
                        input.events.push(egui::Event::PointerButton {
                            pos: p,
                            button: egui::PointerButton::Primary,
                            pressed: self.frames == start,
                            modifiers: Default::default(),
                        });
                    }
                }
            }
            return;
        }
        // Deterministic hover for the opt-in render smoke test; never moves the OS pointer.
        if self.screenshot.is_some()
            && ctx.viewport_id() == egui::ViewportId::ROOT
            && std::env::var("PAINTATLAS_SMOKE_SCENE").is_ok_and(|s| s == "textures")
        {
            let bounds = input.screen_rect.unwrap_or_else(|| ctx.content_rect());
            input.events.push(egui::Event::PointerMoved(
                bounds.center() + Vec2::new(150.0, 150.0),
            ));
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.image_import = None;
        self.image_editor = None;
        self.clipboard = Default::default();
        if let Some(shader) = &self.glass_shader {
            shader.destroy();
        }
        if let Some(shader) = &self.blend_shader {
            shader.destroy();
        }
        if let Some(shader) = &self.selection_shader {
            shader.destroy();
        }
        let ctx = egui::Context::default();
        self.persist_presets(&ctx, true);
        if let Err(e) = self.workspace.persist(&ctx, true) {
            eprintln!("Layout: {e}");
        }
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::theme::apply(ctx, &self.settings.appearance);
        if std::mem::take(&mut self.maximize_on_start) {
            // Some X11 window managers ignore the maximized creation hint.
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.selection.floating.is_some() || self.image_editor.is_some())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status = format!(
                "Aplique ({}) ou cancele ({}) a transformação antes de fechar.",
                self.settings.shortcuts.label(Shortcut::Apply),
                self.settings.shortcuts.label(Shortcut::Cancel)
            );
        } else if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.any_dirty_document()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.gesture = None;
            self.request(Action::Close);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if !self
            .workspace
            .layout
            .contains(crate::workspace::Panel::Settings)
        {
            self.preferences.cancel_shortcut_capture();
        }
        self.handle_image_drops(ctx);
        if self.startup_canvas_required {
            self.clipboard_shortcuts(ctx);
        }
        self.prepare_canvas_backdrop(ctx);
        if self.startup_canvas_required {
            // No document controls are available until the first canvas is created.
            self.new_dialog = true;
            self.startup_splash(ctx);
        } else {
            self.shortcuts(ctx);
            self.top(ctx);
            self.workspace_ui(ctx);
            self.brush_editor_ui(ctx);
        }
        self.dialogs(ctx);
        self.image_dialog(ctx);
        self.export_as_ui(ctx);
        self.clipboard_feedback_ui(ctx);
        self.image_drop_overlay(ctx);
        crate::theme::finish_glass(ctx);
        if self.gesture.is_none() && !self.opacity_edit && self.image_editor.is_none() {
            self.history.commit(&self.doc);
        }

        self.persist_presets(ctx, false);
        if let Err(e) = self.workspace.persist(ctx, false) {
            self.status = format!("Erro ao salvar layout: {e}");
        }
        // Optional native-render smoke test; exits after capturing this app's viewport.
        if let Some(path) = &self.screenshot {
            self.frames += 1;
            let capture_frame = match std::env::var("PAINTATLAS_SMOKE_SCENE").as_deref() {
                Ok("image-drop") => {
                    if self.selection.floating.is_some() {
                        self.frames
                    } else {
                        600
                    }
                }
                Ok("filter-gallery") => {
                    if self.frames >= 20
                        && self
                            .image_editor
                            .as_ref()
                            .is_some_and(image_menu::Editor::gallery_ready)
                    {
                        self.frames
                    } else {
                        600
                    }
                }
                Ok("image-menu" | "filter-menu") => 40,
                Ok("stabilizer") => 170,
                Ok("stabilizer-live") => 140,
                Ok("layer-fluid-drop") => 34,
                Ok("fill-reveal" | "fill-reveal-solid") => 14,
                Ok("clipboard-flight-copy" | "clipboard-flight-paste") => 24,
                Ok("glass-canvas-native") => 60,
                _ => 20,
            };
            if self.frames == capture_frame {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let image = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = image {
                if std::env::var("PAINTATLAS_SMOKE_SCENE")
                    .is_ok_and(|s| s.starts_with("blend-modes"))
                    && let Some(bounds) = self.last_canvas_bounds
                {
                    let center = bounds.center() + self.pan;
                    let meta = serde_json::json!({"center":[center.x,center.y],"zoom":self.zoom,"scale":ctx.pixels_per_point()});
                    let _ = std::fs::write(path.with_extension("json"), meta.to_string());
                    if self.selection.floating.is_some() {
                        let pixels = self.fallback_blend_composite();
                        let _ = image::save_buffer(
                            path.with_file_name("reference.png"),
                            &pixels,
                            self.doc.width,
                            self.doc.height,
                            image::ColorType::Rgba8,
                        );
                    } else {
                        let _ = self.doc.export(&path.with_file_name("reference.png"));
                    }
                }
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                match image::save_buffer(
                    path,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => eprintln!("Screenshot: {}", path.display()),
                    Err(e) => eprintln!("Screenshot failed: {e}"),
                }
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }
}
fn panel_viewport_id(panel: crate::workspace::Panel) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("panel_window", panel))
}
struct PanelViewer<'a>(&'a mut PaintAtlas);
impl egui_dock::TabViewer for PanelViewer<'_> {
    type Tab = crate::workspace::Panel;
    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }
    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(("dock_panel", *tab))
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        self.0.panel_ui(ui, *tab);
    }
    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        *tab != crate::workspace::Panel::Canvas
    }
    fn allowed_in_windows(&self, tab: &mut Self::Tab) -> bool {
        *tab != crate::workspace::Panel::Canvas
    }
    fn min_window_size(&self, tab: &Self::Tab) -> Vec2 {
        if *tab == crate::workspace::Panel::BrushSizes {
            Vec2::new(64.0, 32.0)
        } else {
            Vec2::new(96.0, 32.0)
        }
    }
    fn clear_background(&self, tab: &Self::Tab) -> bool {
        *tab != crate::workspace::Panel::Canvas && !self.0.floating_panels.contains(tab)
    }
    fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
        [false, false]
    }
    fn on_tab_button(&mut self, _tab: &mut Self::Tab, response: &egui::Response) {
        response.clone().on_hover_text("Arraste esta aba para encaixar em outra área ou solte fora para flutuar. Botão direito: posição e janela do sistema.");
    }
    fn context_menu(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut Self::Tab,
        _surface: egui_dock::SurfaceIndex,
        _node: egui_dock::NodeIndex,
    ) {
        if *tab == crate::workspace::Panel::Canvas {
            return;
        }
        if crate::theme::button(ui, "Flutuar no editor").clicked() {
            self.0
                .workspace_actions
                .push(crate::workspace::Action::Float(*tab));
            ui.close();
        }
        if crate::theme::button(ui, "Abrir como janela do sistema").clicked() {
            self.0
                .workspace_actions
                .push(crate::workspace::Action::External(*tab));
            ui.close();
        }
        ui.menu_button("Acoplar…", |ui| {
            for placement in crate::workspace::Placement::ALL {
                if crate::theme::button(ui, placement.title()).clicked() {
                    self.0
                        .workspace_actions
                        .push(crate::workspace::Action::Dock(*tab, placement));
                    ui.close();
                }
            }
        });
    }
}
fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .small()
            .strong()
            .color(Color32::from_rgb(144, 158, 170)),
    );
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Rect;
    #[test]
    fn opacity_drag_reuses_large_canvas_textures_and_is_one_undo_step() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(3000, 3000, true), None, false);
        let _ = ctx.run(
            egui::RawInput {
                max_texture_side: Some(4096),
                ..Default::default()
            },
            |ctx| app.sync_layer_textures(ctx),
        );
        let ids: Vec<_> = app.textures.iter().map(|texture| texture.id()).collect();
        let output = ctx.run(Default::default(), |ctx| {
            for step in 1..=100 {
                app.set_layer_opacity(step as f32 / 200.0);
                app.sync_layer_textures(ctx);
            }
        });
        assert!(output.textures_delta.set.is_empty());
        assert_eq!(
            ids,
            app.textures
                .iter()
                .map(|texture| texture.id())
                .collect::<Vec<_>>()
        );
        assert!(!app.refresh);
        assert!(app.dirty);
        app.opacity_edit = false;
        app.history.commit(&app.doc);
        assert_eq!(app.history.counts(), (1, 0));
        app.undo();
        assert_eq!(app.doc.layers[app.doc.active].opacity, 1.0);
        app.redo();
        assert_eq!(app.doc.layers[app.doc.active].opacity, 0.5);
    }
    #[test]
    fn opacity_invalidates_cpu_blend_fallback_without_reuploading_layers() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, false);
        app.doc.layers[0].pixels = [200, 100, 50, 255].repeat(64);
        app.doc.layers[0].blend_mode = crate::blend::BlendMode::Multiply;
        let _ = ctx.run(Default::default(), |ctx| app.sync_layer_textures(ctx));
        let fallback = app.blend_fallback.as_ref().unwrap().id();
        let output = ctx.run(Default::default(), |ctx| {
            app.set_layer_opacity(0.25);
            app.sync_layer_textures(ctx);
        });
        assert_eq!(output.textures_delta.set.len(), 1);
        assert_eq!(output.textures_delta.set[0].0, fallback);
        assert!(!app.refresh && !app.blend_fallback_dirty);
        assert_eq!(app.fallback_blend_composite()[3], 64);
    }
    #[test]
    fn custom_apply_binding_works_only_during_transform_and_preserves_undo() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(32, 32, false), None, false);
        app.doc.layers[0].pixels[(8 * 32 + 8) * 4 + 3] = 255;
        let original = app.doc.layers[0].pixels.clone();
        app.selection.mask = crate::selection::Mask::rectangle(
            32,
            32,
            Rect::from_min_size(Pos2::new(4.0, 4.0), Vec2::splat(12.0)),
        )
        .map(std::sync::Arc::new);
        app.doc.selection = app.selection.mask.clone();
        let binding = serde_json::from_str(
            r#"{"key":"K","ctrl":false,"shift":false,"alt":false,"super_key":false}"#,
        )
        .unwrap();
        app.settings
            .shortcuts
            .assign(Shortcut::Apply, 0, binding, false);
        app.begin_transform(&ctx, false);
        let f = app.selection.floating.as_mut().unwrap();
        f.bounds = f.bounds.translate(Vec2::new(3.0, 0.0));
        for key in [egui::Key::Enter, egui::Key::K] {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
            assert_eq!(app.selection.floating.is_some(), key == egui::Key::Enter);
        }
        assert_ne!(app.doc.layers[0].pixels, original);
        app.undo();
        assert_eq!(app.doc.layers[0].pixels, original);
    }
    #[test]
    fn custom_tool_binding_replaces_default_without_affecting_text_fields() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(32, 32, false), None, false);
        app.settings.shortcuts = Default::default();
        let binding = serde_json::from_str(
            r#"{"key":"J","ctrl":false,"shift":false,"alt":false,"super_key":false}"#,
        )
        .unwrap();
        assert!(
            app.settings
                .shortcuts
                .assign(Shortcut::Eraser, 0, binding, false)
        );
        let press = |app: &mut PaintAtlas, key| {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
        };
        press(&mut app, egui::Key::E);
        assert!(app.tool == Tool::Brush);
        press(&mut app, egui::Key::J);
        assert!(app.tool == Tool::Eraser);
        app.select_tool(Tool::Brush);
        let id = egui::Id::new("shortcut_text_field");
        let mut text = String::new();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(id))
                    .request_focus();
            });
        });
        press(&mut app, egui::Key::J);
        assert!(app.tool == Tool::Brush);
    }
    #[test]
    fn initial_canvas_is_required_and_creation_persists_defaults() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        app.settings_path = Some(path.clone());
        app.require_initial_canvas();
        assert!(app.new_dialog && app.startup_canvas_required);
        let original = app.doc.composite();
        let point = Pos2::new(200.0, 200.0);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        assert_eq!(app.doc.composite(), original);
        assert!(app.gesture.is_none());
        app.new_width = 320;
        app.new_height = 240;
        app.new_dpi = 600;
        app.white = true;
        app.paper_color = [240, 215, 180];
        app.create_canvas(&ctx);
        assert!(!app.new_dialog && !app.startup_canvas_required);
        assert_eq!(
            (app.doc.width, app.doc.height, app.doc.dpi),
            (320, 240, 600)
        );
        assert_eq!(&app.doc.layers[0].pixels[..4], &[240, 215, 180, 255]);
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.new_canvas, app.settings.new_canvas);
        app.perform(Action::New);
        assert!(app.new_dialog && !app.startup_canvas_required);
        assert_eq!(app.new_width, loaded.new_canvas.width);
        assert_eq!(app.paper_color, loaded.new_canvas.paper_color);
    }

    #[test]
    fn tools_reflow_when_panel_shrinks_and_expands() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        for width in [360.0, 160.0, 80.0, 36.0, 360.0] {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            app.panel_ui(ui, crate::workspace::Panel::Tools);
                        });
                },
            );
            let buttons: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::epaint::Shape::Rect(rect) = &shape.shape
                        && (rect.rect.height() - 29.0).abs() < 0.01
                        && rect.corner_radius == egui::CornerRadius::same(8)
                    {
                        Some(rect.rect)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(buttons.len(), 10, "width {width}");
            for rect in buttons {
                assert!(
                    rect.left() >= 0.0 && rect.right() <= width + 0.01,
                    "button {rect:?} outside panel width {width}"
                );
            }
            let has_labels = output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::epaint::Shape::Text(text)
                    if text.galley.job.text == "Pincel")
            });
            assert_eq!(has_labels, width >= 196.0, "width {width}");
        }
    }

    #[test]
    fn selection_canvas_move_cancel_apply_and_history_with_rotated_mirrored_view() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(16, 16, false), None, false);
        app.fit = false;
        app.zoom = 8.0;
        app.angle = 0.4;
        app.mirrored = true;
        app.tool = Tool::SelectRect;
        let i = (3 * 16 + 3) * 4;
        app.doc.layers[0].pixels[i..i + 4].copy_from_slice(&[245, 30, 70, 128]);
        let original = app.doc.layers[0].pixels.clone();
        let view = CanvasView {
            center: Pos2::new(200.0, 200.0),
            size: Vec2::splat(16.0),
            zoom: 8.0,
            angle: 0.4,
            mirrored: true,
        };
        let draw = |app: &mut PaintAtlas, events| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    app.canvas(ctx);
                },
            );
            app.history.commit(&app.doc);
            output
        };
        let button = |p, pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let start = view.screen([2.25, 2.25]);
        let end = view.screen([6.75, 6.75]);
        draw(&mut app, vec![]);
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        draw(&mut app, vec![egui::Event::PointerMoved(end)]);
        draw(&mut app, vec![button(end, false)]);
        assert_eq!(
            app.selection.mask.as_ref().unwrap().bounds,
            Rect::from_min_max(Pos2::new(2.0, 2.0), Pos2::new(7.0, 7.0))
        );
        assert_eq!(app.history.counts(), (0, 0));
        assert!(!app.dirty);
        app.begin_transform(&ctx, false);
        app.selection.floating.as_mut().unwrap().bounds =
            Rect::from_min_max(Pos2::new(-5.0, -5.0), Pos2::new(40.0, 40.0));
        app.cancel_transform();
        assert_eq!(app.doc.layers[0].pixels, original);
        assert!(!app.dirty);
        app.begin_transform(&ctx, false);
        draw(&mut app, vec![]);
        let a = view.screen([4.5, 4.5]);
        let b = view.screen([8.5, 4.5]);
        draw(&mut app, vec![egui::Event::PointerMoved(a)]);
        draw(&mut app, vec![button(a, true)]);
        draw(&mut app, vec![egui::Event::PointerMoved(b)]);
        draw(&mut app, vec![button(b, false)]);
        assert_eq!(
            app.selection.floating.as_ref().unwrap().bounds.min,
            Pos2::new(6.0, 2.0)
        );
        assert_eq!(app.doc.layers[0].pixels, original);
        draw(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(app.selection.floating.is_none());
        assert!(app.dirty);
        assert_eq!(&app.doc.layers[0].pixels[i..i + 4], &[0, 0, 0, 0]);
        let dest = (3 * 16 + 7) * 4;
        assert_eq!(
            &app.doc.layers[0].pixels[dest..dest + 4],
            &[245, 30, 70, 128]
        );
        assert_eq!(app.history.counts(), (1, 0));
        app.undo();
        assert_eq!(app.doc.layers[0].pixels, original);
        app.redo();
        assert_eq!(
            &app.doc.layers[0].pixels[dest..dest + 4],
            &[245, 30, 70, 128]
        );
    }
    #[test]
    fn selection_toolbar_duplicates_and_corner_handles_resize_without_painting() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(16, 16, false), None, false);
        app.fit = false;
        app.zoom = 8.0;
        app.tool = Tool::SelectRect;
        let i = (3 * 16 + 3) * 4;
        app.doc.layers[0].pixels[i..i + 4].copy_from_slice(&[250, 40, 60, 255]);
        let original = app.doc.layers[0].pixels.clone();
        app.selection.mask = crate::selection::Mask::rectangle(
            16,
            16,
            Rect::from_min_size(Pos2::new(2.0, 2.0), Vec2::splat(4.0)),
        )
        .map(std::sync::Arc::new);
        app.doc.selection = app.selection.mask.clone();
        let draw = |app: &mut PaintAtlas, events| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    app.canvas(ctx);
                },
            );
            app.history.commit(&app.doc);
            output
        };
        let button = |p, pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let duplicate = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Duplicar recorte" => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        draw(&mut app, vec![egui::Event::PointerMoved(duplicate)]);
        draw(&mut app, vec![button(duplicate, true)]);
        draw(&mut app, vec![button(duplicate, false)]);
        assert!(app.selection.floating.as_ref().unwrap().duplicate);
        assert_eq!(app.doc.layers[0].pixels, original);
        draw(&mut app, vec![]);
        let corner = Pos2::new(184.0, 184.0);
        let end = Pos2::new(200.0, 192.0);
        draw(&mut app, vec![egui::Event::PointerMoved(corner)]);
        draw(&mut app, vec![button(corner, true)]);
        draw(&mut app, vec![egui::Event::PointerMoved(end)]);
        draw(&mut app, vec![button(end, false)]);
        assert_eq!(
            app.selection.floating.as_ref().unwrap().bounds.size(),
            Vec2::splat(6.0)
        );
        assert_eq!(app.doc.layers[0].pixels, original);
        assert!(!app.dirty);
        app.selection.floating.as_mut().unwrap().bounds =
            Rect::from_min_size(Pos2::new(8.0, 2.0), Vec2::splat(4.0));
        app.apply_transform();
        app.history.commit(&app.doc);
        let j = (3 * 16 + 9) * 4;
        assert_eq!(&app.doc.layers[0].pixels[i..i + 4], &[250, 40, 60, 255]);
        assert_eq!(&app.doc.layers[0].pixels[j..j + 4], &[250, 40, 60, 255]);
        assert_eq!(app.history.counts(), (1, 0));
        app.undo();
        assert_eq!(app.doc.layers[0].pixels, original);
    }
    #[test]
    fn layer_drag_reorders_both_directions_and_roundtrips_history() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.doc = Document::new(8, 8, true);
        app.doc.add_layer();
        for (layer, name) in app.doc.layers.iter_mut().zip(["Bottom", "Middle", "Top"]) {
            layer.name = name.into();
        }
        app.rename = "Top".into();
        let draw = |app: &mut PaintAtlas, events| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 700.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.layers_ui(ui));
                },
            );
            app.history.commit(&app.doc);
            output
        };
        let position = |output: &egui::FullOutput, name: &str| {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == name => {
                        Some(t.pos + Vec2::new(10.0, 6.0))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let mut output = draw(&mut app, vec![]);
        assert!(position(&output, "Opacidade da camada").y < position(&output, "Top").y);
        for (source, target, offset, expected) in [
            ("Top", "Bottom", 28.0, ["Top", "Bottom", "Middle"]),
            ("Top", "Middle", -12.0, ["Bottom", "Middle", "Top"]),
        ] {
            let start = position(&output, source);
            let end = position(&output, target) + Vec2::new(0.0, offset);
            draw(&mut app, vec![egui::Event::PointerMoved(start)]);
            draw(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
            );
            draw(
                &mut app,
                vec![egui::Event::PointerMoved(start + Vec2::new(0.0, 8.0))],
            );
            draw(&mut app, vec![egui::Event::PointerMoved(end)]);
            draw(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            assert_eq!(
                app.doc
                    .layers
                    .iter()
                    .map(|l| l.name.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(app.doc.layers[app.doc.active].name, "Top");
            for _ in 0..45 {
                output = draw(&mut app, vec![]);
            }
        }
        assert_eq!(app.history.counts(), (2, 0));
        app.undo();
        assert_eq!(app.doc.layers[0].name, "Top");
        app.redo();
        assert_eq!(app.doc.layers[2].name, "Top");
        let counts = app.history.counts();
        app.reorder_layer(2, 3);
        app.history.commit(&app.doc);
        assert_eq!(app.history.counts(), counts);
    }
    fn frame(app: &mut PaintAtlas, ctx: &egui::Context, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                events,
                ..Default::default()
            },
            |ctx| app.canvas(ctx),
        );
    }
    #[test]
    fn color_panel_switches_picker_mode_without_changing_paint_color_or_document() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        let original = Color32::from_rgb(134, 72, 191);
        app.color = original;
        let frame = |app: &mut PaintAtlas, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(420.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.colors_ui(ui));
                },
            )
        };
        frame(&mut app, vec![]);
        for mode in ["Triangular", "Retangular", "Triangular"] {
            let out = frame(&mut app, vec![]);
            let p = out
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == mode => {
                        Some(t.pos + t.galley.rect.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            frame(&mut app, vec![egui::Event::PointerMoved(p)]);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                );
            }
            assert_eq!(app.settings.triangular_color_selector, mode == "Triangular");
            assert_eq!(app.color, original);
            assert!(!app.dirty);
        }
        assert!(app.settings_pending);
    }
    #[test]
    fn tip_gallery_opens_and_selects_a_builtin_mask() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        let expected = app.tip_library.entries[0].id.clone();
        let label = app.tip_library.entries[0].name.clone();
        let mut selected = None;
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 700.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| app.tip_selector_ui(ui, &mut selected));
                },
            )
        };
        let text_center = |out: &egui::FullOutput, text: &str| {
            out.shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == text => {
                        Some(t.pos + t.galley.rect.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing gallery label: {text}"))
        };
        frame(vec![]);
        let out = frame(vec![]);
        let p = text_center(&out, "Circular (sem textura)");
        frame(vec![egui::Event::PointerMoved(p)]);
        frame(vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        frame(vec![]);
        let out = frame(vec![]);
        let p = text_center(&out, &label);
        frame(vec![egui::Event::PointerMoved(p)]);
        frame(vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        assert_eq!(selected, Some(expected));
        assert!(!app.dirty);
    }
    #[test]
    fn brush_cursor_uses_pressure_size_and_transforms_the_texture_outline() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.size = 100.0;
        app.pressure_enabled = true;
        app.pressure_gamma = 1.0;
        app.pressure_minimum = 0.1;
        let view = CanvasView {
            center: Pos2::new(200.0, 200.0),
            size: Vec2::splat(400.0),
            zoom: 2.0,
            angle: 0.0,
            mirrored: false,
        };
        for pressure in [0.0, 0.25, 1.0] {
            app.pressure = pressure;
            let brush = app.brush();
            assert!((brush.size - 100.0 * (0.1 + 0.9 * pressure)).abs() < 0.001);
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.paint_brush_cursor(ui.painter(), view, view.center, &brush)
                });
            });
            let radius = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Circle(c) => Some(c.radius),
                    _ => None,
                })
                .unwrap();
            assert!(
                (radius - brush.size).abs() < 0.001,
                "circle must use the same size as the actual stamp at 2x zoom"
            );
        }
        app.active_tip = Some(std::sync::Arc::new(
            crate::brush_tip::BrushTip::from_rgba(image::RgbaImage::from_pixel(
                16,
                8,
                image::Rgba([0, 0, 0, 255]),
            ))
            .unwrap(),
        ));
        let mut view = view;
        view.angle = 0.7;
        view.mirrored = true;
        let brush = app.brush();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.paint_brush_cursor(ui.painter(), view, view.center, &brush)
            });
        });
        let path = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Path(p) => Some(p),
                _ => None,
            })
            .unwrap();
        for p in &path.points {
            let d = view.document(*p);
            assert!((d[0] - 200.0).abs() <= brush.size * 0.501);
            assert!((d[1] - 200.0).abs() <= brush.size * 0.251);
        }
        assert!(!app.dirty);
    }
    #[test]
    fn circle_and_dot_cursors_override_textured_brush_without_changing_it() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.size = 100.0;
        app.pressure_enabled = false;
        app.active_tip = Some(std::sync::Arc::new(
            crate::brush_tip::BrushTip::from_rgba(image::RgbaImage::from_pixel(
                16,
                8,
                image::Rgba([0, 0, 0, 255]),
            ))
            .unwrap(),
        ));
        let view = CanvasView {
            center: Pos2::new(100.0, 100.0),
            size: Vec2::splat(200.0),
            zoom: 2.0,
            angle: 0.4,
            mirrored: true,
        };
        for (style, expected_radius) in [
            (crate::preferences::CursorStyle::Circle, 100.0),
            (crate::preferences::CursorStyle::Dot, 3.0),
        ] {
            app.settings.cursor_style = style;
            let brush = app.brush();
            let output = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.paint_brush_cursor(ui.painter(), view, view.center, &brush);
                });
            });
            let circles: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Circle(c) => Some(c.radius),
                    _ => None,
                })
                .collect();
            assert_eq!(circles.len(), 2);
            assert_eq!(circles[0], expected_radius);
            assert!(brush.tip.is_some());
            assert_eq!(brush.size, 100.0);
        }
    }
    #[test]
    fn circle_cursor_pressure_is_optional_without_affecting_painted_size() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.settings.cursor_style = crate::preferences::CursorStyle::Circle;
        assert!(!app.settings.circle_cursor_pressure);
        app.size = 100.0;
        app.pressure_enabled = true;
        app.pressure_gamma = 1.0;
        app.pressure_minimum = 0.1;
        let view = CanvasView {
            center: Pos2::new(100.0, 100.0),
            size: Vec2::splat(200.0),
            zoom: 2.0,
            angle: 0.0,
            mirrored: false,
        };
        for follows_pressure in [false, true] {
            app.settings.circle_cursor_pressure = follows_pressure;
            for pressure in [0.0, 0.25, 1.0] {
                app.pressure = pressure;
                let brush = app.brush();
                let actual_size = 100.0 * (0.1 + 0.9 * pressure);
                assert!((brush.size - actual_size).abs() < 0.001);
                let output = ctx.run(Default::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        app.paint_brush_cursor(ui.painter(), view, view.center, &brush);
                    });
                });
                let radius = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Circle(c) => Some(c.radius),
                        _ => None,
                    })
                    .unwrap();
                let expected = if follows_pressure { actual_size } else { 100.0 };
                assert!((radius - expected).abs() < 0.001);
            }
        }
    }
    #[test]
    fn edited_brush_can_be_saved_as_a_new_independent_preset() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        assert!(!app.preset_modified());
        app.size = 81.0;
        app.hardness = 0.4;
        app.remember_preset();
        assert!(
            app.preset_modified(),
            "autosave must not hide modified state"
        );
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.properties_ui(ui));
        });
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == "Salvar como novo preset…")));
        app.open_brush_editor();
        let mut draft = app.brush_editor.take().unwrap();
        assert_eq!(draft.size, 81.0);
        draft.name = "Caneta personalizada".into();
        draft.eraser = true;
        app.brush_editor = Some(draft);
        let editor_frame = |app: &mut PaintAtlas, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 1000.0))),
                    events,
                    ..Default::default()
                },
                |ctx| app.brush_editor_ui(ctx),
            )
        };
        editor_frame(&mut app, vec![]);
        let output = editor_frame(&mut app, vec![]);
        let button = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Criar preset" => {
                    Some(t.pos + t.galley.rect.size() * 0.5)
                }
                _ => None,
            })
            .expect("create action must be visible in the initial form");
        editor_frame(&mut app, vec![egui::Event::PointerMoved(button)]);
        editor_frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: button,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        editor_frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: button,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(app.brush_editor.is_none());
        assert_eq!(app.settings.brushes.len(), 6);
        assert_eq!(app.settings.selected, 5);
        assert!(app.tool == Tool::Eraser);
        assert!(!app.preset_modified());
        app.size = 32.0;
        app.select_preset(0);
        assert_eq!(app.size, 81.0);
        assert_eq!(app.settings.originals[0].size, 12.0);
        app.select_preset(5);
        assert_eq!(app.size, 32.0);
        assert_eq!(app.settings.originals[5].size, 81.0);
        assert!(app.preset_modified());
        assert!(!app.dirty, "preset changes must not modify the document");
    }
    #[test]
    fn docked_brush_sizes_can_shrink_on_all_canvas_edges() {
        use crate::workspace::{Action as DockAction, Panel, Placement};
        for placement in [
            Placement::Left,
            Placement::Right,
            Placement::Above,
            Placement::Below,
        ] {
            let ctx = egui::Context::default();
            let mut app = PaintAtlas::from_context(&ctx);
            app.workspace.layout.dock = egui_dock::DockState::new(vec![Panel::Canvas]);
            app.workspace
                .layout
                .apply(DockAction::Dock(Panel::BrushSizes, placement));
            let frame = |app: &mut PaintAtlas, events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1440.0, 940.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| app.workspace_ui(ctx),
                )
            };
            for _ in 0..4 {
                frame(&mut app, vec![]);
            }
            let (surface, node, _) = app
                .workspace
                .layout
                .dock
                .find_tab(&Panel::BrushSizes)
                .unwrap();
            let rect = app.workspace.layout.dock[surface][node].rect().unwrap();
            let (from, delta, horizontal) = match placement {
                Placement::Left => (
                    rect.right_center() + Vec2::new(0.5, 0.0),
                    Vec2::new(20.0 - rect.width(), 0.0),
                    true,
                ),
                Placement::Right => (
                    rect.left_center() - Vec2::new(0.5, 0.0),
                    Vec2::new(rect.width() - 20.0, 0.0),
                    true,
                ),
                Placement::Above => (
                    rect.center_bottom() + Vec2::new(0.0, 0.5),
                    Vec2::new(0.0, 20.0 - rect.height()),
                    false,
                ),
                Placement::Below => (
                    rect.center_top() - Vec2::new(0.0, 0.5),
                    Vec2::new(0.0, rect.height() - 20.0),
                    false,
                ),
                _ => unreachable!(),
            };
            frame(&mut app, vec![egui::Event::PointerMoved(from)]);
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
            );
            frame(&mut app, vec![egui::Event::PointerMoved(from + delta)]);
            frame(&mut app, vec![]);
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: from + delta,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            for _ in 0..3 {
                frame(&mut app, vec![]);
            }
            let rect = app.workspace.layout.dock[surface][node].rect().unwrap();
            let extent = if horizontal {
                rect.width()
            } else {
                rect.height()
            };
            assert!(
                (62.0..=66.0).contains(&extent),
                "docked minimum should be 64, got {extent}"
            );
            assert!(!app.dirty, "resizing a dock must not paint");
        }
    }
    #[test]
    fn floating_brush_sizes_can_shrink_to_a_narrow_short_palette() {
        use crate::workspace::{Action as DockAction, Panel};
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.workspace
            .layout
            .apply(DockAction::Float(Panel::BrushSizes));
        let surface = app
            .workspace
            .layout
            .dock
            .find_tab(&Panel::BrushSizes)
            .unwrap()
            .0;
        let frame = |app: &mut PaintAtlas, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 940.0))),
                    events,
                    ..Default::default()
                },
                |ctx| app.workspace_ui(ctx),
            )
        };
        for _ in 0..4 {
            frame(&mut app, vec![]);
        }
        let rect = app
            .workspace
            .layout
            .dock
            .get_window_state(surface)
            .unwrap()
            .rect();
        let from = rect.max - Vec2::splat(2.0);
        let target = rect.min + Vec2::new(80.0, 115.0);
        frame(&mut app, vec![egui::Event::PointerMoved(from)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(target)]);
        frame(&mut app, vec![]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        for _ in 0..3 {
            frame(&mut app, vec![]);
        }
        let resized = app
            .workspace
            .layout
            .dock
            .get_window_state(surface)
            .unwrap()
            .rect();
        assert!(
            resized.width() <= 95.0 && resized.height() <= 135.0,
            "palette failed to shrink: {rect:?} -> {resized:?}"
        );
        assert!(!app.dirty);
    }
    #[test]
    fn workspace_drag_tab_into_canvas_center_and_edges() {
        use crate::workspace::{Action as DockAction, Panel};
        for offset in [
            Vec2::ZERO,
            Vec2::new(-0.46, 0.0),
            Vec2::new(0.46, 0.0),
            Vec2::new(0.0, -0.46),
            Vec2::new(0.0, 0.46),
        ] {
            let ctx = egui::Context::default();
            let mut app = PaintAtlas::from_context(&ctx);
            let frame = |app: &mut PaintAtlas, events: Vec<egui::Event>| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1440.0, 940.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| app.workspace_ui(ctx),
                )
            };
            frame(&mut app, vec![]);
            frame(&mut app, vec![]);
            let output = frame(&mut app, vec![]);
            let from = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == "Cores" => {
                        Some(t.pos + t.galley.rect.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            let (surface, node, _) = app.workspace.layout.dock.find_tab(&Panel::Canvas).unwrap();
            let rect = app.workspace.layout.dock[surface][node].rect().unwrap();
            let target = rect.center() + rect.size() * offset;
            frame(&mut app, vec![egui::Event::PointerMoved(from)]);
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
            );
            frame(
                &mut app,
                vec![egui::Event::PointerMoved(from + Vec2::new(15.0, 15.0))],
            );
            frame(&mut app, vec![]);
            frame(&mut app, vec![egui::Event::PointerMoved(target)]);
            frame(&mut app, vec![]);
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: target,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            assert!(!app.dirty, "moving a panel must not paint on the canvas");
            let canvas = app.workspace.layout.dock.find_tab(&Panel::Canvas).unwrap();
            let colors = app.workspace.layout.dock.find_tab(&Panel::Colors).unwrap();
            assert_eq!(canvas.0, colors.0, "drop must remain in the main workspace");
            if offset == Vec2::ZERO {
                assert_eq!(canvas.1, colors.1, "center must group tabs");
            } else {
                assert_ne!(
                    canvas.1, colors.1,
                    "canvas edge must create its own dock area"
                );
                frame(&mut app, vec![]);
                let dock = &app.workspace.layout.dock;
                let canvas_rect = dock[canvas.0][canvas.1].rect().unwrap();
                let color_rect = dock[colors.0][colors.1].rect().unwrap();
                if offset.x < 0.0 {
                    assert!(color_rect.right() <= canvas_rect.left());
                }
                if offset.x > 0.0 {
                    assert!(color_rect.left() >= canvas_rect.right());
                }
                if offset.y < 0.0 {
                    assert!(color_rect.bottom() <= canvas_rect.top());
                }
                if offset.y > 0.0 {
                    assert!(color_rect.top() >= canvas_rect.bottom());
                }
            }
            app.workspace.layout.apply(DockAction::Hide(Panel::Layers));
            app.workspace_actions.push(DockAction::Open(Panel::Layers));
            app.apply_workspace_actions(&ctx);
            assert!(app.workspace.layout.contains(Panel::Layers));
            assert!(app.workspace.layout.valid());
        }
    }
    #[test]
    fn rotated_mirrored_canvas_paints_at_document_coordinate() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(64, 64, false), None, false);
        app.fit = false;
        app.zoom = 2.0;
        app.angle = 0.7;
        app.mirrored = true;
        app.size = 3.0;
        app.pressure_enabled = false;
        let pixels = app.doc.layers[0].pixels.clone();
        frame(&mut app, &ctx, vec![]);
        assert_eq!(pixels, app.doc.layers[0].pixels);
        assert!(!app.dirty);
        let view = CanvasView {
            center: Pos2::new(200.0, 200.0),
            size: Vec2::splat(64.0),
            zoom: 2.0,
            angle: 0.7,
            mirrored: true,
        };
        let p = view.screen([20.5, 21.5]);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(app.doc.layers[0].pixels[(21 * 64 + 20) * 4 + 3] > 0);
    }
    #[test]
    fn brush_and_eraser_shortcuts_restore_each_last_preset_and_settings() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        let mut brush = app.settings.brushes[2].clone();
        brush.name = "Meu pincel".into();
        app.create_brush(brush).unwrap();
        let brush_index = app.settings.selected;
        let mut eraser = app.settings.brushes[4].clone();
        eraser.name = "Minha borracha".into();
        app.create_brush(eraser).unwrap();
        let eraser_index = app.settings.selected;
        app.size = 53.0;
        app.select_preset(brush_index);
        app.size = 71.0;
        app.opacity = 0.6;
        let press = |app: &mut PaintAtlas, key| {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: false,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
        };
        for _ in 0..3 {
            press(&mut app, egui::Key::E);
            assert_eq!(app.settings.selected, eraser_index);
            assert_eq!(app.size, 53.0);
            press(&mut app, egui::Key::B);
            assert_eq!(app.settings.selected, brush_index);
            assert_eq!(app.size, 71.0);
            assert_eq!(app.opacity, 0.6);
        }
        app.select_tool(Tool::Hand);
        app.select_tool(Tool::Eraser);
        app.select_tool(Tool::Rectangle);
        app.select_tool(Tool::Brush);
        assert_eq!(app.settings.selected, brush_index);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("brushes.json");
        app.settings.save(&path).unwrap();
        let mut restored = PaintAtlas::from_context(&ctx);
        restored.settings = Settings::load(&path).unwrap();
        restored.apply_preset(restored.settings.selected);
        restored.select_tool(Tool::Eraser);
        assert_eq!(restored.settings.selected, eraser_index);
        restored.select_tool(Tool::Brush);
        assert_eq!(restored.settings.selected, brush_index);
        assert_eq!(restored.size, 71.0);
        assert!(!app.dirty);
    }
    #[test]
    fn switching_presets_remembers_independent_settings() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.size = 19.0;
        app.stabilization = 61.0;
        app.pressure_gamma = 1.4;
        app.select_preset(4);
        assert!(app.tool == Tool::Eraser);
        app.size = 47.0;
        app.select_preset(0);
        assert_eq!(app.size, 19.0);
        assert_eq!(app.stabilization, 61.0);
        assert_eq!(app.pressure_gamma, 1.4);
        app.select_tool(Tool::Eraser);
        assert_eq!(app.size, 47.0);
    }
    #[test]
    fn stabilization_100_visibly_smooths_painted_jitter_and_keeps_one_undo_step() {
        fn draw(amount: f32) -> (Document, f32) {
            let ctx = egui::Context::default();
            let mut app = PaintAtlas::from_context(&ctx);
            app.reset(Document::new(256, 256, false), None, false);
            app.fit = false;
            app.zoom = 1.0;
            app.size = 2.0;
            app.hardness = 1.0;
            app.pressure_enabled = false;
            app.stabilization = amount;
            let mut time = 0.0;
            let mut frame = |app: &mut PaintAtlas, events| {
                time += 1.0 / 120.0;
                let _ = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                        events,
                        ..Default::default()
                    },
                    |ctx| app.canvas(ctx),
                );
            };
            frame(&mut app, vec![]);
            let mut p = Pos2::new(100.0, 210.0);
            frame(&mut app, vec![egui::Event::PointerMoved(p)]);
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
            );
            for step in 1..=60 {
                p = Pos2::new(
                    100.0 + step as f32 * 3.0,
                    200.0 + if step % 2 == 0 { 10.0 } else { -10.0 },
                );
                frame(&mut app, vec![egui::Event::PointerMoved(p)]);
            }
            let gesture = app.gesture.as_ref().unwrap();
            let lag = (gesture.last[0] - gesture.raw_last[0])
                .hypot(gesture.last[1] - gesture.raw_last[1]);
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            assert!(app.gesture.is_none());
            app.history.commit(&app.doc);
            assert_eq!(app.history.counts(), (1, 0));
            let pixels = app.doc.layers[0].pixels.clone();
            app.undo();
            assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
            app.redo();
            assert_eq!(app.doc.layers[0].pixels, pixels);
            (app.doc, lag)
        }
        let (raw, raw_lag) = draw(0.0);
        let (smooth, smooth_lag) = draw(100.0);
        assert!(raw_lag < 0.001);
        assert!(smooth_lag > 20.0);
        let spread = |doc: &Document| {
            let mut min = 256;
            let mut max = 0;
            for x in 50..100 {
                for y in 0..256 {
                    if doc.layers[0].pixels[(y * 256 + x) * 4 + 3] > 32 {
                        min = min.min(y);
                        max = max.max(y);
                    }
                }
            }
            assert!(max >= min);
            (max - min) as f32
        };
        assert!(
            spread(&smooth) < spread(&raw) * 0.5,
            "painted jitter must be visibly smaller at 100%"
        );
        assert_ne!(raw.layers[0].pixels, smooth.layers[0].pixels);
    }
    #[test]
    fn stabilizer_flushes_to_final_point_on_release() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(64, 64, false), None, false);
        app.fit = false;
        app.zoom = 4.0;
        app.stabilization = 100.0;
        app.size = 2.0;
        app.pressure_enabled = false;
        let a = Pos2::new(184.0, 184.0);
        let b = Pos2::new(216.0, 216.0);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(a)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: a,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(b),
                egui::Event::PointerButton {
                    pos: b,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
        );
        assert!(app.gesture.is_none());
        assert!(app.doc.layers[0].pixels[(36 * 64 + 36) * 4 + 3] > 0);
    }
    #[test]
    fn reverse_dial_uses_physical_subtract_and_preserves_steps() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.settings.appearance.reduced_motion = true;
        app.reset(Document::new(64, 64, false), None, false);
        frame(&mut app, &ctx, vec![]);
        let initial = app.zoom;
        let event = |key, physical_key| egui::Event::Key {
            key,
            physical_key,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        };
        frame(
            &mut app,
            &ctx,
            vec![event(egui::Key::Equals, Some(egui::Key::Minus))],
        );
        assert!((app.zoom - initial / 1.25).abs() < 0.001);
        let previous = app.zoom;
        frame(
            &mut app,
            &ctx,
            vec![
                event(egui::Key::Plus, None),
                event(egui::Key::Minus, None),
                event(egui::Key::Minus, None),
            ],
        );
        assert!((app.zoom - previous / 1.25).abs() < 0.001);
        frame(&mut app, &ctx, vec![]);
        assert_eq!(ctx.zoom_factor(), 1.0);
    }
    #[test]
    fn rotation_keeps_zoomed_viewport_center_fixed_through_every_animation_frame() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        for mirrored in [false, true] {
            for reduced_motion in [false, true] {
                app.settings.appearance.reduced_motion = reduced_motion;
                app.zoom = 4.0;
                app.pan = Vec2::new(1600.0, -1100.0);
                app.angle = 350.0_f32.to_radians();
                app.rotation_target = None;
                let view = |app: &PaintAtlas| CanvasView {
                    center: Pos2::new(500.0, 400.0) + app.pan,
                    size: Vec2::new(3000.0, 3000.0),
                    zoom: app.zoom,
                    angle: app.angle,
                    mirrored,
                };
                let center = Pos2::new(500.0, 400.0);
                let point = view(&app).document(center);
                app.request_rotation(45.0_f32.to_radians());
                for _ in 0..90 {
                    app.animate_rotation(1.0 / 60.0);
                    assert!(view(&app).screen(point).distance(center) < 0.02);
                }
                app.set_rotation_target(0.0);
                for _ in 0..90 {
                    app.animate_rotation(1.0 / 60.0);
                    assert!(view(&app).screen(point).distance(center) < 0.02);
                }
                assert!(app.rotation_target.is_none());
            }
        }
    }

    #[test]
    fn smooth_rotation_accumulates_reverses_and_restores_across_zero() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.settings.appearance.reduced_motion = false;
        app.angle = 0.0;
        let step = 15.0_f32.to_radians();
        app.request_rotation(step);
        app.request_rotation(step);
        app.animate_rotation(1.0 / 60.0);
        assert!(app.angle > 0.0 && app.angle < 2.0 * step);
        for _ in 0..90 {
            app.animate_rotation(1.0 / 60.0);
        }
        assert!((app.angle - 2.0 * step).abs() < 0.00001);
        assert!(app.rotation_target.is_none());
        app.request_rotation(-step);
        app.animate_rotation(1.0 / 60.0);
        assert!(app.angle < 2.0 * step && app.angle > step);
        app.angle = 350.0_f32.to_radians();
        app.rotation_target = None;
        app.set_rotation_target(0.0);
        app.animate_rotation(1.0 / 60.0);
        assert!(app.angle > 350.0_f32.to_radians());
        for _ in 0..90 {
            app.animate_rotation(1.0 / 60.0);
        }
        assert!(app.angle.abs() < 0.00001);
        app.settings.appearance.reduced_motion = true;
        app.request_rotation(-step);
        app.animate_rotation(1.0 / 60.0);
        assert!((app.angle - 345.0_f32.to_radians()).abs() < 0.00001);
        assert!(app.rotation_target.is_none());
    }

    #[test]
    fn smooth_rotation_speed_is_independent_of_frame_rate() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.settings.appearance.reduced_motion = false;
        let mut results = Vec::new();
        for fps in [30, 60, 120] {
            app.angle = 0.0;
            app.rotation_target = None;
            app.request_rotation(std::f32::consts::FRAC_PI_2);
            for _ in 0..fps / 5 {
                app.animate_rotation(1.0 / fps as f32);
            }
            results.push(app.angle);
        }
        assert!((results[0] - results[1]).abs() < 0.0001);
        assert!((results[1] - results[2]).abs() < 0.0001);
    }

    #[test]
    fn smooth_zoom_accumulates_reverses_and_keeps_anchor_fixed() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.settings.appearance.reduced_motion = false;
        app.zoom = 2.0;
        app.pan = Vec2::new(25.0, -10.0);
        let anchor = Vec2::new(-75.0, 40.0);
        let document_point = (anchor - app.pan) / app.zoom;
        app.request_zoom(1.25, false, anchor);
        app.request_zoom(1.25, false, anchor);
        app.animate_zoom(1.0 / 60.0);
        assert!(app.zoom > 2.0 && app.zoom < 3.125);
        for _ in 0..90 {
            app.animate_zoom(1.0 / 60.0);
            assert!(((anchor - app.pan) / app.zoom - document_point).length() < 0.001);
        }
        assert_eq!(app.zoom, 3.125);
        assert!(app.zoom_animation.is_none());
        app.request_zoom(0.5, false, anchor);
        app.animate_zoom(1.0 / 60.0);
        assert!(app.zoom < 3.125 && app.zoom > 1.5625);
        for _ in 0..90 {
            app.animate_zoom(1.0 / 60.0);
        }
        assert_eq!(app.zoom, 1.5625);
        assert!(((anchor - app.pan) / app.zoom - document_point).length() < 0.001);
        app.request_zoom(1000.0, false, anchor);
        app.settings.appearance.reduced_motion = true;
        app.animate_zoom(1.0 / 60.0);
        assert_eq!(app.zoom, 16.0);
        assert!(app.zoom_animation.is_none());
    }

    #[test]
    fn smooth_zoom_speed_is_independent_of_frame_rate() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.settings.appearance.reduced_motion = false;
        let mut results = Vec::new();
        for fps in [30, 60, 120] {
            app.zoom = 1.0;
            app.zoom_animation = None;
            app.request_zoom(4.0, false, Vec2::ZERO);
            for _ in 0..fps / 5 {
                app.animate_zoom(1.0 / fps as f32);
            }
            results.push(app.zoom);
        }
        assert!((results[0] - results[1]).abs() < 0.0001);
        assert!((results[1] - results[2]).abs() < 0.0001);
    }

    #[test]
    fn negative_wheel_reduces_zoom() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(64, 64, false), None, false);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(Pos2::new(200.0, 200.0))],
        );
        let initial = app.zoom;
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(0.0, -1.0),
                modifiers: egui::Modifiers::CTRL,
            }],
        );
        assert!(app.zoom < initial);
        assert_eq!(ctx.zoom_factor(), 1.0);
    }
    #[test]
    fn tablet_dial_shortcuts_zoom_canvas_without_scaling_hud() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.settings.appearance.reduced_motion = true;
        app.reset(Document::new(64, 64, false), None, false);
        frame(&mut app, &ctx, vec![]);
        let initial = app.zoom;
        let hud = ctx.zoom_factor();
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        };
        frame(&mut app, &ctx, vec![key(egui::Key::Plus)]);
        frame(&mut app, &ctx, vec![]);
        assert!((app.zoom - initial * 1.25).abs() < 0.001);
        assert_eq!(ctx.zoom_factor(), hud);
        frame(&mut app, &ctx, vec![key(egui::Key::Minus)]);
        frame(&mut app, &ctx, vec![]);
        assert!((app.zoom - initial).abs() < 0.001);
        frame(&mut app, &ctx, vec![key(egui::Key::Equals)]);
        assert!(app.zoom > initial);
        frame(&mut app, &ctx, vec![key(egui::Key::Num0)]);
        frame(&mut app, &ctx, vec![]);
        assert_eq!(app.zoom, 1.0);
        assert_eq!(ctx.zoom_factor(), hud);
    }
    #[test]
    fn wheel_with_control_zooms_at_cursor_without_scaling_hud() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(64, 64, false), None, false);
        let p = Pos2::new(150.0, 170.0);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
        let initial = app.zoom;
        let center = Pos2::new(200.0, 200.0);
        let before = (p - center - app.pan) / app.zoom;
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(0.0, 1.0),
                modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            }],
        );
        assert!(app.zoom > initial);
        let after = (p - center - app.pan) / app.zoom;
        assert!((before - after).length() < 0.001);
        frame(&mut app, &ctx, vec![]);
        assert_eq!(ctx.zoom_factor(), 1.0);
    }
    #[test]
    fn repeated_ctrl_z_and_ctrl_shift_z_roundtrip_separate_strokes() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(64, 64, false), None, false);
        app.size = 3.0;
        app.pressure_enabled = false;
        frame(&mut app, &ctx, vec![]);
        for i in 0..8 {
            let p = Pos2::new(160.0, 160.0 + i as f32 * 9.0);
            frame(&mut app, &ctx, vec![egui::Event::PointerMoved(p)]);
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
            );
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(p + Vec2::new(60.0, 0.0))],
            );
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: p + Vec2::new(60.0, 0.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
        }
        app.history.commit(&app.doc);
        assert_eq!(app.history.counts(), (8, 0));
        let painted = app.doc.layers[0].pixels.clone();
        let press = |app: &mut PaintAtlas, redo| {
            let modifiers = if redo {
                egui::Modifiers::CTRL | egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::CTRL | egui::Modifiers::COMMAND
            };
            for pressed in [true, false] {
                let _ = ctx.run(
                    egui::RawInput {
                        modifiers,
                        events: vec![egui::Event::Key {
                            key: egui::Key::Z,
                            physical_key: Some(egui::Key::Z),
                            pressed,
                            repeat: false,
                            modifiers,
                        }],
                        ..Default::default()
                    },
                    |ctx| app.shortcuts(ctx),
                );
            }
        };
        for _ in 0..3 {
            for _ in 0..8 {
                press(&mut app, false);
            }
            assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
            assert_eq!(app.history.counts(), (0, 8));
            for _ in 0..8 {
                press(&mut app, true);
            }
            assert_eq!(app.doc.layers[0].pixels, painted);
            assert_eq!(app.history.counts(), (8, 0));
        }
    }
    #[test]
    fn distant_brush_marks_upload_local_pixels_with_identical_colors() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(3000, 3000, false), None, false);
        let _ = ctx.run(
            egui::RawInput {
                max_texture_side: Some(4096),
                ..Default::default()
            },
            |ctx| app.sync_layer_textures(ctx),
        );
        let id = app.textures[0].id();
        let brush = app.brush();
        app.doc.stamp([40.0, 40.0], brush.clone(), false);
        app.doc.stamp([2900.0, 2900.0], brush, false);
        app.changed();
        let output = ctx.run(
            egui::RawInput {
                max_texture_side: Some(4096),
                ..Default::default()
            },
            |ctx| app.sync_layer_textures(ctx),
        );
        let mut uploaded = 0;
        for (texture, delta) in output.textures_delta.set {
            if texture != id {
                continue;
            }
            let pos = delta
                .pos
                .expect("painting must preserve the existing texture");
            let egui::ImageData::Color(image) = delta.image;
            uploaded += image.size[0] * image.size[1];
            for y in 0..image.size[1] {
                for x in 0..image.size[0] {
                    let offset = ((pos[1] + y) * 3000 + pos[0] + x) * 4;
                    let p = &app.doc.layers[0].pixels[offset..offset + 4];
                    assert_eq!(
                        image.pixels[y * image.size[0] + x],
                        Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
                    );
                }
            }
        }
        assert!(uploaded > 0 && uploaded < 128 * 128 * 8);
    }
    #[test]
    fn canvas_pointer_stroke_and_history() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(64, 64, false), None, false);
        app.size = 4.0;
        let a = Pos2::new(180.0, 180.0);
        let b = Pos2::new(220.0, 220.0);
        frame(&mut app, &ctx, vec![]);
        let texture_id = app.textures[0].id();
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(a)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: a,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(b)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: b,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(
            app.doc.layers[0]
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 0)
                .count()
                > 20
        );
        assert_eq!(
            app.textures[0].id(),
            texture_id,
            "stroke must update the existing GPU texture"
        );
        assert!(app.doc.damage.is_none());
        assert!(app.gesture.is_none());
        assert!(app.dirty);
        let painted = app.doc.composite();
        app.undo();
        assert!(app.doc.composite().iter().all(|&b| b == 0));
        app.redo();
        assert_eq!(app.doc.composite(), painted);
    }
    #[test]
    fn fit_animation_smoothly_centers_zoomed_canvas_and_honors_reduced_motion() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.zoom = 8.0;
        app.pan = Vec2::new(800.0, -600.0);
        app.fit_animation = Some(0.7);
        app.animate_fit(1.0 / 60.0);
        assert!(app.zoom > 0.7 && app.zoom < 8.0);
        assert!(app.pan.length() > 0.0 && app.pan.length() < 1000.0);
        for _ in 0..120 {
            app.animate_fit(1.0 / 60.0);
        }
        assert_eq!(app.zoom, 0.7);
        assert_eq!(app.pan, Vec2::ZERO);
        assert!(app.fit_animation.is_none());
        app.pan = Vec2::splat(100.0);
        app.fit_animation = Some(2.0);
        app.settings.appearance.reduced_motion = true;
        app.animate_fit(1.0 / 60.0);
        assert_eq!(app.zoom, 2.0);
        assert_eq!(app.pan, Vec2::ZERO);
    }
    #[test]
    fn ctrl_t_without_selection_transforms_content_bounds_and_cancel_restores_no_selection() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(16, 16, false), None, false);
        app.settings.shortcuts = Default::default();
        app.doc.layers[0].pixels[(3 * 16 + 2) * 4..][..4].copy_from_slice(&[200, 50, 100, 180]);
        app.doc.layers[0].pixels[(7 * 16 + 9) * 4..][..4].copy_from_slice(&[20, 120, 200, 255]);
        let original = app.doc.layers[0].pixels.clone();
        let trigger = |app: &mut PaintAtlas| {
            let _ = ctx.run(
                egui::RawInput {
                    modifiers: egui::Modifiers::CTRL,
                    events: vec![egui::Event::Key {
                        key: egui::Key::T,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::CTRL,
                    }],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
        };
        trigger(&mut app);
        let floating = app.selection.floating.as_ref().unwrap();
        assert_eq!(
            floating.bounds,
            Rect::from_min_max(Pos2::new(2.0, 3.0), Pos2::new(10.0, 8.0))
        );
        app.cancel_transform();
        assert!(app.selection.mask.is_none() && app.doc.selection.is_none());
        assert_eq!(app.doc.layers[0].pixels, original);
        trigger(&mut app);
        app.selection.floating.as_mut().unwrap().bounds = app
            .selection
            .floating
            .as_ref()
            .unwrap()
            .bounds
            .translate(Vec2::new(1.0, 0.0));
        app.apply_transform();
        app.history.commit(&app.doc);
        assert!(app.selection.mask.is_none());
        assert_eq!(
            &app.doc.layers[0].pixels[(3 * 16 + 3) * 4..][..4],
            &[200, 50, 100, 180]
        );
        app.undo();
        assert_eq!(app.doc.layers[0].pixels, original);
        app.redo();
        assert_ne!(app.doc.layers[0].pixels, original);
    }
    #[test]
    fn free_transform_keeps_explicit_selection_and_rejects_empty_layers() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(16, 16, false), None, false);
        app.begin_transform(&ctx, false);
        assert!(app.selection.floating.is_none() && app.selection.mask.is_none());
        app.doc.layers[0].pixels[(3 * 16 + 3) * 4 + 3] = 255;
        let mask = std::sync::Arc::new(
            crate::selection::Mask::rectangle(
                16,
                16,
                Rect::from_min_size(Pos2::new(1.0, 1.0), Vec2::splat(5.0)),
            )
            .unwrap(),
        );
        app.selection.mask = Some(mask.clone());
        app.begin_transform(&ctx, false);
        assert_eq!(app.selection.floating.as_ref().unwrap().bounds, mask.bounds);
        app.cancel_transform();
        assert!(std::sync::Arc::ptr_eq(
            app.selection.mask.as_ref().unwrap(),
            &mask
        ));
    }
    #[test]
    fn whole_layer_transform_keeps_transparency_lock_and_unchanged_apply_has_no_history_step() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(8, 8, false), None, false);
        app.doc.layers[0].pixels[(2 * 8 + 2) * 4 + 3] = 255;
        app.doc.layers[0].alpha_lock = true;
        app.begin_transform(&ctx, false);
        assert!(app.selection.floating.is_some());
        app.apply_transform();
        app.history.commit(&app.doc);
        assert!(app.selection.mask.is_none() && app.doc.selection.is_none());
        assert!(app.doc.layers[0].alpha_lock);
        assert_eq!(app.history.counts(), (0, 0));
    }
}
