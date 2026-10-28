//! Registro único dos atalhos de teclado, incluindo modificadores mantidos.
// =============================================================================
// ATENÇÃO OBRIGATÓRIA PARA NOVAS FUNÇÕES COM ATALHO:
// Cadastre a ação e seus padrões em `actions!` abaixo e use `consume`/`held`.
// NUNCA adicione key_pressed/consume_key/key_down fixos fora deste módulo.
// Atualize menus/dicas com `label` e teste configuração, conflito e persistência.
// Todo novo atalho DEVE aparecer em Configurações > Atalhos.
// =============================================================================
use eframe::egui::{self, Key, Modifiers};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

macro_rules! actions {
    ($( $action:ident, $category:literal, $label:literal, $scope:expr, $held:literal, [$($default:literal),*]; )*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Action { $($action),* }
        impl Action {
            pub const ALL: &'static [Self] = &[$(Self::$action),*];
            pub fn id(self) -> &'static str { match self { $(Self::$action => stringify!($action)),* } }
            pub fn label(self) -> &'static str { match self { $(Self::$action => $label),* } }
            pub fn category(self) -> &'static str { match self { $(Self::$action => $category),* } }
            pub fn scope(self) -> u16 { match self { $(Self::$action => $scope),* } }
            pub fn is_held(self) -> bool { match self { $(Self::$action => $held),* } }
            fn make_defaults(self) -> Vec<Binding> { match self { $(Self::$action => vec![$(Binding::parse($default)),*]),* } }
        }
    }
}
// Contexts: normal editor=1, painting=2, selection=4, transform=8, zoom=16,
// layers=32, rulers=64, QuickShape=128, clone source=256.
actions! {
    New, "File", "New Canvas", 1, false, ["Ctrl+N"];
    Open, "File", "Open Project or Image", 1, false, ["Ctrl+O"];
    Save, "File", "Save Project", 1, false, ["Ctrl+S"];
    SaveAs, "File", "Save Project As", 1, false, [];
    ExportAs, "File", "Export As", 1, false, ["Ctrl+Alt+Shift+E"];
    Export, "File", "Quick Export PNG", 1, false, ["Ctrl+E"];
    CloseTab, "File", "Close Current Canvas", 1, false, ["Ctrl+W"];
    CloseAll, "File", "Close All Canvases", 1, false, [];
    Settings, "File", "Open settings", 1, false, [];
    FilterGallery, "Filters", "Filter Gallery", 1, false, [];
    RepeatFilter, "Filters", "Repeat last filter", 1, false, ["Ctrl+Alt+F"];
    Average, "Filters", "Blur: Average", 1, false, [];
    Blur, "Filters", "Blur: Blur", 1, false, [];
    BlurMore, "Filters", "Blur: Blur More", 1, false, [];
    BoxBlur, "Filters", "Blur: Box Blur", 1, false, [];
    GaussianBlur, "Filters", "Blur: Gaussian Blur", 1, false, [];
    MotionBlur, "Filters", "Blur: Motion Blur", 1, false, [];
    RadialBlur, "Filters", "Blur: Radial Blur", 1, false, [];
    ZoomBlur, "Filters", "Blur: Zoom Blur", 1, false, [];
    ShapeBlur, "Filters", "Blur: Shape Blur", 1, false, [];
    SurfaceBlur, "Filters", "Blur: Surface Blur", 1, false, [];
    IrisBlur, "Filters", "Blur Gallery: Iris Blur", 1, false, [];
    TiltShift, "Filters", "Blur Gallery: Tilt-Shift", 1, false, [];
    AddNoise, "Filters", "Noise: Add Noise", 1, false, [];
    Despeckle, "Filters", "Noise: Despeckle", 1, false, [];
    DustScratches, "Filters", "Noise: Dust & Scratches", 1, false, [];
    Median, "Filters", "Noise: Median", 1, false, [];
    ReduceNoise, "Filters", "Noise: Reduce Noise", 1, false, [];
    Displace, "Filters", "Distort: Displace", 1, false, [];
    Pinch, "Filters", "Distort: Pinch", 1, false, [];
    Polar, "Filters", "Distort: Polar Coordinates", 1, false, [];
    Ripple, "Filters", "Distort: Ripple", 1, false, [];
    Shear, "Filters", "Distort: Shear", 1, false, [];
    Spherize, "Filters", "Distort: Spherize", 1, false, [];
    Twirl, "Filters", "Distort: Twirl", 1, false, [];
    Wave, "Filters", "Distort: Wave", 1, false, [];
    ZigZag, "Filters", "Distort: ZigZag", 1, false, [];
    Glass, "Filters", "Distort: Glass", 1, false, [];
    ColorHalftone, "Filters", "Pixelate: Color Halftone", 1, false, [];
    Crystallize, "Filters", "Pixelate: Crystallize", 1, false, [];
    Facet, "Filters", "Pixelate: Facet", 1, false, [];
    Fragment, "Filters", "Pixelate: Fragment", 1, false, [];
    Mezzotint, "Filters", "Pixelate: Mezzotint", 1, false, [];
    Mosaic, "Filters", "Pixelate: Mosaic", 1, false, [];
    Pointillize, "Filters", "Pixelate: Pointillize", 1, false, [];
    Clouds, "Filters", "Render: Clouds", 1, false, [];
    DifferenceClouds, "Filters", "Render: Difference Clouds", 1, false, [];
    Fibers, "Filters", "Render: Fibers", 1, false, [];
    LensFlare, "Filters", "Render: Lens Flare", 1, false, [];
    Sharpen, "Filters", "Sharpen: Sharpen", 1, false, [];
    SharpenMore, "Filters", "Sharpen: Sharpen More", 1, false, [];
    SharpenEdges, "Filters", "Sharpen: Sharpen Edges", 1, false, [];
    AdaptiveSharpen, "Filters", "Sharpen: Adaptive Sharpen", 1, false, [];
    UnsharpMask, "Filters", "Sharpen: Unsharp Mask", 1, false, [];
    Diffuse, "Filters", "Stylize: Diffuse", 1, false, [];
    Emboss, "Filters", "Stylize: Emboss", 1, false, [];
    FindEdges, "Filters", "Stylize: Find Edges", 1, false, [];
    OilPaint, "Filters", "Stylize: Oil Paint", 1, false, [];
    Solarize, "Filters", "Stylize: Solarize", 1, false, [];
    TraceContour, "Filters", "Stylize: Trace Contour", 1, false, [];
    Wind, "Filters", "Stylize: Wind", 1, false, [];
    ColoredPencil, "Filters", "Artistic: Colored Pencil", 1, false, [];
    DryBrush, "Filters", "Artistic: Dry Brush", 1, false, [];
    FilmGrain, "Filters", "Artistic: Film Grain", 1, false, [];
    NeonGlow, "Filters", "Artistic: Neon Glow", 1, false, [];
    PosterEdges, "Filters", "Artistic: Poster Edges", 1, false, [];
    Watercolor, "Filters", "Artistic: Watercolor", 1, false, [];
    Crosshatch, "Filters", "Brush Strokes: Crosshatch", 1, false, [];
    InkOutlines, "Filters", "Brush Strokes: Ink Outlines", 1, false, [];
    GraphicPen, "Filters", "Sketch: Graphic Pen", 1, false, [];
    Photocopy, "Filters", "Sketch: Photocopy", 1, false, [];
    Stamp, "Filters", "Sketch: Stamp", 1, false, [];
    HalftonePattern, "Filters", "Sketch: Halftone Pattern", 1, false, [];
    Grain, "Filters", "Texture: Grain", 1, false, [];
    MosaicTiles, "Filters", "Texture: Mosaic Tiles", 1, false, [];
    Patchwork, "Filters", "Texture: Patchwork", 1, false, [];
    StainedGlass, "Filters", "Texture: Stained Glass", 1, false, [];
    Texturizer, "Filters", "Texture: Texturizer", 1, false, [];
    Deinterlace, "Filters", "Video: De-Interlace", 1, false, [];
    Custom, "Filters", "Other: Custom (3 × 3)", 1, false, [];
    HighPass, "Filters", "Other: High Pass", 1, false, [];
    Maximum, "Filters", "Other: Maximum", 1, false, [];
    Minimum, "Filters", "Other: Minimum", 1, false, [];
    Offset, "Filters", "Other: Offset", 1, false, [];
    ImageBrightness, "Image", "Brightness / Contrast", 1, false, [];
    ImageLevels, "Image", "Levels", 1, false, ["Ctrl+L"];
    ImageCurves, "Image", "Curves", 1, false, ["Ctrl+M"];
    ImageExposure, "Image", "Exposure", 1, false, [];
    ImageVibrance, "Image", "Vibrance", 1, false, [];
    ImageHue, "Image", "Hue / Saturation", 1, false, ["Ctrl+U"];
    ImageBalance, "Image", "Color Balance", 1, false, ["Ctrl+B"];
    ImageBlackWhite, "Image", "Black & White", 1, false, ["Ctrl+Alt+Shift+B"];
    ImagePhotoFilter, "Image", "Photo Filter", 1, false, [];
    ImageChannelMixer, "Image", "Channel Mixer", 1, false, [];
    ImageInvert, "Image", "Invert Colors", 1, false, ["Ctrl+I"];
    ImagePosterize, "Image", "Posterize", 1, false, [];
    ImageThreshold, "Image", "Threshold", 1, false, [];
    ImageGradientMap, "Image", "Gradient Map", 1, false, [];
    ImageShadows, "Image", "Shadows / Highlights", 1, false, [];
    ImageDesaturate, "Image", "Desaturate", 1, false, ["Ctrl+Shift+U"];
    ImageReplaceColor, "Image", "Replace Color", 1, false, [];
    ImageEqualize, "Image", "Equalize", 1, false, [];
    ImageAutoTone, "Image", "Auto Tone", 1, false, ["Ctrl+Shift+L"];
    ImageAutoContrast, "Image", "Auto Contrast", 1, false, ["Ctrl+Alt+Shift+L"];
    ImageAutoColor, "Image", "Auto Color", 1, false, ["Ctrl+Shift+B"];
    ImageSize, "Image", "Image Size", 1, false, ["Ctrl+Alt+I"];
    ImageCanvasSize, "Image", "Canvas Size", 1, false, ["Ctrl+Alt+C"];
    ImageCrop, "Image", "Crop image to selection", 1, false, [];
    ImageTrim, "Image", "Trim", 1, false, [];
    ImageDuplicate, "Image", "Duplicate Image", 1, false, [];
    ImageRotate90, "Image", "Rotate Image 90° Clockwise", 1, false, [];
    ImageRotate180, "Image", "Rotate Image 180°", 1, false, [];
    ImageRotate270, "Image", "Rotate Image 90° Counterclockwise", 1, false, [];
    ImageRotate, "Image", "Arbitrary image rotation", 1, false, [];
    ImageFlipHorizontal, "Image", "Flip Image Horizontally", 1, false, [];
    ImageFlipVertical, "Image", "Flip Image Vertically", 1, false, [];
    Undo, "Edit", "Undo / cancel transform", 9, false, ["Ctrl+Z"];
    Redo, "Edit", "Redo", 1, false, ["Ctrl+Shift+Z", "Ctrl+Y"];
    Copy, "Edit", "Copy layer or selection", 1, false, ["Ctrl+C", "Copy"];
    CopyMerged, "Edit", "Copy merged", 1, false, ["Ctrl+Shift+C"];
    Paste, "Edit", "Paste image", 1, false, ["Ctrl+V", "Paste"];
    Brush, "Tools", "Brush", 1, false, ["B"];
    Eraser, "Tools", "Eraser", 1, false, ["E"];
    Fill, "Tools", "Fill", 1, false, ["G"];
    Picker, "Tools", "Eyedropper", 1, false, ["I"];
    Line, "Tools", "Line", 1, false, ["L"];
    Rectangle, "Tools", "Rectangle", 1, false, ["R"];
    Ellipse, "Tools", "Ellipse", 1, false, ["O"];
    ZoomTool, "Tools", "Zoom", 1, false, ["Z"];
    ZoomOutModifier, "Canvas", "Zoom out when clicking with the Zoom tool", 16, true, ["Alt"];
    Hand, "Tools", "Pan canvas", 1, false, ["H"];
    SelectRect, "Tools", "Rectangular selection", 1, false, ["S"];
    Lasso, "Tools", "Lasso", 1, false, ["A"];
    LiquifyTool, "Tools", "Liquify", 1, false, [];
    CloneTool, "Tools", "Clone", 1, false, [];
    CloneSource, "Clone", "Set clone source", 256, true, ["Alt"];
    SmallerBrush, "Brush", "Decrease size", 1, false, ["OpenBracket"];
    LargerBrush, "Brush", "Increase size", 1, false, ["CloseBracket"];
    BrushSizes, "Brush", "Open brush sizes", 1, false, ["F6"];
    RotateLeft, "Canvas", "Rotate view −15°", 1, false, ["Q"];
    RotateRight, "Canvas", "Rotate view +15°", 1, false, ["W"];
    Mirror, "Canvas", "Mirror view", 1, false, ["M"];
    Fit, "Canvas", "Fit canvas to window", 1, false, ["F"];
    ZoomIn, "Canvas", "Zoom in", 9, false, ["Ctrl+Plus", "Ctrl+Equals", "Ctrl+Shift+Plus", "Ctrl+Shift+Equals"];
    ZoomOut, "Canvas", "Zoom out", 9, false, ["Ctrl+Minus", "Ctrl+Shift+Minus"];
    ResetZoom, "Canvas", "100% zoom", 9, false, ["Ctrl+0"];
    QuickShapeRegular, "QuickShape", "Regular shapes / 15° rotation", 128, true, ["Shift"];
    QuickShapeApply, "QuickShape", "Apply shape", 128, false, ["Enter"];
    QuickShapeCancel, "QuickShape", "Restore original stroke", 128, false, ["Escape"];
    QuickShapeToggle, "QuickShape", "Enable / disable QuickShape", 1, false, [];
    RulerTool, "Rulers", "Create / edit rulers", 1, false, ["U"];
    SnapRulers, "Rulers", "Enable snapping to rulers", 1, false, ["Ctrl+2"];
    ShowRulers, "Rulers", "Show / hide rulers", 1, false, ["Ctrl+Shift+R"];
    RulerAngleSnap, "Rulers", "15° angles when editing rulers", 64, true, ["Shift"];
    TemporaryZoom, "Canvas", "Temporary zoom: drag to zoom in/out", 15, true, ["Ctrl+Space"];
    Pan, "Canvas", "Pan view while held", 15, true, ["Space"];
    BrushHud, "Brush", "HUD: drag for size / opacity", 17, true, ["Ctrl+Alt"];
    TemporaryPicker, "Brush", "Temporary eyedropper while held", 2, true, ["Alt"];
    SelectAll, "Selection", "Select all", 1, false, ["Ctrl+A"];
    Deselect, "Selection", "Deselect", 1, false, ["Ctrl+D"];
    Transform, "Selection", "Move / transform content", 1, false, ["Ctrl+T"];
    DeleteSelection, "Selection", "Delete selected content", 1, false, ["Delete"];
    Cancel, "Selection", "Cancel selection / transform", 13, false, ["Escape"];
    Apply, "Transform", "Apply transform", 8, false, ["Enter"];
    MoveLeft, "Transform", "Move 1 px left", 8, false, ["ArrowLeft"];
    MoveRight, "Transform", "Move 1 px right", 8, false, ["ArrowRight"];
    MoveUp, "Transform", "Move 1 px up", 8, false, ["ArrowUp"];
    MoveDown, "Transform", "Move 1 px down", 8, false, ["ArrowDown"];
    FastLeft, "Transform", "Move 10 px left", 8, false, ["Shift+ArrowLeft"];
    FastRight, "Transform", "Move 10 px right", 8, false, ["Shift+ArrowRight"];
    FastUp, "Transform", "Move 10 px up", 8, false, ["Shift+ArrowUp"];
    FastDown, "Transform", "Move 10 px down", 8, false, ["Shift+ArrowDown"];
    LayerToggleSelection, "Layers", "Toggle layer selection", 32, true, ["Ctrl"];
    LayerRangeSelection, "Layers", "Select a range of layers", 32, true, ["Shift"];
    LayerGroup, "Layers", "Group selected layers", 1, false, ["Ctrl+G"];
    LayerMerge, "Layers", "Merge selected layers", 1, false, ["Ctrl+Shift+E"];
    SelectionAdd, "Selection", "Add to selection while held", 4, true, ["Shift"];
    SelectionSubtract, "Selection", "Subtract from selection while held", 4, true, ["Alt"];
    Proportion, "Transform", "Toggle aspect ratio while held", 8, true, ["Shift"];
    ScaleFromCenter, "Transform", "Scale from center while held", 8, true, ["Alt"];
    ConstrainMove, "Transform", "Constrain movement to 45° while held", 8, true, ["Shift"];
    RotationSnap, "Transform", "Snap rotation to 15° while held", 8, true, ["Shift"];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub key: Option<String>,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}
impl Binding {
    fn parse(text: &str) -> Self {
        let mut b = Self::from_modifiers(Modifiers::NONE);
        for part in text.split('+') {
            match part {
                "Ctrl" => b.ctrl = true,
                "Shift" => b.shift = true,
                "Alt" => b.alt = true,
                "Super" => b.super_key = true,
                key => {
                    b.key = Some(
                        Key::from_name(key)
                            .expect("invalid shortcut default")
                            .name()
                            .into(),
                    )
                }
            }
        }
        b
    }
    pub fn from_modifiers(m: Modifiers) -> Self {
        Self {
            key: None,
            ctrl: m.ctrl || (m.command && !m.mac_cmd),
            shift: m.shift,
            alt: m.alt,
            super_key: m.mac_cmd,
        }
    }
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.super_key {
            parts.push("Super");
        }
        if let Some(key) = &self.key {
            parts.push(key);
        }
        parts.join(" + ")
    }
    fn valid(&self, held: bool) -> bool {
        self.key
            .as_ref()
            .is_none_or(|k| Key::from_name(k).is_some())
            && (self.key.is_some()
                || held && (self.ctrl || self.alt || self.shift || self.super_key))
    }
    fn matches(&self, key: Key, physical: Option<Key>, mods: Modifiers, action: Action) -> bool {
        let Some(expected) = self.key.as_deref().and_then(Key::from_name) else {
            return false;
        };
        let actual = Self::from_modifiers(mods);
        // Some tablet dials report logical '+' for both directions. The physical
        // arithmetic key remains authoritative, as in the original tablet fix.
        let key = if matches!(action, Action::ZoomIn | Action::ZoomOut)
            && matches!(expected, Key::Plus | Key::Minus | Key::Equals)
        {
            if key == Key::Minus || physical == Some(Key::Minus) {
                Key::Minus
            } else {
                physical
                    .filter(|k| matches!(k, Key::Plus | Key::Equals))
                    .unwrap_or(key)
            }
        } else {
            key
        };
        expected == key
            && self.ctrl == actual.ctrl
            && self.alt == actual.alt
            && self.shift == actual.shift
            && self.super_key == actual.super_key
    }
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Shortcuts(pub BTreeMap<String, Vec<Binding>>);
impl Shortcuts {
    pub fn bindings(&self, action: Action) -> &[Binding] {
        static DEFAULTS: std::sync::OnceLock<BTreeMap<&'static str, Vec<Binding>>> =
            std::sync::OnceLock::new();
        let defaults = DEFAULTS.get_or_init(|| {
            Action::ALL
                .iter()
                .map(|a| (a.id(), a.make_defaults()))
                .collect()
        });
        self.0
            .get(action.id())
            .map(Vec::as_slice)
            .unwrap_or(&defaults[action.id()])
    }
    pub fn label(&self, action: Action) -> String {
        let bindings = self.bindings(action);
        if bindings.is_empty() {
            "No shortcut".into()
        } else {
            bindings
                .iter()
                .map(Binding::label)
                .collect::<Vec<_>>()
                .join(" / ")
        }
    }
    pub fn menu(&self, action: Action, text: &str) -> String {
        if self.bindings(action).is_empty() {
            text.into()
        } else {
            format!("{text}    {}", self.label(action))
        }
    }
    /// Normalize a tablet's physical +/- before ordinary commands can consume
    /// its misleading logical symbol. Custom zoom bindings remain authoritative.
    pub fn normalize_zoom_events(&self, events: &mut [egui::Event]) {
        for event in events {
            if let egui::Event::Key {
                key,
                physical_key: Some(physical),
                modifiers,
                ..
            } = event
                && matches!(physical, Key::Minus | Key::Plus | Key::Equals)
            {
                let normalized = if *key == Key::Minus || *physical == Key::Minus {
                    Key::Minus
                } else {
                    *physical
                };
                let action = if normalized == Key::Minus {
                    Action::ZoomOut
                } else {
                    Action::ZoomIn
                };
                if self.bindings(action).iter().any(|b| {
                    b.key.as_deref().and_then(Key::from_name) == Some(normalized)
                        && b.matches(normalized, Some(normalized), *modifiers, action)
                }) {
                    *key = normalized;
                }
            }
        }
    }
    pub fn consume(&self, input: &mut egui::InputState, action: Action) -> bool {
        self.count(input, action) > 0
    }
    pub fn count(&self, input: &mut egui::InputState, action: Action) -> usize {
        let mut count = 0;
        let bindings = self.bindings(action);
        let mods = input.modifiers;
        input.events.retain(|event| {
            let event = key_event(event, mods);
            if let Some((key, physical, modifiers, pressed)) = event
                && pressed
                && bindings
                    .iter()
                    .any(|b| b.matches(key, physical, modifiers, action))
            {
                count += 1;
                false
            } else {
                true
            }
        });
        count
    }
    pub fn held(&self, input: &egui::InputState, action: Action) -> bool {
        self.bindings(action).iter().any(|b| {
            let m = Binding::from_modifiers(input.modifiers);
            // Extra modifiers are allowed for held gestures, e.g. Shift while panning.
            (!b.ctrl || m.ctrl)
                && (!b.shift || m.shift)
                && (!b.alt || m.alt)
                && (!b.super_key || m.super_key)
                && b.key
                    .as_deref()
                    .and_then(Key::from_name)
                    .is_none_or(|key| input.key_down(key))
        })
    }
    pub fn conflicts(&self, action: Action, binding: &Binding) -> Vec<Action> {
        Action::ALL.iter().copied().filter(|other| *other != action
            && other.scope() & action.scope() != 0
            // These modifiers share Shift in mutually exclusive move/resize/rotate drags.
            && !(matches!(action, Action::Proportion | Action::RotationSnap | Action::ConstrainMove)
                && matches!(other, Action::Proportion | Action::RotationSnap | Action::ConstrainMove))
            && !matches!((action, *other),
                (Action::ScaleFromCenter, Action::ConstrainMove | Action::RotationSnap)
                | (Action::ConstrainMove | Action::RotationSnap, Action::ScaleFromCenter))
            && self.bindings(*other).contains(binding)).collect()
    }
    pub fn assign(&mut self, action: Action, slot: usize, binding: Binding, replace: bool) -> bool {
        if !binding.valid(action.is_held()) {
            return false;
        }
        let conflicts = self.conflicts(action, &binding);
        if !replace && !conflicts.is_empty() {
            return false;
        }
        for other in conflicts {
            let mut list = self.bindings(other).to_vec();
            list.retain(|b| b != &binding);
            self.0.insert(other.id().into(), list);
        }
        let mut list = self.bindings(action).to_vec();
        if slot < list.len() {
            list.remove(slot);
        }
        if !list.contains(&binding) {
            list.insert(slot.min(list.len()), binding);
        }
        self.0.insert(action.id().into(), list);
        true
    }
    pub fn restore(&mut self, action: Action) {
        let defaults = action.make_defaults();
        self.0.insert(action.id().into(), Vec::new());
        for (slot, binding) in defaults.into_iter().enumerate() {
            self.assign(action, slot, binding, true);
        }
        self.0.remove(action.id());
    }
    pub fn sanitize(&mut self) {
        for action in Action::ALL {
            if let Some(list) = self.0.get_mut(action.id()) {
                list.retain(|b| b.valid(action.is_held()));
                for binding in list.iter_mut() {
                    binding.key = binding
                        .key
                        .as_deref()
                        .and_then(Key::from_name)
                        .map(|k| k.name().into());
                }
                let mut seen = Vec::new();
                list.retain(|b| {
                    if seen.contains(b) {
                        false
                    } else {
                        seen.push(b.clone());
                        true
                    }
                });
            }
        }
    }
}
/// Raw native keys keep arbitrary remappings/held gestures possible. TextEdit
/// still receives semantic clipboard events; the drawing only needs one event.
pub fn deduplicate_clipboard_events(events: &mut Vec<egui::Event>) {
    let raw = [(Key::C, Key::Copy), (Key::V, Key::Paste), (Key::X, Key::Cut)]
        .map(|(letter, dedicated)| events.iter().any(|event| {
            matches!(event, egui::Event::Key { key, modifiers, pressed: true, .. }
                if *key == dedicated || *key == letter && (modifiers.ctrl || modifiers.command || modifiers.mac_cmd))
        }));
    events.retain(|event| match event {
        egui::Event::Copy => !raw[0],
        egui::Event::Paste(_) => !raw[1],
        egui::Event::Cut => !raw[2],
        _ => true,
    });
}
/// Native clipboard backends turn Ctrl+C/V/X into semantic events. Treat them
/// as bindings too, so remapping/disabling their defaults actually takes effect.
pub fn key_event(
    event: &egui::Event,
    mods: Modifiers,
) -> Option<(Key, Option<Key>, Modifiers, bool)> {
    let semantic_mods = if mods.ctrl || mods.command || mods.mac_cmd {
        mods
    } else {
        mods.plus(Modifiers::CTRL)
    };
    match event {
        egui::Event::Key {
            key,
            physical_key,
            modifiers,
            pressed,
            ..
        } => Some((*key, *physical_key, *modifiers, *pressed)),
        egui::Event::Copy => Some((Key::C, None, semantic_mods, true)),
        egui::Event::Cut => Some((Key::X, None, semantic_mods, true)),
        egui::Event::Paste(_) => Some((Key::V, None, semantic_mods, true)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    #[test]
    fn movement_constraint_can_be_rebound_without_changing_resize_or_rotation() {
        let ctx = egui::Context::default();
        let mut shortcuts = Shortcuts::default();
        assert!(shortcuts.assign(Action::ConstrainMove, 0, Binding::parse("Alt"), false));
        for modifiers in [Modifiers::SHIFT, Modifiers::ALT, Modifiers::NONE] {
            let _ = ctx.run(
                egui::RawInput {
                    modifiers,
                    ..Default::default()
                },
                |ctx| {
                    ctx.input(|input| {
                        assert_eq!(shortcuts.held(input, Action::ConstrainMove), modifiers.alt);
                        assert_eq!(shortcuts.held(input, Action::Proportion), modifiers.shift);
                        assert_eq!(shortcuts.held(input, Action::RotationSnap), modifiers.shift);
                    });
                },
            );
        }
    }
    #[test]
    fn physical_subtract_survives_non_arithmetic_logical_symbols_and_command_routing() {
        let s = Shortcuts::default();
        for key in [
            Key::Minus,
            Key::Plus,
            Key::Equals,
            Key::Slash,
            Key::S,
            Key::Num0,
        ] {
            for mods in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::SHIFT] {
                let mut events = vec![egui::Event::Key {
                    key,
                    physical_key: Some(Key::Minus),
                    pressed: true,
                    repeat: false,
                    modifiers: mods,
                }];
                assert!(s.bindings(Action::ZoomOut).iter().any(|b| b.matches(
                    key,
                    Some(Key::Minus),
                    mods,
                    Action::ZoomOut
                )));
                assert!(!s.bindings(Action::ZoomIn).iter().any(|b| b.matches(
                    key,
                    Some(Key::Minus),
                    mods,
                    Action::ZoomIn
                )));
                s.normalize_zoom_events(&mut events);
                assert!(matches!(
                    events[0],
                    egui::Event::Key {
                        key: Key::Minus,
                        ..
                    }
                ));
            }
        }
        let mut custom = s.clone();
        custom
            .0
            .insert(Action::ZoomOut.id().into(), vec![Binding::parse("Ctrl+J")]);
        assert!(custom.bindings(Action::ZoomOut)[0].matches(
            Key::J,
            Some(Key::Minus),
            Modifiers::CTRL,
            Action::ZoomOut
        ));
        assert!(!custom.bindings(Action::ZoomOut)[0].matches(
            Key::Minus,
            Some(Key::Minus),
            Modifiers::CTRL,
            Action::ZoomOut
        ));
        let mut events = vec![egui::Event::Key {
            key: Key::J,
            physical_key: Some(Key::Minus),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        }];
        custom.normalize_zoom_events(&mut events);
        assert!(matches!(events[0], egui::Event::Key { key: Key::J, .. }));
    }
    #[test]
    fn raw_and_semantic_clipboard_pair_executes_only_once() {
        let mut events = vec![
            event(Key::V, Modifiers::CTRL),
            egui::Event::Paste(String::new()),
        ];
        deduplicate_clipboard_events(&mut events);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], egui::Event::Key { key: Key::V, .. }));
    }
    #[test]
    fn catalog_defaults_are_valid_without_accidental_context_conflicts() {
        let shortcuts = Shortcuts::default();
        for action in Action::ALL {
            for b in shortcuts.bindings(*action) {
                assert!(b.valid(action.is_held()), "{}", action.id());
                assert!(
                    shortcuts.conflicts(*action, b).is_empty(),
                    "{}",
                    action.id()
                );
            }
        }
    }
    #[test]
    fn remapping_disabling_and_exact_modifiers_preserve_other_actions() {
        let mut shortcuts = Shortcuts::default();
        assert!(shortcuts.assign(Action::Brush, 0, Binding::parse("J"), false));
        assert!(!shortcuts.bindings(Action::Brush).iter().any(|b| b.matches(
            Key::B,
            None,
            Modifiers::NONE,
            Action::Brush
        )));
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    event(Key::J, Modifiers::NONE),
                    event(Key::J, Modifiers::CTRL),
                ],
                ..Default::default()
            },
            |ctx| {
                ctx.input_mut(|input| {
                    assert_eq!(shortcuts.count(input, Action::Brush), 1);
                    assert_eq!(
                        input
                            .events
                            .iter()
                            .filter(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
                            .count(),
                        1
                    );
                });
            },
        );
        shortcuts.0.insert(Action::Brush.id().into(), Vec::new());
        assert!(shortcuts.bindings(Action::Brush).is_empty());
        shortcuts.restore(Action::Brush);
        assert_eq!(
            shortcuts.bindings(Action::Brush),
            Action::Brush.make_defaults()
        );
    }
    #[test]
    fn conflicts_require_explicit_replacement_and_restore_resolves_conflicts() {
        let mut s = Shortcuts::default();
        let eraser = Binding::parse("E");
        assert_eq!(s.conflicts(Action::Brush, &eraser), vec![Action::Eraser]);
        assert!(!s.assign(Action::Brush, 0, eraser.clone(), false));
        assert!(s.assign(Action::Brush, 0, eraser, true));
        assert!(s.bindings(Action::Eraser).is_empty());
        s.restore(Action::Eraser);
        assert!(s.bindings(Action::Brush).is_empty());
        assert_eq!(s.label(Action::Eraser), "E");
    }
    #[test]
    fn clipboard_events_can_be_reassigned_to_any_action_and_defaults_disabled() {
        let ctx = egui::Context::default();
        let mut s = Shortcuts::default();
        assert!(s.assign(Action::Save, 0, Binding::parse("Ctrl+V"), true));
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Paste(String::new())],
                modifiers: Modifiers::CTRL,
                ..Default::default()
            },
            |ctx| {
                ctx.input_mut(|input| {
                    assert!(!s.consume(input, Action::Paste));
                    assert!(s.consume(input, Action::Save));
                });
            },
        );
    }
    #[test]
    fn custom_held_key_replaces_space_and_keeps_other_modifiers_usable() {
        let ctx = egui::Context::default();
        let mut s = Shortcuts::default();
        assert!(s.assign(Action::Pan, 0, Binding::parse("J"), false));
        let _ = ctx.run(
            egui::RawInput {
                events: vec![event(Key::Space, Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| {
                assert!(!ctx.input(|i| s.held(i, Action::Pan)));
            },
        );
        let _ = ctx.run(
            egui::RawInput {
                events: vec![event(Key::J, Modifiers::SHIFT)],
                modifiers: Modifiers::SHIFT,
                ..Default::default()
            },
            |ctx| {
                assert!(ctx.input(|i| s.held(i, Action::Pan)));
            },
        );
    }
    #[test]
    fn configuration_roundtrips_and_legacy_settings_receive_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = crate::presets::Settings::default();
        settings
            .shortcuts
            .assign(Action::Save, 0, Binding::parse("Ctrl+Shift+S"), false);
        settings
            .shortcuts
            .0
            .insert(Action::Undo.id().into(), Vec::new());
        settings.save(&path).unwrap();
        let loaded = crate::presets::Settings::load(&path).unwrap();
        assert_eq!(loaded.shortcuts, settings.shortcuts);
        let mut json = serde_json::to_value(&settings).unwrap();
        json.as_object_mut().unwrap().remove("shortcuts");
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let legacy = crate::presets::Settings::load(&path).unwrap();
        assert_eq!(legacy.shortcuts.label(Action::Save), "Ctrl + S");
        assert!(!legacy.shortcuts.bindings(Action::Undo).is_empty());
    }
}
