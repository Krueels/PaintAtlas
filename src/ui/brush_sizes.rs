//! Dockable brush-size palette. Rows are virtualized: 1,000 choices do not mean
//! 1,000 widgets painted on every frame.
use crate::painting::MAX_BRUSH_SIZE;
use eframe::egui::{self, Pos2, Sense, Vec2};

pub struct BrushSizes {
    locate: bool,
}
impl Default for BrushSizes {
    fn default() -> Self {
        Self { locate: true }
    }
}
impl BrushSizes {
    pub fn locate_current(&mut self) {
        self.locate = true;
    }
    pub fn contents(&mut self, ui: &mut egui::Ui, size: &mut f32) {
        let compact = ui.available_width() < 240.0 || ui.available_height() < 200.0;
        if !compact {
            ui.heading(crate::i18n::ui(ui.ctx(), "Brush Sizes"));
        }
        ui.horizontal_wrapped(|ui| {
            if !compact {
                ui.label(crate::i18n::ui(ui.ctx(), "Current"));
            }
            ui.add(
                egui::DragValue::new(size)
                    .range(1.0..=MAX_BRUSH_SIZE)
                    .speed(1.0)
                    .max_decimals(0)
                    .suffix(" px"),
            );
            if ui
                .button(crate::i18n::ui(
                    ui.ctx(),
                    if compact { "↕" } else { "Locate" },
                ))
                .on_hover_text(crate::i18n::ui(
                    ui.ctx(),
                    "Show the current size on the grid",
                ))
                .clicked()
            {
                self.locate = true;
            }
        });
        if !compact {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(crate::i18n::ui(
                        ui.ctx(),
                        "1 to 1000 px · circles shown at an illustrative scale",
                    ))
                    .small(),
                )
                .wrap(),
            );
        }
        ui.separator();
        let cell = Vec2::new(52.0_f32.min((ui.available_width() - 12.0).max(24.0)), 62.0);
        let gap = Vec2::new(6.0, 6.0);
        let columns = ((ui.available_width() - 12.0 + gap.x) / (cell.x + gap.x))
            .floor()
            .max(1.0) as usize;
        let rows = (MAX_BRUSH_SIZE as usize).div_ceil(columns);
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("size_grid")
            .min_scrolled_height(0.0)
            .min_scrolled_width(0.0)
            .auto_shrink([false, false]);
        if self.locate {
            let index = size.round().clamp(1.0, MAX_BRUSH_SIZE) as usize - 1;
            scroll = scroll.vertical_scroll_offset(
                (index / columns).saturating_sub(3) as f32 * (cell.y + gap.y),
            );
            self.locate = false;
        }
        ui.spacing_mut().item_spacing = gap;
        scroll.show_rows(ui, cell.y, rows, |ui, range| {
            ui.spacing_mut().item_spacing = gap;
            let mut targets = Vec::new();
            for row in range {
                ui.horizontal(|ui| {
                    for column in 0..columns {
                        let value = row * columns + column + 1;
                        if value > MAX_BRUSH_SIZE as usize {
                            break;
                        }
                        ui.push_id(value, |ui| {
                            let selected = (*size - value as f32).abs() < 0.5;
                            let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    selected,
                                    format!("{value} pixels"),
                                )
                            });
                            targets.push((value, rect));
                            let visuals = crate::theme::row_visuals(ui, &response, false);
                            let color = if selected {
                                crate::theme::accent(ui.ctx())
                            } else {
                                visuals.text_color()
                            };
                            ui.painter().rect(
                                rect,
                                9.0,
                                visuals.bg_fill,
                                visuals.bg_stroke,
                                egui::StrokeKind::Inside,
                            );
                            let radius = 1.0 + 17.0 * (value as f32).ln() / MAX_BRUSH_SIZE.ln();
                            ui.painter().circle_filled(
                                Pos2::new(rect.center().x, rect.min.y + 22.0),
                                radius,
                                color,
                            );
                            ui.painter().text(
                                Pos2::new(rect.center().x, rect.max.y - 11.0),
                                egui::Align2::CENTER_CENTER,
                                value.to_string(),
                                egui::FontId::proportional(12.0),
                                color,
                            );
                            if response
                                .on_hover_text(crate::i18n::ui(ui.ctx(), format!("Use {value} px")))
                                .clicked()
                            {
                                *size = value as f32;
                            }
                        });
                    }
                });
            }
            crate::selection_motion::paint(
                ui,
                ui.id().with(("brush_size_selection", columns)),
                targets
                    .iter()
                    .find(|(value, _)| (*size - *value as f32).abs() < 0.5)
                    .map(|(_, rect)| *rect),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn virtual_grid_reaches_1000_and_click_applies_size() {
        let ctx = egui::Context::default();
        let mut palette = BrushSizes::default();
        let mut size = 1000.0;
        let mut frame = |events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(440.0, 580.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| palette.contents(ui, &mut size));
                },
            )
        };
        frame(vec![]);
        frame(vec![]);
        let output = frame(vec![]);
        let center = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "999" => {
                    Some(t.pos + t.galley.rect.size() * 0.5)
                }
                _ => None,
            })
            .expect("Localizar must make the last row visible");
        frame(vec![egui::Event::PointerMoved(center)]);
        frame(vec![egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        assert_eq!(size, 999.0);
    }
}
