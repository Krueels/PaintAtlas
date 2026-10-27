use crate::shortcuts::{Action, Binding, Shortcuts};
use eframe::egui;

#[derive(Default)]
pub struct Editor {
    search: String,
    category: String,
    capture: Option<Capture>,
}
struct Capture {
    action: Action,
    slot: usize,
    candidate: Option<Binding>,
    last_modifiers: egui::Modifiers,
}
impl Editor {
    pub fn cancel_capture(&mut self) {
        self.capture = None;
    }
    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }
    fn categories() -> Vec<&'static str> {
        let mut categories = Vec::new();
        for action in Action::ALL {
            if !categories.contains(&action.category()) {
                categories.push(action.category());
            }
        }
        categories
    }
    pub fn show(&mut self, ui: &mut egui::Ui, shortcuts: &mut Shortcuts) -> bool {
        let mut changed = self.capture_ui(ui.ctx(), shortcuts);
        let categories = Self::categories();
        ui.heading(crate::i18n::ui(ui.ctx(), "Keyboard shortcuts"));
        crate::theme::caption(
            ui,
            crate::i18n::ui(
                ui.ctx(),
                "Click a binding and press the new shortcut. Changes are applied and saved automatically.",
            ),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::ui(ui.ctx(), "Search"));
            ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(160.0));
            egui::ComboBox::from_id_salt("shortcut_category")
                .selected_text(crate::i18n::ui(
                    ui.ctx(),
                    if self.category.is_empty() {
                        "All categories"
                    } else {
                        &self.category
                    },
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.category,
                        String::new(),
                        crate::i18n::ui(ui.ctx(), "All categories"),
                    );
                    for &category in &categories {
                        ui.selectable_value(
                            &mut self.category,
                            category.into(),
                            crate::i18n::ui(ui.ctx(), category),
                        );
                    }
                });
        });
        if crate::theme::button(ui, "Restore all default shortcuts").clicked() {
            shortcuts.0.clear();
            changed = true;
        }
        ui.separator();
        let search = self.search.to_lowercase();
        for &category in &categories {
            if !self.category.is_empty() && self.category != category {
                continue;
            }
            let actions: Vec<_> = Action::ALL
                .iter()
                .copied()
                .filter(|a| {
                    a.category() == category
                        && (search.is_empty()
                            || format!(
                                "{} {} {} {} {}",
                                a.category(),
                                a.label(),
                                crate::i18n::ui(ui.ctx(), a.category()),
                                crate::i18n::ui(ui.ctx(), a.label()),
                                shortcuts.label(*a)
                            )
                            .to_lowercase()
                            .contains(&search))
                })
                .collect();
            if actions.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(crate::i18n::ui(ui.ctx(), category))
                .default_open(true)
                .show(ui, |ui| {
                    for action in actions {
                        ui.push_id(action.id(), |ui| {
                            ui.label(
                                egui::RichText::new(crate::i18n::ui(ui.ctx(), action.label()))
                                    .strong(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                let list = shortcuts.bindings(action).to_vec();
                                if list.is_empty() {
                                    crate::theme::caption(
                                        ui,
                                        crate::i18n::ui(ui.ctx(), "No shortcut"),
                                    );
                                }
                                for (slot, binding) in list.iter().enumerate() {
                                    if ui
                                        .button(crate::i18n::ui(ui.ctx(), binding.label()))
                                        .on_hover_text(crate::i18n::ui(ui.ctx(), "Change binding"))
                                        .clicked()
                                    {
                                        self.capture = Some(Capture {
                                            action,
                                            slot,
                                            candidate: None,
                                            last_modifiers: egui::Modifiers::NONE,
                                        });
                                    }
                                    if ui
                                        .small_button("×")
                                        .on_hover_text(crate::i18n::ui(
                                            ui.ctx(),
                                            "Remove this shortcut",
                                        ))
                                        .clicked()
                                    {
                                        let mut list = shortcuts.bindings(action).to_vec();
                                        list.remove(slot);
                                        shortcuts.0.insert(action.id().into(), list);
                                        changed = true;
                                        break;
                                    }
                                }
                                if ui
                                    .small_button(crate::i18n::ui(ui.ctx(), "+ Shortcut"))
                                    .clicked()
                                {
                                    self.capture = Some(Capture {
                                        action,
                                        slot: shortcuts.bindings(action).len(),
                                        candidate: None,
                                        last_modifiers: egui::Modifiers::NONE,
                                    });
                                }
                                if ui
                                    .small_button(crate::i18n::ui(ui.ctx(), "Reset"))
                                    .clicked()
                                {
                                    shortcuts.restore(action);
                                    changed = true;
                                }
                            });
                            if action.is_held() {
                                crate::theme::caption(
                                    ui,
                                    crate::i18n::ui(ui.ctx(), "Hold during the gesture"),
                                );
                            }
                            ui.add_space(5.0);
                        });
                    }
                });
        }
        changed
    }
    fn capture_ui(&mut self, ctx: &egui::Context, shortcuts: &mut Shortcuts) -> bool {
        let Some(capture) = &mut self.capture else {
            return false;
        };
        // All keyboard input is swallowed while listening. Captured Ctrl+S,
        // Ctrl+V, Escape etc. must never operate on the drawing or a text field.
        ctx.input_mut(|input| {
            let mods = input.modifiers;
            let mut received_key = false;
            input.events.retain(|event| {
                if let Some((key, _, modifiers, pressed)) = crate::shortcuts::key_event(event, mods)
                {
                    if pressed {
                        received_key = true;
                        let mut b = Binding::from_modifiers(modifiers);
                        b.key = Some(key.name().into());
                        capture.candidate = Some(b);
                    }
                    false
                } else {
                    !matches!(event, egui::Event::Text(_))
                }
            });
            let modifiers = Binding::from_modifiers(mods);
            let previous = Binding::from_modifiers(capture.last_modifiers);
            let added_modifier = modifiers.ctrl && !previous.ctrl
                || modifiers.shift && !previous.shift
                || modifiers.alt && !previous.alt
                || modifiers.super_key && !previous.super_key;
            if capture.action.is_held() && !received_key && added_modifier {
                capture.candidate = Some(modifiers);
            }
            capture.last_modifiers = mods;
        });
        let mut close = false;
        let mut changed = false;
        egui::Modal::new(egui::Id::new("shortcut_capture")).show(ctx, |ui| {
            ui.heading(crate::i18n::ui(ui.ctx(), "Set shortcut"));
            ui.label(crate::i18n::ui(ui.ctx(), capture.action.label()));
            crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), if capture.action.is_held() {
                "Press a key or just Ctrl, Alt, or Shift. Then click Use shortcut."
            } else { "Press the desired shortcut. Escape can also be assigned; to leave, click Cancel." }));
            if let Some(binding) = &capture.candidate {
                ui.label(egui::RichText::new(crate::i18n::ui(ui.ctx(), binding.label())).size(22.0));
                let conflicts = shortcuts.conflicts(capture.action, binding);
                if !conflicts.is_empty() {
                    ui.colored_label(ui.visuals().warn_fg_color,
                        crate::i18n::ui(ui.ctx(), format!("Already used by: {}", conflicts.iter().map(|a| a.label()).collect::<Vec<_>>().join(", "))));
                    crate::theme::caption(ui, crate::i18n::ui(ui.ctx(), "Replace removes this binding from the conflicting actions."));
                }
                if ui.button(crate::i18n::ui(ui.ctx(), if conflicts.is_empty() { "Use shortcut" } else { "Replace conflicts and use" })).clicked() {
                    changed = shortcuts.assign(capture.action, capture.slot, binding.clone(), true);
                    close = changed;
                }
            } else { ui.label(crate::i18n::ui(ui.ctx(), "Waiting for a key…")); }
            if ui.button(crate::i18n::ui(ui.ctx(), "Cancel")).clicked() { close = true; }
        });
        if close {
            self.capture = None;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modifier_only_capture_keeps_the_chord_after_modifiers_are_released() {
        let ctx = egui::Context::default();
        let mut editor = Editor {
            capture: Some(Capture {
                action: Action::Pan,
                slot: 0,
                candidate: None,
                last_modifiers: egui::Modifiers::NONE,
            }),
            ..Default::default()
        };
        let mut shortcuts = Shortcuts::default();
        for modifiers in [
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Modifiers::SHIFT,
            egui::Modifiers::NONE,
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    modifiers,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        editor.show(ui, &mut shortcuts);
                    });
                },
            );
        }
        assert_eq!(
            editor
                .capture
                .as_ref()
                .unwrap()
                .candidate
                .as_ref()
                .unwrap()
                .label(),
            "Ctrl + Shift"
        );
    }
    #[test]
    fn capture_swallows_keys_and_reports_the_combination_without_running_it() {
        let ctx = egui::Context::default();
        let mut editor = Editor {
            capture: Some(Capture {
                action: Action::Brush,
                slot: 0,
                candidate: None,
                last_modifiers: egui::Modifiers::NONE,
            }),
            ..Default::default()
        };
        let mut shortcuts = Shortcuts::default();
        let _ = ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers::CTRL,
                events: vec![egui::Event::Key {
                    key: egui::Key::S,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    editor.show(ui, &mut shortcuts);
                });
                assert!(!ctx.input_mut(|i| shortcuts.consume(i, Action::Save)));
            },
        );
        assert_eq!(
            editor
                .capture
                .as_ref()
                .unwrap()
                .candidate
                .as_ref()
                .unwrap()
                .label(),
            "Ctrl + S"
        );
        assert_eq!(shortcuts.label(Action::Brush), "B");
        assert!(editor.capturing());
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    #[test]
    fn every_shortcut_category_is_available_including_rulers_and_quickshape() {
        let categories = Editor::categories();
        assert!(categories.contains(&"Rulers"));
        assert!(categories.contains(&"QuickShape"));
        assert!(
            Action::ALL
                .iter()
                .all(|a| categories.contains(&a.category()))
        );
    }
}
