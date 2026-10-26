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
    pub fn show(&mut self, ui: &mut egui::Ui, shortcuts: &mut Shortcuts) -> bool {
        let mut changed = self.capture_ui(ui.ctx(), shortcuts);
        ui.heading("Atalhos de teclado");
        crate::theme::caption(
            ui,
            "Clique em uma combinação e pressione o novo atalho. Alterações são aplicadas e salvas automaticamente.",
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Buscar");
            ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(160.0));
            egui::ComboBox::from_id_salt("shortcut_category")
                .selected_text(if self.category.is_empty() {
                    "Todas as categorias"
                } else {
                    &self.category
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.category, String::new(), "Todas as categorias");
                    for category in [
                        "File",
                        "Image",
                        "Filters",
                        "Camadas",
                        "Edição",
                        "Ferramentas",
                        "Pincel",
                        "Canvas",
                        "Seleção",
                        "Transformação",
                    ] {
                        ui.selectable_value(&mut self.category, category.into(), category);
                    }
                });
        });
        if crate::theme::button(ui, "Restaurar todos os atalhos padrão").clicked() {
            shortcuts.0.clear();
            changed = true;
        }
        ui.separator();
        let search = self.search.to_lowercase();
        for category in [
            "File",
            "Image",
            "Filters",
            "Camadas",
            "Edição",
            "Ferramentas",
            "Pincel",
            "Canvas",
            "Seleção",
            "Transformação",
        ] {
            if !self.category.is_empty() && self.category != category {
                continue;
            }
            let actions: Vec<_> = Action::ALL
                .iter()
                .copied()
                .filter(|a| {
                    a.category() == category
                        && (search.is_empty()
                            || format!("{} {} {}", a.category(), a.label(), shortcuts.label(*a))
                                .to_lowercase()
                                .contains(&search))
                })
                .collect();
            if actions.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(category)
                .default_open(true)
                .show(ui, |ui| {
                    for action in actions {
                        ui.push_id(action.id(), |ui| {
                            ui.label(egui::RichText::new(action.label()).strong());
                            ui.horizontal_wrapped(|ui| {
                                let list = shortcuts.bindings(action).to_vec();
                                if list.is_empty() {
                                    crate::theme::caption(ui, "Sem atalho");
                                }
                                for (slot, binding) in list.iter().enumerate() {
                                    if ui
                                        .button(binding.label())
                                        .on_hover_text("Alterar combinação")
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
                                        .on_hover_text("Remover este atalho")
                                        .clicked()
                                    {
                                        let mut list = shortcuts.bindings(action).to_vec();
                                        list.remove(slot);
                                        shortcuts.0.insert(action.id().into(), list);
                                        changed = true;
                                        break;
                                    }
                                }
                                if ui.small_button("+ Atalho").clicked() {
                                    self.capture = Some(Capture {
                                        action,
                                        slot: shortcuts.bindings(action).len(),
                                        candidate: None,
                                        last_modifiers: egui::Modifiers::NONE,
                                    });
                                }
                                if ui.small_button("Restaurar").clicked() {
                                    shortcuts.restore(action);
                                    changed = true;
                                }
                            });
                            if action.is_held() {
                                crate::theme::caption(ui, "Manter pressionado durante o gesto");
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
            ui.heading("Definir atalho");
            ui.label(capture.action.label());
            crate::theme::caption(ui, if capture.action.is_held() {
                "Pressione uma tecla ou apenas Ctrl, Alt ou Shift. Depois clique em Usar atalho."
            } else { "Pressione a combinação desejada. Escape também pode ser configurado; para sair, clique em Cancelar." });
            if let Some(binding) = &capture.candidate {
                ui.label(egui::RichText::new(binding.label()).size(22.0));
                let conflicts = shortcuts.conflicts(capture.action, binding);
                if !conflicts.is_empty() {
                    ui.colored_label(ui.visuals().warn_fg_color,
                        format!("Já usado por: {}", conflicts.iter().map(|a| a.label()).collect::<Vec<_>>().join(", ")));
                    crate::theme::caption(ui, "Substituir remove esta combinação das ações em conflito.");
                }
                if ui.button(if conflicts.is_empty() { "Usar atalho" } else { "Substituir conflitos e usar" }).clicked() {
                    changed = shortcuts.assign(capture.action, capture.slot, binding.clone(), true);
                    close = changed;
                }
            } else { ui.label("Aguardando tecla…"); }
            if ui.button("Cancelar").clicked() { close = true; }
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
