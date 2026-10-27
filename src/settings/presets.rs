use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderBackend {
    #[default]
    OpenGl,
    Vulkan,
}
fn default_spacing() -> f32 {
    0.15
}
fn default_update_checks() -> bool {
    true
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub eraser: bool,
    #[serde(default)]
    pub texture: Option<String>,
    #[serde(default = "default_spacing")]
    pub spacing: f32,
    pub size: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub stabilization: f32,
    pub pressure: bool,
    pub gamma: f32,
    pub minimum: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewCanvasSettings {
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    pub paper: bool,
    pub paper_color: [u8; 3],
}
impl Default for NewCanvasSettings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 800,
            dpi: crate::document::DEFAULT_DPI,
            paper: true,
            paper_color: [255; 3],
        }
    }
}
impl NewCanvasSettings {
    fn valid(&self) -> bool {
        (16..=crate::document::MAX_SIDE).contains(&self.width)
            && (16..=crate::document::MAX_SIDE).contains(&self.height)
            && (1..=crate::document::MAX_DPI).contains(&self.dpi)
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub shortcuts: crate::shortcuts::Shortcuts,
    pub version: u32,
    #[serde(default)]
    pub new_canvas: NewCanvasSettings,
    #[serde(default)]
    pub appearance: crate::theme::Appearance,
    #[serde(default)]
    pub cursor_style: crate::preferences::CursorStyle,
    #[serde(default)]
    pub circle_cursor_pressure: bool,
    #[serde(default = "scrubby_enabled")]
    pub scrubby_zoom: bool,
    #[serde(default = "scrubby_enabled")]
    pub show_rulers: bool,
    #[serde(default = "scrubby_enabled")]
    pub snap_rulers: bool,
    #[serde(default)]
    pub quickshape: crate::quickshape::Settings,
    #[serde(default)]
    pub references: crate::reference::Library,
    #[serde(default)]
    pub render_gpu: String,
    #[serde(default)]
    pub language: crate::i18n::Language,
    #[serde(default)]
    pub render_backend: RenderBackend,
    #[serde(default)]
    pub triangular_color_selector: bool,
    #[serde(default = "default_update_checks")]
    pub check_for_updates: bool,
    pub selected: usize,
    #[serde(default)]
    pub last_brush: Option<usize>,
    #[serde(default)]
    pub last_eraser: Option<usize>,
    pub brushes: Vec<Preset>,
    #[serde(default)]
    pub originals: Vec<Preset>,
}
fn scrubby_enabled() -> bool {
    true
}
impl Default for Settings {
    fn default() -> Self {
        let mut settings = Self {
            shortcuts: Default::default(),
            originals: Vec::new(),
            version: 1,
            new_canvas: Default::default(),
            appearance: Default::default(),
            cursor_style: Default::default(),
            circle_cursor_pressure: false,
            scrubby_zoom: true,
            show_rulers: true,
            snap_rulers: true,
            quickshape: Default::default(),
            references: Default::default(),
            render_gpu: String::new(),
            language: crate::i18n::Language::default(),
            render_backend: RenderBackend::default(),
            triangular_color_selector: false,
            check_for_updates: true,
            selected: 0,
            last_brush: Some(0),
            last_eraser: Some(4),
            brushes: vec![
                Preset {
                    name: "Caneta de arte-final".into(),
                    eraser: false,
                    texture: None,
                    spacing: 0.15,
                    size: 12.0,
                    opacity: 1.0,
                    hardness: 1.0,
                    stabilization: 25.0,
                    pressure: true,
                    gamma: 0.7,
                    minimum: 0.05,
                },
                Preset {
                    name: "Lápis".into(),
                    eraser: false,
                    texture: None,
                    spacing: 0.15,
                    size: 4.0,
                    opacity: 0.45,
                    hardness: 0.7,
                    stabilization: 10.0,
                    pressure: true,
                    gamma: 1.0,
                    minimum: 0.15,
                },
                Preset {
                    name: "Opaque Brush".into(),
                    eraser: false,
                    texture: None,
                    spacing: 0.15,
                    size: 32.0,
                    opacity: 1.0,
                    hardness: 0.8,
                    stabilization: 10.0,
                    pressure: true,
                    gamma: 0.65,
                    minimum: 0.1,
                },
                Preset {
                    name: "Aerógrafo".into(),
                    eraser: false,
                    texture: None,
                    spacing: 0.15,
                    size: 90.0,
                    opacity: 0.12,
                    hardness: 0.0,
                    stabilization: 0.0,
                    pressure: true,
                    gamma: 0.8,
                    minimum: 0.3,
                },
                Preset {
                    name: "Borracha".into(),
                    eraser: true,
                    texture: None,
                    spacing: 0.15,
                    size: 32.0,
                    opacity: 1.0,
                    hardness: 1.0,
                    stabilization: 10.0,
                    pressure: true,
                    gamma: 0.7,
                    minimum: 0.1,
                },
            ],
        };
        settings.originals = settings.brushes.clone();
        settings
    }
}
impl Settings {
    pub fn last_for_tool(&self, eraser: bool) -> Option<usize> {
        let last = if eraser {
            self.last_eraser
        } else {
            self.last_brush
        };
        last.filter(|&i| self.brushes.get(i).is_some_and(|p| p.eraser == eraser))
            .or_else(|| self.brushes.iter().position(|p| p.eraser == eraser))
    }
    pub fn valid(&self) -> bool {
        self.version == 1
            && self.new_canvas.valid()
            && self.appearance.valid()
            && self.references.valid()
            && self.quickshape.delay.is_finite()
            && (0.2..=2.0).contains(&self.quickshape.delay)
            && !self.brushes.is_empty()
            && self.brushes.len() <= 256
            && self.selected < self.brushes.len()
            && (self.originals.is_empty() || self.originals.len() == self.brushes.len())
            && self.brushes.iter().chain(self.originals.iter()).all(|b| {
                !b.name.trim().is_empty()
                    && b.name.chars().count() <= 100
                    && b.texture
                        .as_ref()
                        .is_none_or(|id| !id.is_empty() && id.len() <= 256)
                    && [
                        (b.spacing, 0.05, 2.0),
                        (b.size, 1.0, crate::painting::MAX_BRUSH_SIZE),
                        (b.opacity, 0.01, 1.0),
                        (b.hardness, 0.0, 1.0),
                        (b.stabilization, 0.0, 100.0),
                        (b.gamma, 0.3, 3.0),
                        (b.minimum, 0.01, 1.0),
                    ]
                    .iter()
                    .all(|(v, min, max)| v.is_finite() && v >= min && v <= max)
            })
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        // Reference libraries store paths here; allow large lists without embedding pixels.
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("Preferências muito grandes".into());
        }
        let mut s: Self = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        s.shortcuts.sanitize();
        if !s.valid() {
            return Err("Preferências inválidas".into());
        }
        if s.originals.is_empty() {
            s.originals = s.brushes.clone();
        }
        Ok(s)
    }
    pub fn add(&mut self, mut preset: Preset) -> Result<usize, String> {
        preset.name = preset.name.trim().to_owned();
        if self.brushes.len() >= 256 {
            return Err("Limite de 256 pincéis atingido.".into());
        }
        if preset.name.is_empty() || preset.name.chars().count() > 100 {
            return Err("Informe um nome de até 100 caracteres.".into());
        }
        if self
            .brushes
            .iter()
            .any(|p| p.name.to_lowercase() == preset.name.to_lowercase())
        {
            return Err("Já existe um pincel com esse nome.".into());
        }
        let mut candidate = self.clone();
        if candidate.originals.is_empty() {
            candidate.originals = candidate.brushes.clone();
        }
        candidate.brushes.push(preset.clone());
        candidate.originals.push(preset);
        candidate.selected = candidate.brushes.len() - 1;
        if !candidate.valid() {
            return Err("Configurações de pincel inválidas.".into());
        }
        let index = candidate.selected;
        *self = candidate;
        Ok(index)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if !self.valid() {
            return Err("Preferências inválidas".into());
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let mut f = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        f.write_all(&bytes)
            .and_then(|_| f.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        f.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub fn settings_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    if let Some(path) = std::env::var_os("PAINTATLAS_SETTINGS") {
        return Some(path.into());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))?;
    Some(base.join("paintatlas/brushes.json"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quickshape_preferences_roundtrip_and_legacy_defaults() {
        let mut settings = Settings::default();
        settings.quickshape.enabled = false;
        settings.quickshape.delay = 1.2;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quickshape.json");
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert!(!loaded.quickshape.enabled);
        assert_eq!(loaded.quickshape.delay, 1.2);
        let mut value = serde_json::to_value(&settings).unwrap();
        value.as_object_mut().unwrap().remove("quickshape");
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert!(loaded.quickshape.enabled);
        assert_eq!(loaded.quickshape.delay, 0.65);
    }
    #[test]
    fn custom_presets_and_originals_survive_restart_and_legacy_settings_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("brushes.json");
        let mut s = Settings {
            triangular_color_selector: true,
            ..Default::default()
        };
        let mut custom = s.brushes[0].clone();
        custom.name = "Minha caneta".into();
        custom.size = 63.0;
        custom.texture = Some("default:hexagon.png".into());
        custom.spacing = 0.6;
        let i = s.add(custom.clone()).unwrap();
        assert_eq!(i, 5);
        s.brushes[i].size = 77.0;
        s.save(&path).unwrap();
        let restored = Settings::load(&path).unwrap();
        assert!(restored == s);
        assert_eq!(restored.originals[i].size, 63.0);
        assert_eq!(restored.brushes[i].size, 77.0);
        assert!(s.add(custom).is_err(), "duplicate names must be rejected");
        let mut legacy = serde_json::to_value(Settings::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("new_canvas");
        legacy.as_object_mut().unwrap().remove("originals");
        legacy.as_object_mut().unwrap().remove("appearance");
        legacy.as_object_mut().unwrap().remove("cursor_style");
        legacy
            .as_object_mut()
            .unwrap()
            .remove("circle_cursor_pressure");
        legacy.as_object_mut().unwrap().remove("render_gpu");
        legacy.as_object_mut().unwrap().remove("render_backend");
        legacy
            .as_object_mut()
            .unwrap()
            .remove("triangular_color_selector");
        for brush in legacy["brushes"].as_array_mut().unwrap() {
            let b = brush.as_object_mut().unwrap();
            b.remove("texture");
            b.remove("spacing");
        }

        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let restored = Settings::load(&path).unwrap();
        assert_eq!(restored.originals.len(), 5);
        assert!(!restored.triangular_color_selector);
        assert!(restored.appearance == crate::theme::Appearance::default());
        assert!(restored.cursor_style == crate::preferences::CursorStyle::Brush);
        assert!(!restored.circle_cursor_pressure);
        assert!(restored.scrubby_zoom);
        assert!(restored.render_gpu.is_empty());
        assert_eq!(restored.render_backend, RenderBackend::OpenGl);
        assert_eq!(restored.new_canvas, NewCanvasSettings::default());
        assert!(restored.valid());
    }
    #[test]
    fn scrubby_zoom_preference_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = Settings {
            scrubby_zoom: false,
            ..Default::default()
        };
        settings.save(&path).unwrap();
        assert!(!Settings::load(&path).unwrap().scrubby_zoom);
    }
    #[test]
    fn canvas_defaults_survive_restart_and_invalid_dimensions_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings {
            new_canvas: NewCanvasSettings {
                width: 1920,
                height: 1080,
                dpi: 600,
                paper: false,
                paper_color: [230, 210, 170],
            },
            ..Default::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(
            Settings::load(&path).unwrap().new_canvas,
            settings.new_canvas
        );
        settings.new_canvas.width = 0;
        assert!(!settings.valid());
        settings.new_canvas.width = 1920;
        settings.new_canvas.dpi = 0;
        assert!(!settings.valid());
    }

    #[test]
    fn customized_presets_survive_restart_and_corruption_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("brushes.json");
        let mut s = Settings {
            selected: 3,
            ..Default::default()
        };
        s.brushes[3].size = 1000.0;
        s.brushes[3].gamma = 1.7;
        s.brushes[3].stabilization = 37.0;
        s.cursor_style = crate::preferences::CursorStyle::Dot;
        s.circle_cursor_pressure = true;
        s.render_gpu = "pci-0000_03_00_0".into();
        s.appearance.accent = crate::theme::Accent::Rose;
        s.appearance.transparency = 0.8;
        s.appearance.reduced_motion = true;
        s.save(&p).unwrap();
        assert!(Settings::load(&p).unwrap() == s);
        std::fs::write(&p, "{}").unwrap();
        assert!(Settings::load(&p).is_err());
    }
}
