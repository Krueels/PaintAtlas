//! Shared visual language; no layout, document or docking data is changed here.
use eframe::egui::{self, Color32, Pos2, Response, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    Blue,
    Mint,
    Violet,
    Rose,
    Orange,
}
impl Accent {
    pub const ALL: [Self; 5] = [
        Self::Blue,
        Self::Mint,
        Self::Violet,
        Self::Rose,
        Self::Orange,
    ];
    pub fn color(self) -> Color32 {
        match self {
            Self::Blue => Color32::from_rgb(64, 156, 255),
            Self::Mint => Color32::from_rgb(100, 210, 180),
            Self::Violet => Color32::from_rgb(179, 145, 255),
            Self::Rose => Color32::from_rgb(255, 117, 155),
            Self::Orange => Color32::from_rgb(255, 179, 94),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Blue => "Azul",
            Self::Mint => "Menta",
            Self::Violet => "Violeta",
            Self::Rose => "Rosa",
            Self::Orange => "Âmbar",
        }
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub accent: Accent,
    pub transparency: f32,
    pub reduced_motion: bool,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            accent: Accent::Blue,
            transparency: 0.55,
            reduced_motion: false,
        }
    }
}
impl Appearance {
    pub fn valid(&self) -> bool {
        self.transparency.is_finite() && (0.0..=1.0).contains(&self.transparency)
    }
}
pub fn appearance(ctx: &egui::Context) -> Appearance {
    ctx.data(|d| d.get_temp::<Appearance>(egui::Id::new("paintatlas_appearance")))
        .unwrap_or_default()
}
pub fn accent(ctx: &egui::Context) -> Color32 {
    appearance(ctx).accent.color()
}
pub fn reduced_motion(ctx: &egui::Context) -> bool {
    appearance(ctx).reduced_motion
}
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Inter.ttf")).into(),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "Inter".into());
    let local_data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/share"))
        });
    if let Some(path) = local_data.map(|base| base.join("paintatlas/fonts/SF-Pro-Text-Regular.otf"))
        && let Ok(bytes) = std::fs::read(&path)
    {
        fonts.font_data.insert(
            "SF Pro Text".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "SF Pro Text".into());
        eprintln!("Fonte da interface: SF Pro Text ({})", path.display());
    }
    ctx.set_fonts(fonts);
}
pub fn apply(ctx: &egui::Context, a: &Appearance) {
    let id = egui::Id::new("paintatlas_appearance");
    if ctx.data(|d| d.get_temp::<Appearance>(id)).as_ref() == Some(a) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(id, a.clone()));
    let mut s = (*ctx.style()).clone();
    s.visuals = egui::Visuals::dark();
    let v = &mut s.visuals;
    let accent = a.accent.color();
    let material = glass_tint(a.transparency);
    v.panel_fill = material;
    v.extreme_bg_color = Color32::from_gray(20);
    v.faint_bg_color = Color32::from_gray(36);
    v.window_fill = material;
    v.window_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(26));
    v.window_corner_radius = egui::CornerRadius::same(16);
    v.menu_corner_radius = egui::CornerRadius::same(12);
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 9],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    v.popup_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    v.weak_text_color = Some(Color32::from_gray(190));
    v.selection.bg_fill = mix(Color32::from_gray(35), accent, 0.28);
    v.selection.stroke = Stroke::new(1.0_f32, accent);
    v.hyperlink_color = accent;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.widgets.noninteractive.bg_fill = Color32::from_gray(31);
    v.widgets.noninteractive.weak_bg_fill = Color32::from_gray(31);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(49));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, Color32::from_gray(232));
    for (w, fill, stroke) in [
        (&mut v.widgets.inactive, 44, 55),
        (&mut v.widgets.hovered, 55, 75),
        (&mut v.widgets.active, 63, 95),
        (&mut v.widgets.open, 49, 70),
    ] {
        w.bg_fill = Color32::from_gray(fill);
        w.weak_bg_fill = Color32::from_gray(fill);
        w.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(stroke));
        w.fg_stroke = Stroke::new(1.3_f32, Color32::from_gray(235));
        w.corner_radius = egui::CornerRadius::same(8);
        w.expansion = 0.0;
    }
    s.animation_time = if a.reduced_motion { 0.0 } else { 0.16 };
    // Keep the existing control sizes, grid spacing and dock geometry.
    s.spacing.item_spacing = Vec2::new(8.0, 9.0);
    s.spacing.button_padding = Vec2::new(12.0, 7.0);
    s.text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(10.0));
    ctx.set_style(s);
    ctx.request_repaint();
}
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t).round() as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t).round() as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t).round() as u8,
    )
}
pub fn glass_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::popup(ui.style())
        .corner_radius(16)
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(28)))
}
/// This design always has glass, including preferences saved with 0% transparency.
pub fn glass_strength(value: f32) -> f32 {
    value.clamp(0.35, 0.85)
}
fn glass_tint(value: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        24,
        27,
        33,
        (200.0 - glass_strength(value) * 36.0).round() as u8,
    )
}

// Frames are finished after layout, before egui tessellates. Insert the blur
// immediately before their tint, so controls are never sampled into their own
// backdrop. This also covers native egui popups, combo boxes and modal dialogs.
pub fn finish_glass(ctx: &egui::Context) {
    let Some(shader) = ctx.data(|d| {
        d.get_temp::<std::sync::Arc<crate::glass_gpu::Shader>>(egui::Id::new("glass_shader"))
    }) else {
        return;
    };
    let a = appearance(ctx);
    let material = glass_tint(a.transparency);
    let strength = glass_strength(a.transparency);
    let mut layers = ctx.memory(|m| m.layer_ids().collect::<Vec<_>>());
    if !layers.contains(&egui::LayerId::background()) {
        layers.push(egui::LayerId::background());
    }
    ctx.graphics_mut(|graphics| {
        for layer in layers {
            if let Some(list) = graphics.get_mut(layer) {
                let count = list.all_entries().len();
                for index in 0..count {
                    list.mutate_shape(egui::layers::ShapeIdx(index), |entry| {
                        frost_frame(&mut entry.shape, material, &|rect, corner, opacity| {
                            shader.callback(rect, corner, strength, opacity)
                        });
                    });
                }
            }
        }
    });
}
fn frost_frame(
    shape: &mut egui::Shape,
    material: Color32,
    callback: &impl Fn(egui::Rect, f32, f32) -> egui::PaintCallback,
) {
    match shape {
        egui::Shape::Vec(shapes) => {
            // Already finished in this viewport; do not add a second blur.
            if !matches!(shapes.first(), Some(egui::Shape::Callback(_))) {
                for shape in shapes {
                    frost_frame(shape, material, callback);
                }
            }
        }
        egui::Shape::Rect(rect) => {
            let corner = rect.corner_radius.nw;
            if corner < 8
                || rect.rect.width() < 32.0
                || rect.rect.height() < 16.0
                || rect.fill.a() == 0
                || rect.fill.a() > material.a()
            {
                return;
            }
            let opacity = rect.fill.a() as f32 / material.a() as f32;
            let expected = material.gamma_multiply(opacity);
            if !rect
                .fill
                .to_array()
                .iter()
                .zip(expected.to_array())
                .all(|(a, b)| a.abs_diff(b) <= 1)
            {
                return;
            }
            *shape = egui::Shape::Vec(vec![
                callback(rect.rect, corner as f32, opacity).into(),
                shape.clone(),
            ]);
        }
        _ => {}
    }
}
pub fn backdrop(ctx: &egui::Context, painter: &egui::Painter, rect: egui::Rect, corner: f32) {
    let strength = glass_strength(appearance(ctx).transparency);
    if strength > 0.0
        && let Some(shader) = ctx.data(|d| {
            d.get_temp::<std::sync::Arc<crate::glass_gpu::Shader>>(egui::Id::new("glass_shader"))
        })
    {
        painter.add(shader.callback(rect, corner, strength, painter.opacity()));
    }
}
pub fn dock(style: &mut egui_dock::Style, ctx: &egui::Context) {
    let accent = accent(ctx);
    style.main_surface_border_stroke = Stroke::NONE;
    style.tab_bar.bg_fill = glass_tint(appearance(ctx).transparency);
    style.tab_bar.hline_color = Color32::TRANSPARENT;
    style.tab_bar.corner_radius = egui::CornerRadius::same(8);
    style.tab.tab_body.bg_fill = glass_tint(appearance(ctx).transparency);
    style.tab.tab_body.corner_radius = egui::CornerRadius::same(12);
    style.tab.tab_body.stroke = Stroke::new(1.0_f32, Color32::from_gray(43));
    style.tab.hline_below_active_tab_name = true;
    for tab in [
        &mut style.tab.active,
        &mut style.tab.focused,
        &mut style.tab.active_with_kb_focus,
        &mut style.tab.focused_with_kb_focus,
    ] {
        tab.corner_radius = egui::CornerRadius::same(11);
        tab.bg_fill = Color32::from_gray(49);
        tab.text_color = Color32::from_gray(245);
        tab.outline_color = Color32::from_gray(65);
    }
    for tab in [
        &mut style.tab.inactive,
        &mut style.tab.inactive_with_kb_focus,
    ] {
        tab.corner_radius = egui::CornerRadius::same(11);
        tab.bg_fill = Color32::TRANSPARENT;
        tab.text_color = Color32::from_gray(153);
        tab.outline_color = Color32::TRANSPARENT;
    }
    style.tab.hovered.corner_radius = egui::CornerRadius::same(11);
    style.tab.hovered.bg_fill = Color32::from_gray(42);
    style.tab.hovered.outline_color = Color32::TRANSPARENT;
    style.tab.hovered.text_color = Color32::WHITE;
    style.separator.color_idle = Color32::from_gray(43);
    style.separator.color_hovered = mix(Color32::from_gray(43), accent, 0.6);
    style.separator.color_dragged = accent;
    style.overlay.selection_color = accent.gamma_multiply(0.25);
    style.buttons.close_tab_color = Color32::from_gray(140);
    style.buttons.close_tab_active_color = Color32::WHITE;
}
pub fn row_visuals(ui: &egui::Ui, r: &Response, selected: bool) -> egui::style::WidgetVisuals {
    let duration = if reduced_motion(ui.ctx()) { 0.0 } else { 0.16 };
    let selection = ui
        .ctx()
        .animate_bool_with_time(r.id.with("row_selection"), selected, duration);
    let hover = ui
        .ctx()
        .animate_bool_with_time(r.id.with("row_hover"), r.hovered(), duration);
    let mut visuals = *ui.style().interact(r);
    visuals.bg_fill = mix(
        ui.visuals().widgets.inactive.bg_fill,
        ui.visuals().widgets.hovered.bg_fill,
        hover,
    );
    visuals.bg_fill = mix(visuals.bg_fill, ui.visuals().selection.bg_fill, selection);
    visuals.fg_stroke.color = mix(ui.visuals().text_color(), accent(ui.ctx()), selection);
    visuals
}
pub fn feedback(ui: &egui::Ui, r: &Response) {
    let duration = if reduced_motion(ui.ctx()) { 0.0 } else { 0.14 };
    let hover = ui.ctx().animate_bool_with_time(
        r.id.with("hover_glow"),
        r.hovered() && ui.is_enabled(),
        duration,
    );
    let press = ui.ctx().animate_bool_with_time(
        r.id.with("press_glow"),
        r.is_pointer_button_down_on(),
        duration * 0.65,
    );
    if hover > 0.0 {
        ui.painter()
            .rect_filled(r.rect, 8.0, Color32::from_white_alpha((hover * 8.0) as u8));
    }
    if press > 0.0 {
        ui.painter()
            .rect_filled(r.rect, 8.0, Color32::from_black_alpha((press * 24.0) as u8));
    }
}
pub fn caption(ui: &mut egui::Ui, text: impl Into<egui::RichText>) -> Response {
    ui.label(text.into().small().color(ui.visuals().weak_text_color()))
}
pub fn button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> Response {
    let r = ui.button(text);
    feedback(ui, &r);
    r
}
pub fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) -> Response {
    ui.horizontal_wrapped(|ui| {
        let (rect, track) = ui.allocate_exact_size(Vec2::new(32.0, 18.0), Sense::click());
        let text = ui.add(egui::Label::new(label).sense(Sense::click()));
        let mut response = track.union(text);
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, label)
        });
        let duration = if reduced_motion(ui.ctx()) { 0.0 } else { 0.16 };
        let t = ui
            .ctx()
            .animate_bool_with_time(response.id.with("toggle"), *on, duration);
        let fill = mix(Color32::from_gray(67), accent(ui.ctx()), t);
        ui.painter().rect_filled(rect, 9.0, fill);
        let center = Pos2::new(
            egui::lerp(rect.left() + 9.0..=rect.right() - 9.0, t),
            rect.center().y,
        );
        ui.painter().circle_filled(
            center + Vec2::new(0.0, 0.8),
            7.3,
            Color32::from_black_alpha(40),
        );
        ui.painter().circle_filled(center, 7.0, Color32::WHITE);
        response
    })
    .inner
}
pub fn tool_button(
    ui: &mut egui::Ui,
    label: &str,
    tool: crate::document::Tool,
    selected: bool,
    shortcut: &str,
    size: Vec2,
    icon_only: bool,
) -> Response {
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    r.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    let visuals = row_visuals(ui, &r, false);
    ui.painter().rect(
        rect,
        8.0,
        visuals.bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let text = label;
    let p = ui.painter().with_clip_rect(rect.shrink(3.0));
    icon(
        &p,
        if icon_only {
            rect.center()
        } else {
            Pos2::new(rect.left() + 13.0, rect.center().y)
        },
        tool,
        if selected {
            accent(ui.ctx())
        } else {
            Color32::from_gray(194)
        },
    );
    if !icon_only {
        p.text(
            Pos2::new(rect.left() + 26.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(11.5),
            visuals.text_color(),
        );
    }
    feedback(ui, &r);
    r.on_hover_text(format!("{text} · Atalho: {}", shortcut))
}
fn icon(p: &egui::Painter, center: Pos2, tool: crate::document::Tool, color: Color32) {
    use crate::document::Tool;
    let origin = center - Vec2::splat(7.0);
    let at = |x: f32, y: f32| origin + Vec2::new(x, y);
    let stroke = Stroke::new(1.3_f32, color);
    let line = |a, b| {
        p.line_segment([a, b], stroke);
    };
    let path = |pts: Vec<Pos2>| {
        p.add(egui::Shape::line(pts, stroke));
    };
    match tool {
        Tool::Brush => {
            path(vec![
                at(2.0, 11.0),
                at(9.0, 2.0),
                at(12.0, 4.0),
                at(5.0, 13.0),
                at(2.0, 13.0),
                at(2.0, 11.0),
            ]);
            line(at(7.0, 5.0), at(10.0, 7.0));
        }
        Tool::Eraser => {
            path(vec![
                at(2.0, 8.0),
                at(8.0, 2.0),
                at(13.0, 7.0),
                at(7.0, 13.0),
                at(5.0, 13.0),
                at(2.0, 10.0),
                at(2.0, 8.0),
            ]);
            line(at(5.0, 5.0), at(10.0, 10.0));
        }
        Tool::Fill => {
            path(vec![
                at(1.0, 7.0),
                at(7.0, 1.0),
                at(13.0, 7.0),
                at(7.0, 13.0),
                at(1.0, 7.0),
            ]);
            line(at(3.0, 7.0), at(11.0, 7.0));
            p.circle_filled(at(13.0, 12.0), 1.4, color);
        }
        Tool::Picker => {
            line(at(2.0, 12.0), at(11.0, 3.0));
            line(at(8.0, 2.0), at(13.0, 7.0));
            line(at(2.0, 12.0), at(4.0, 12.0));
        }
        Tool::Line => line(at(2.0, 12.0), at(12.0, 2.0)),
        Tool::Rectangle => {
            p.rect_stroke(
                egui::Rect::from_min_max(at(1.0, 3.0), at(13.0, 12.0)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Tool::Ellipse => {
            p.circle_stroke(center, 5.5, stroke);
        }
        Tool::Zoom => {
            p.circle_stroke(at(6.0, 6.0), 4.5, stroke);
            line(at(9.5, 9.5), at(13.0, 13.0));
            line(at(3.5, 6.0), at(8.5, 6.0));
            line(at(6.0, 3.5), at(6.0, 8.5));
        }
        Tool::Hand => {
            path(vec![
                at(2.0, 8.0),
                at(5.0, 11.0),
                at(5.0, 3.0),
                at(7.0, 3.0),
                at(7.0, 8.0),
                at(7.0, 1.0),
                at(9.0, 1.0),
                at(9.0, 8.0),
                at(9.0, 4.0),
                at(11.0, 4.0),
                at(11.0, 8.0),
                at(12.0, 6.0),
                at(13.0, 6.0),
                at(13.0, 11.0),
                at(10.0, 14.0),
                at(6.0, 14.0),
                at(2.0, 10.0),
            ]);
        }
        Tool::SelectRect => {
            for (a, b) in [
                (at(1.0, 5.0), at(1.0, 1.0)),
                (at(1.0, 1.0), at(5.0, 1.0)),
                (at(9.0, 1.0), at(13.0, 1.0)),
                (at(13.0, 1.0), at(13.0, 5.0)),
                (at(13.0, 9.0), at(13.0, 13.0)),
                (at(13.0, 13.0), at(9.0, 13.0)),
                (at(5.0, 13.0), at(1.0, 13.0)),
                (at(1.0, 13.0), at(1.0, 9.0)),
            ] {
                line(a, b);
            }
        }
        Tool::Lasso => {
            let points = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0 * std::f32::consts::TAU;
                    center + Vec2::new(t.cos() * 6.0, t.sin() * 4.5 - 2.0)
                })
                .collect();
            path(points);
            path(vec![
                at(7.0, 9.5),
                at(4.0, 12.0),
                at(7.0, 14.0),
                at(11.0, 12.0),
            ]);
        }
    }
}

#[cfg(test)]
mod glass_tests {
    use super::*;
    fn callback(rect: egui::Rect, _: f32, _: f32) -> egui::PaintCallback {
        egui::PaintCallback {
            rect,
            callback: std::sync::Arc::new(()),
        }
    }
    #[test]
    fn glass_is_mandatory_and_frame_blur_precedes_tint_only_once() {
        assert_eq!(glass_strength(0.0), 0.35);
        assert_eq!(glass_strength(1.0), 0.85);
        let material = glass_tint(0.0);
        assert!(material.a() < 255);
        let rect = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
        for opacity in [1.0, 0.5] {
            let fill = material.gamma_multiply(opacity);
            let mut shape = egui::Shape::rect_filled(rect, 12, fill);
            frost_frame(&mut shape, material, &callback);
            frost_frame(&mut shape, material, &callback);
            let egui::Shape::Vec(shapes) = shape else {
                panic!("missing glass")
            };
            assert_eq!(shapes.len(), 2);
            assert!(matches!(&shapes[0], egui::Shape::Callback(c) if c.rect == rect));
            assert!(matches!(&shapes[1], egui::Shape::Rect(r) if r.fill == fill && r.rect == rect));
        }
    }
    #[test]
    fn material_processing_preserves_controls_and_opaque_colors() {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
        for fill in [
            Color32::from_gray(44),
            Color32::from_rgb(24, 27, 33),
            Color32::RED,
        ] {
            let mut shape = egui::Shape::rect_filled(rect, 12, fill);
            frost_frame(&mut shape, glass_tint(0.55), &callback);
            assert!(matches!(shape, egui::Shape::Rect(r) if r.fill == fill));
        }
        let mut shape = egui::Shape::mesh(egui::Mesh::default());
        frost_frame(&mut shape, glass_tint(0.55), &callback);
        assert!(matches!(shape, egui::Shape::Mesh(_)));
    }
}
