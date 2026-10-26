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
            pub fn scope(self) -> u8 { match self { $(Self::$action => $scope),* } }
            pub fn is_held(self) -> bool { match self { $(Self::$action => $held),* } }
            fn make_defaults(self) -> Vec<Binding> { match self { $(Self::$action => vec![$(Binding::parse($default)),*]),* } }
        }
    }
}
// Contexts: normal editor=1, painting=2, creating selection=4, transform=8.
actions! {
    New, "Arquivo", "Novo canvas", 1, false, ["Ctrl+N"];
    Open, "Arquivo", "Abrir projeto ou imagem", 1, false, ["Ctrl+O"];
    Save, "Arquivo", "Salvar projeto", 1, false, ["Ctrl+S"];
    SaveAs, "Arquivo", "Salvar projeto como", 1, false, [];
    ExportAs, "Arquivo", "Exportar como", 1, false, ["Ctrl+Alt+Shift+E"];
    Export, "Arquivo", "Exportar PNG rápido", 1, false, ["Ctrl+E"];
    CloseTab, "Arquivo", "Fechar canvas atual", 1, false, ["Ctrl+W"];
    CloseAll, "Arquivo", "Fechar todos os canvases", 1, false, [];
    Settings, "Arquivo", "Abrir configurações", 1, false, [];
    FilterGallery, "Filtros", "Galeria de filtros", 1, false, [];
    RepeatFilter, "Filtros", "Repetir último filtro", 1, false, ["Ctrl+Alt+F"];
    Average, "Filtros", "Desfoque: Média", 1, false, [];
    Blur, "Filtros", "Desfoque: Desfoque", 1, false, [];
    BlurMore, "Filtros", "Desfoque: Desfocar mais", 1, false, [];
    BoxBlur, "Filtros", "Desfoque: Desfoque de caixa", 1, false, [];
    GaussianBlur, "Filtros", "Desfoque: Desfoque gaussiano", 1, false, [];
    MotionBlur, "Filtros", "Desfoque: Desfoque de movimento", 1, false, [];
    RadialBlur, "Filtros", "Desfoque: Desfoque radial", 1, false, [];
    ZoomBlur, "Filtros", "Desfoque: Desfoque de zoom", 1, false, [];
    ShapeBlur, "Filtros", "Desfoque: Desfoque de forma", 1, false, [];
    SurfaceBlur, "Filtros", "Desfoque: Desfoque de superfície", 1, false, [];
    IrisBlur, "Filtros", "Galeria de desfoque: Desfoque de íris", 1, false, [];
    TiltShift, "Filtros", "Galeria de desfoque: Inclinação / deslocamento", 1, false, [];
    AddNoise, "Filtros", "Ruído: Adicionar ruído", 1, false, [];
    Despeckle, "Filtros", "Ruído: Remover manchas", 1, false, [];
    DustScratches, "Filtros", "Ruído: Poeira e arranhões", 1, false, [];
    Median, "Filtros", "Ruído: Mediana", 1, false, [];
    ReduceNoise, "Filtros", "Ruído: Reduzir ruído", 1, false, [];
    Displace, "Filtros", "Distorcer: Deslocar por mapa", 1, false, [];
    Pinch, "Filtros", "Distorcer: Comprimir", 1, false, [];
    Polar, "Filtros", "Distorcer: Coordenadas polares", 1, false, [];
    Ripple, "Filtros", "Distorcer: Encrespar", 1, false, [];
    Shear, "Filtros", "Distorcer: Cisalhamento", 1, false, [];
    Spherize, "Filtros", "Distorcer: Esferizar", 1, false, [];
    Twirl, "Filtros", "Distorcer: Torcer", 1, false, [];
    Wave, "Filtros", "Distorcer: Onda", 1, false, [];
    ZigZag, "Filtros", "Distorcer: Zigue-zague", 1, false, [];
    Glass, "Filtros", "Distorcer: Vidro", 1, false, [];
    ColorHalftone, "Filtros", "Pixelização: Meio-tom colorido", 1, false, [];
    Crystallize, "Filtros", "Pixelização: Cristalizar", 1, false, [];
    Facet, "Filtros", "Pixelização: Facetar", 1, false, [];
    Fragment, "Filtros", "Pixelização: Fragmento", 1, false, [];
    Mezzotint, "Filtros", "Pixelização: Mezzotint", 1, false, [];
    Mosaic, "Filtros", "Pixelização: Mosaico", 1, false, [];
    Pointillize, "Filtros", "Pixelização: Pontilhar", 1, false, [];
    Clouds, "Filtros", "Renderizar: Nuvens", 1, false, [];
    DifferenceClouds, "Filtros", "Renderizar: Nuvens por diferença", 1, false, [];
    Fibers, "Filtros", "Renderizar: Fibras", 1, false, [];
    LensFlare, "Filtros", "Renderizar: Reflexo de lente", 1, false, [];
    Sharpen, "Filtros", "Nitidez: Aplicar nitidez", 1, false, [];
    SharpenMore, "Filtros", "Nitidez: Mais nitidez", 1, false, [];
    SharpenEdges, "Filtros", "Nitidez: Tornar bordas nítidas", 1, false, [];
    AdaptiveSharpen, "Filtros", "Nitidez: Nitidez adaptativa", 1, false, [];
    UnsharpMask, "Filtros", "Nitidez: Máscara de nitidez", 1, false, [];
    Diffuse, "Filtros", "Estilização: Difusão", 1, false, [];
    Emboss, "Filtros", "Estilização: Entalhe", 1, false, [];
    FindEdges, "Filtros", "Estilização: Arestas", 1, false, [];
    OilPaint, "Filtros", "Estilização: Pintura a óleo", 1, false, [];
    Solarize, "Filtros", "Estilização: Solarização", 1, false, [];
    TraceContour, "Filtros", "Estilização: Traçar contorno", 1, false, [];
    Wind, "Filtros", "Estilização: Vento", 1, false, [];
    ColoredPencil, "Filtros", "Artísticos: Lápis de cor", 1, false, [];
    DryBrush, "Filtros", "Artísticos: Pincel a seco", 1, false, [];
    FilmGrain, "Filtros", "Artísticos: Granulação de filme", 1, false, [];
    NeonGlow, "Filtros", "Artísticos: Brilho de neon", 1, false, [];
    PosterEdges, "Filtros", "Artísticos: Arestas posterizadas", 1, false, [];
    Watercolor, "Filtros", "Artísticos: Aquarela", 1, false, [];
    Crosshatch, "Filtros", "Traçados de pincel: Hachura", 1, false, [];
    InkOutlines, "Filtros", "Traçados de pincel: Contornos de tinta", 1, false, [];
    GraphicPen, "Filtros", "Croqui: Bico de pena", 1, false, [];
    Photocopy, "Filtros", "Croqui: Fotocópia", 1, false, [];
    Stamp, "Filtros", "Croqui: Carimbo", 1, false, [];
    HalftonePattern, "Filtros", "Croqui: Matriz de meio-tom", 1, false, [];
    Grain, "Filtros", "Textura: Granulado", 1, false, [];
    MosaicTiles, "Filtros", "Textura: Ladrilhos de mosaico", 1, false, [];
    Patchwork, "Filtros", "Textura: Bordado", 1, false, [];
    StainedGlass, "Filtros", "Textura: Vitral", 1, false, [];
    Texturizer, "Filtros", "Textura: Texturizador", 1, false, [];
    Deinterlace, "Filtros", "Vídeo: Desentrelaçar", 1, false, [];
    Custom, "Filtros", "Outro: Personalizado (3 × 3)", 1, false, [];
    HighPass, "Filtros", "Outro: Alta frequência", 1, false, [];
    Maximum, "Filtros", "Outro: Máximo", 1, false, [];
    Minimum, "Filtros", "Outro: Mínimo", 1, false, [];
    Offset, "Filtros", "Outro: Deslocamento", 1, false, [];
    ImageBrightness, "Imagem", "Brilho / contraste", 1, false, [];
    ImageLevels, "Imagem", "Níveis", 1, false, ["Ctrl+L"];
    ImageCurves, "Imagem", "Curvas", 1, false, ["Ctrl+M"];
    ImageExposure, "Imagem", "Exposição", 1, false, [];
    ImageVibrance, "Imagem", "Vibração", 1, false, [];
    ImageHue, "Imagem", "Matiz / saturação", 1, false, ["Ctrl+U"];
    ImageBalance, "Imagem", "Equilíbrio de cores", 1, false, ["Ctrl+B"];
    ImageBlackWhite, "Imagem", "Preto e branco", 1, false, ["Ctrl+Alt+Shift+B"];
    ImagePhotoFilter, "Imagem", "Filtro de fotos", 1, false, [];
    ImageChannelMixer, "Imagem", "Misturador de canais", 1, false, [];
    ImageInvert, "Imagem", "Inverter cores", 1, false, ["Ctrl+I"];
    ImagePosterize, "Imagem", "Posterizar", 1, false, [];
    ImageThreshold, "Imagem", "Limiar", 1, false, [];
    ImageGradientMap, "Imagem", "Mapa de degradê", 1, false, [];
    ImageShadows, "Imagem", "Sombras / realces", 1, false, [];
    ImageDesaturate, "Imagem", "Desaturar", 1, false, ["Ctrl+Shift+U"];
    ImageReplaceColor, "Imagem", "Substituir cor", 1, false, [];
    ImageEqualize, "Imagem", "Equalizar", 1, false, [];
    ImageAutoTone, "Imagem", "Tom automático", 1, false, ["Ctrl+Shift+L"];
    ImageAutoContrast, "Imagem", "Contraste automático", 1, false, ["Ctrl+Alt+Shift+L"];
    ImageAutoColor, "Imagem", "Cor automática", 1, false, ["Ctrl+Shift+B"];
    ImageSize, "Imagem", "Tamanho da imagem", 1, false, ["Ctrl+Alt+I"];
    ImageCanvasSize, "Imagem", "Tamanho da tela de pintura", 1, false, ["Ctrl+Alt+C"];
    ImageCrop, "Imagem", "Cortar imagem pela seleção", 1, false, [];
    ImageTrim, "Imagem", "Aparar bordas", 1, false, [];
    ImageDuplicate, "Imagem", "Duplicar imagem", 1, false, [];
    ImageRotate90, "Imagem", "Rotacionar imagem 90° horário", 1, false, [];
    ImageRotate180, "Imagem", "Rotacionar imagem 180°", 1, false, [];
    ImageRotate270, "Imagem", "Rotacionar imagem 90° anti-horário", 1, false, [];
    ImageRotate, "Imagem", "Rotação arbitrária da imagem", 1, false, [];
    ImageFlipHorizontal, "Imagem", "Espelhar imagem horizontalmente", 1, false, [];
    ImageFlipVertical, "Imagem", "Espelhar imagem verticalmente", 1, false, [];
    Undo, "Edição", "Desfazer / cancelar transformação", 9, false, ["Ctrl+Z"];
    Redo, "Edição", "Refazer", 1, false, ["Ctrl+Shift+Z", "Ctrl+Y"];
    Copy, "Edição", "Copiar camada ou seleção", 1, false, ["Ctrl+C", "Copy"];
    CopyMerged, "Edição", "Copiar mesclado", 1, false, ["Ctrl+Shift+C"];
    Paste, "Edição", "Colar imagem", 1, false, ["Ctrl+V", "Paste"];
    Brush, "Ferramentas", "Pincel", 1, false, ["B"];
    Eraser, "Ferramentas", "Borracha", 1, false, ["E"];
    Fill, "Ferramentas", "Preencher", 1, false, ["G"];
    Picker, "Ferramentas", "Conta-gotas", 1, false, ["I"];
    Line, "Ferramentas", "Linha", 1, false, ["L"];
    Rectangle, "Ferramentas", "Retângulo", 1, false, ["R"];
    Ellipse, "Ferramentas", "Elipse", 1, false, ["O"];
    ZoomTool, "Ferramentas", "Zoom", 1, false, ["Z"];
    ZoomOutModifier, "Canvas", "Afastar ao clicar com a ferramenta Zoom", 16, true, ["Alt"];
    Hand, "Ferramentas", "Mover tela", 1, false, ["H"];
    SelectRect, "Ferramentas", "Seleção retangular", 1, false, ["S"];
    Lasso, "Ferramentas", "Laço", 1, false, ["A"];
    SmallerBrush, "Pincel", "Diminuir tamanho", 1, false, ["OpenBracket"];
    LargerBrush, "Pincel", "Aumentar tamanho", 1, false, ["CloseBracket"];
    BrushSizes, "Pincel", "Abrir tamanhos de pincel", 1, false, ["F6"];
    RotateLeft, "Canvas", "Rotacionar vista −15°", 1, false, ["Q"];
    RotateRight, "Canvas", "Rotacionar vista +15°", 1, false, ["W"];
    Mirror, "Canvas", "Espelhar vista", 1, false, ["M"];
    Fit, "Canvas", "Ajustar canvas à janela", 1, false, ["F"];
    ZoomIn, "Canvas", "Aumentar zoom", 9, false, ["Ctrl+Plus", "Ctrl+Equals", "Ctrl+Shift+Plus", "Ctrl+Shift+Equals"];
    ZoomOut, "Canvas", "Diminuir zoom", 9, false, ["Ctrl+Minus", "Ctrl+Shift+Minus"];
    ResetZoom, "Canvas", "Zoom de 100%", 9, false, ["Ctrl+0"];
    QuickShapeRegular, "QuickShape", "Formas regulares / rotação de 15°", 128, true, ["Shift"];
    QuickShapeApply, "QuickShape", "Aplicar forma", 128, false, ["Enter"];
    QuickShapeCancel, "QuickShape", "Restaurar traço original", 128, false, ["Escape"];
    QuickShapeToggle, "QuickShape", "Ativar / desativar QuickShape", 1, false, [];
    RulerTool, "Réguas", "Criar / editar réguas", 1, false, ["U"];
    SnapRulers, "Réguas", "Ativar encaixe nas réguas", 1, false, ["Ctrl+2"];
    ShowRulers, "Réguas", "Mostrar / ocultar réguas", 1, false, ["Ctrl+Shift+R"];
    RulerAngleSnap, "Réguas", "Ângulos de 15° ao editar régua", 64, true, ["Shift"];
    TemporaryZoom, "Canvas", "Zoom temporário: arraste para aproximar/afastar", 15, true, ["Ctrl+Space"];
    Pan, "Canvas", "Mover vista enquanto segura", 15, true, ["Space"];
    BrushHud, "Pincel", "HUD: arraste para tamanho / opacidade", 17, true, ["Ctrl+Alt"];
    TemporaryPicker, "Pincel", "Conta-gotas temporário enquanto segura", 2, true, ["Alt"];
    SelectAll, "Seleção", "Selecionar tudo", 1, false, ["Ctrl+A"];
    Deselect, "Seleção", "Desmarcar seleção", 1, false, ["Ctrl+D"];
    Transform, "Seleção", "Mover / transformar conteúdo", 1, false, ["Ctrl+T"];
    DeleteSelection, "Seleção", "Apagar conteúdo selecionado", 1, false, ["Delete"];
    Cancel, "Seleção", "Cancelar seleção / transformação", 13, false, ["Escape"];
    Apply, "Transformação", "Aplicar transformação", 8, false, ["Enter"];
    MoveLeft, "Transformação", "Mover 1 px à esquerda", 8, false, ["ArrowLeft"];
    MoveRight, "Transformação", "Mover 1 px à direita", 8, false, ["ArrowRight"];
    MoveUp, "Transformação", "Mover 1 px para cima", 8, false, ["ArrowUp"];
    MoveDown, "Transformação", "Mover 1 px para baixo", 8, false, ["ArrowDown"];
    FastLeft, "Transformação", "Mover 10 px à esquerda", 8, false, ["Shift+ArrowLeft"];
    FastRight, "Transformação", "Mover 10 px à direita", 8, false, ["Shift+ArrowRight"];
    FastUp, "Transformação", "Mover 10 px para cima", 8, false, ["Shift+ArrowUp"];
    FastDown, "Transformação", "Mover 10 px para baixo", 8, false, ["Shift+ArrowDown"];
    LayerToggleSelection, "Camadas", "Alternar seleção de camada", 32, true, ["Ctrl"];
    LayerRangeSelection, "Camadas", "Selecionar intervalo de camadas", 32, true, ["Shift"];
    LayerGroup, "Camadas", "Agrupar camadas selecionadas", 1, false, ["Ctrl+G"];
    LayerMerge, "Camadas", "Mesclar camadas selecionadas", 1, false, ["Ctrl+Shift+E"];
    SelectionAdd, "Seleção", "Adicionar à seleção enquanto segura", 4, true, ["Shift"];
    SelectionSubtract, "Seleção", "Subtrair da seleção enquanto segura", 4, true, ["Alt"];
    Proportion, "Transformação", "Alternar proporção enquanto segura", 8, true, ["Shift"];
    ConstrainMove, "Transformação", "Restringir movimento a 45° enquanto segura", 8, true, ["Shift"];
    RotationSnap, "Transformação", "Encaixar rotação em 15° enquanto segura", 8, true, ["Shift"];
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
            "Sem atalho".into()
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
