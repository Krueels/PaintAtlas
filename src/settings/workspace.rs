//! Persistent, document-independent panel layout and dock operations.
use bincode::Options;
use eframe::egui::{Pos2, Vec2};
use egui_dock::{DockState, Node, NodeIndex, SurfaceIndex};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Panel {
    Canvas,
    Tools,
    Properties,
    Brushes,
    Colors,
    Layers,
    Navigation,
    BrushSizes,
    Files,
    Help,
    Settings,
    References,
}
impl Panel {
    pub const ALL: [Self; 12] = [
        Self::Canvas,
        Self::Tools,
        Self::Properties,
        Self::Brushes,
        Self::Colors,
        Self::Layers,
        Self::Navigation,
        Self::BrushSizes,
        Self::Files,
        Self::Help,
        Self::Settings,
        Self::References,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Canvas => "Canvas",
            Self::Tools => "Tools",
            Self::Properties => "Properties",
            Self::Brushes => "Brushes",
            Self::Colors => "Colors",
            Self::Layers => "Layers",
            Self::Navigation => "Navigation",
            Self::BrushSizes => "Brush Sizes",
            Self::Files => "File",
            Self::Help => "Help",
            Self::Settings => "Settings",
            Self::References => "References",
        }
    }
    pub fn localized_title(self, language: crate::i18n::Language) -> &'static str {
        let key = match self {
            Self::Canvas => "panel.canvas",
            Self::Tools => "panel.tools",
            Self::Properties => "panel.properties",
            Self::Brushes => "panel.brushes",
            Self::Colors => "panel.colors",
            Self::Layers => "panel.layers",
            Self::Navigation => "panel.navigation",
            Self::BrushSizes => "panel.brush_sizes",
            Self::Files => "panel.files",
            Self::Help => "panel.help",
            Self::Settings => "panel.settings",
            Self::References => "panel.references",
        };
        crate::i18n::text(language, key)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Placement {
    Left,
    Right,
    Above,
    Below,
    Tab,
}
impl Placement {
    pub const ALL: [Self; 5] = [Self::Left, Self::Right, Self::Above, Self::Below, Self::Tab];
    pub fn localized_title(self, language: crate::i18n::Language) -> &'static str {
        let key = match self {
            Self::Left => "placement.left",
            Self::Right => "placement.right",
            Self::Above => "placement.above",
            Self::Below => "placement.below",
            Self::Tab => "placement.tab",
        };
        crate::i18n::text(language, key)
    }
}
#[derive(Clone, Copy)]
pub enum Action {
    Open(Panel),
    Float(Panel),
    External(Panel),
    Dock(Panel, Placement),
    Hide(Panel),
    Reset,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct External {
    pub panel: Panel,
    pub position: Option<Pos2>,
    pub size: Vec2,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Layout {
    pub version: u32,
    pub dock: DockState<Panel>,
    pub external: Vec<External>,
}
impl Default for Layout {
    fn default() -> Self {
        let mut dock = DockState::new(vec![Panel::Canvas]);
        let [canvas, left] = dock.main_surface_mut().split_left(
            NodeIndex::root(),
            0.19,
            vec![Panel::Tools, Panel::Files],
        );
        let [_, right] = dock
            .main_surface_mut()
            .split_right(canvas, 0.74, vec![Panel::Colors]);
        let [_, properties] =
            dock.main_surface_mut()
                .split_below(left, 0.27, vec![Panel::Properties]);
        dock.main_surface_mut().split_below(
            properties,
            0.66,
            vec![Panel::Brushes, Panel::Navigation],
        );
        dock.main_surface_mut()
            .split_below(right, 0.48, vec![Panel::Layers]);
        translate(&mut dock);
        Self {
            version: 1,
            dock,
            external: Vec::new(),
        }
    }
}
impl Layout {
    pub fn contains(&self, panel: Panel) -> bool {
        self.dock.find_tab(&panel).is_some() || self.external.iter().any(|p| p.panel == panel)
    }
    fn remove(&mut self, panel: Panel) {
        if let Some(location) = self.dock.find_tab(&panel) {
            self.dock.remove_tab(location);
        }
        self.external.retain(|p| p.panel != panel);
    }
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Reset => *self = Self::default(),
            Action::Open(panel) => {
                if let Some(path) = self.dock.find_tab(&panel) {
                    self.dock.set_active_tab(path);
                    self.dock.set_focused_node_and_surface((path.0, path.1));
                } else if !self.external.iter().any(|p| p.panel == panel) {
                    self.apply(Action::Float(panel));
                }
            }
            Action::Float(panel) if panel != Panel::Canvas => {
                self.remove(panel);
                let surface = self.dock.add_window(vec![panel]);
                if let Some(window) = self.dock.get_window_state_mut(surface) {
                    window
                        .set_position(Pos2::new(260.0, 140.0))
                        .set_size(Vec2::new(
                            if panel == Panel::Settings {
                                580.0
                            } else {
                                360.0
                            },
                            500.0,
                        ));
                }
            }
            Action::External(panel) if panel != Panel::Canvas => {
                self.remove(panel);
                self.external.push(External {
                    panel,
                    position: None,
                    size: Vec2::new(360.0, 520.0),
                });
            }
            Action::Dock(panel, placement) if panel != Panel::Canvas => {
                self.remove(panel);
                let (surface, node, _) = self
                    .dock
                    .find_tab(&Panel::Canvas)
                    .expect("canvas is permanent");
                match placement {
                    Placement::Left => {
                        self.dock[surface].split_left(node, 0.25, vec![panel]);
                    }
                    Placement::Right => {
                        self.dock[surface].split_right(node, 0.75, vec![panel]);
                    }
                    Placement::Above => {
                        self.dock[surface].split_above(node, 0.25, vec![panel]);
                    }
                    Placement::Below => {
                        self.dock[surface].split_below(node, 0.75, vec![panel]);
                    }
                    Placement::Tab => {
                        self.dock.set_focused_node_and_surface((surface, node));
                        self.dock.push_to_focused_leaf(panel);
                    }
                }
            }
            Action::Hide(panel) if panel != Panel::Canvas => self.remove(panel),
            _ => {}
        }
        translate(&mut self.dock);
    }
    pub fn valid(&self) -> bool {
        if self.version != 1
            || self.dock.surfaces_count() > Panel::ALL.len()
            || self.external.len() > Panel::ALL.len()
        {
            return false;
        }
        if !self
            .dock
            .find_tab(&Panel::Canvas)
            .is_some_and(|(s, _, _)| s == SurfaceIndex::main())
        {
            return false;
        }
        let mut seen = HashSet::new();
        for (_, panel) in self.dock.iter_all_tabs() {
            if !seen.insert(*panel) {
                return false;
            }
        }
        for p in &self.external {
            if p.panel == Panel::Canvas
                || !seen.insert(p.panel)
                || !p.size.is_finite()
                || p.size.x < 100.0
                || p.size.y < 100.0
                || p.size.max_elem() > 20000.0
                || p.position
                    .is_some_and(|v| !v.x.is_finite() || !v.y.is_finite())
            {
                return false;
            }
        }
        let mut count = 0;
        for (_, node) in self.dock.iter_all_nodes() {
            count += 1;
            if count > 512 {
                return false;
            }
            match node {
                Node::Leaf(leaf) if leaf.tabs.is_empty() || leaf.active.0 >= leaf.tabs.len() => {
                    return false;
                }
                Node::Horizontal(split) | Node::Vertical(split)
                    if !split.fraction.is_finite() || !(0.0..=1.0).contains(&split.fraction) =>
                {
                    return false;
                }
                _ => {}
            }
        }
        true
    }
}
pub fn translate(dock: &mut DockState<Panel>) {
    dock.translations.tab_context_menu.close_button = "Hide panel".into();
    dock.translations.tab_context_menu.eject_button = "Undock".into();
    dock.translations.leaf.close_all_button = "Close group".into();
    dock.translations.leaf.minimize_button = "Collapse group".into();
}
pub struct Workspace {
    pub layout: Layout,
    path: Option<PathBuf>,
    observed: Vec<u8>,
    dirty: bool,
    edited: Instant,
}
impl Workspace {
    pub fn new(path: Option<PathBuf>) -> Self {
        let mut layout = path
            .as_ref()
            .and_then(|p| Self::read(p).ok())
            .unwrap_or_default();
        translate(&mut layout.dock);
        Self {
            layout,
            path,
            observed: Vec::new(),
            dirty: false,
            edited: Instant::now(),
        }
    }
    pub fn read(path: &Path) -> Result<Layout, String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 512 * 1024 {
            return Err("Layout is too large".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let layout: Layout = bincode::DefaultOptions::new()
            .with_limit(512 * 1024)
            .deserialize(&bytes)
            .map_err(|e| e.to_string())?;
        if !layout.valid() {
            return Err("Invalid layout".into());
        }
        Ok(layout)
    }
    pub fn persist(&mut self, ctx: &eframe::egui::Context, force: bool) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let bytes = bincode::DefaultOptions::new()
            .serialize(&self.layout)
            .map_err(|e| e.to_string())?;
        if bytes != self.observed {
            self.observed = bytes;
            self.dirty = true;
            self.edited = Instant::now();
        }
        if self.dirty {
            if force || self.edited.elapsed() > Duration::from_millis(600) {
                let parent = path.parent().unwrap_or(Path::new("."));
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                let mut f = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
                f.write_all(&self.observed)
                    .and_then(|_| f.as_file().sync_all())
                    .map_err(|e| e.to_string())?;
                f.persist(path).map_err(|e| e.to_string())?;
                self.dirty = false;
            } else {
                ctx.request_repaint_after(Duration::from_millis(600));
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panels_detach_redock_hide_and_restore_without_duplicates() {
        let mut layout = Layout::default();
        assert!(layout.valid());
        layout.apply(Action::Float(Panel::Properties));
        assert!(layout.dock.find_tab(&Panel::Properties).unwrap().0 != SurfaceIndex::main());
        layout.apply(Action::External(Panel::Properties));
        assert_eq!(layout.external.len(), 1);
        layout.apply(Action::Dock(Panel::Properties, Placement::Left));
        assert!(layout.external.is_empty());
        layout.apply(Action::Open(Panel::Properties));
        assert!(layout.valid());
        layout.apply(Action::Hide(Panel::Properties));
        assert!(!layout.contains(Panel::Properties));
        layout.apply(Action::Hide(Panel::Canvas));
        assert!(layout.contains(Panel::Canvas));
        layout.apply(Action::Reset);
        assert!(layout.contains(Panel::Properties));
        assert!(layout.valid());
    }
    #[test]
    fn layout_roundtrip_retains_floating_and_native_panels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.bin");
        let ctx = eframe::egui::Context::default();
        let mut workspace = Workspace::new(Some(path.clone()));
        workspace.layout.apply(Action::Float(Panel::BrushSizes));
        workspace.layout.apply(Action::External(Panel::Colors));
        workspace.persist(&ctx, true).unwrap();
        let restored = Workspace::read(&path).unwrap();
        assert!(restored.valid());
        assert_eq!(restored.external[0].panel, Panel::Colors);
        assert!(restored.contains(Panel::BrushSizes));
        std::fs::write(&path, b"broken").unwrap();
        assert!(Workspace::read(&path).is_err());
    }
}
