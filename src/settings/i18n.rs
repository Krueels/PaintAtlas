use serde::{Deserialize, Serialize};

/// Languages supported by the application.
///
/// Keep translation text in this module and use [`text`] for user-facing UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    PortugueseBrazil,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::PortugueseBrazil];

    pub const fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::PortugueseBrazil => "Português (Brasil)",
        }
    }
}

/// Resolve a stable translation key to the selected language.
/// Missing translations deliberately fall back to English.
pub fn text(language: Language, key: &'static str) -> &'static str {
    match (language, key) {
        (Language::PortugueseBrazil, "settings.language") => "Idioma",
        (Language::PortugueseBrazil, "settings.language_hint") => {
            "O idioma é aplicado imediatamente e salvo nas preferências."
        }
        (Language::PortugueseBrazil, "settings.cursor") => "Cursor",
        (Language::PortugueseBrazil, "settings.rendering") => "Renderização",
        (Language::PortugueseBrazil, "settings.colors") => "Cores",
        (Language::PortugueseBrazil, "settings.appearance") => "Aparência",
        (Language::PortugueseBrazil, "settings.shortcuts") => "Atalhos",
        (Language::PortugueseBrazil, "settings.updates") => "Atualizações",
        (Language::PortugueseBrazil, "settings.brush_eraser") => "Pincel e borracha",
        (Language::PortugueseBrazil, "settings.cursor_type") => "Tipo de cursor",
        (Language::PortugueseBrazil, "settings.graphics_device") => "Dispositivo gráfico",
        (Language::PortugueseBrazil, "settings.color_selector") => "Seletor de cor",
        (Language::PortugueseBrazil, "settings.accent_color") => "Cor de destaque",
        (Language::PortugueseBrazil, "settings.glass_intensity") => "Intensidade do vidro",
        (Language::PortugueseBrazil, "settings.motion") => "Movimento",
        (Language::PortugueseBrazil, "settings.reduce_motion") => "Reduzir movimento",
        (Language::PortugueseBrazil, "settings.restore_appearance") => "Restaurar aparência padrão",
        (Language::PortugueseBrazil, "settings.saved_hint") => {
            "Preferências salvas automaticamente. Aparência e cursor mudam imediatamente."
        }
        (Language::PortugueseBrazil, "settings.english") => "Inglês",
        (Language::PortugueseBrazil, "settings.portuguese_brazil") => "Português (Brasil)",
        (Language::PortugueseBrazil, "app.file") => "Arquivo",
        (Language::PortugueseBrazil, "app.image") => "Imagem",
        (Language::PortugueseBrazil, "app.filters") => "Filtros",
        (Language::PortugueseBrazil, "app.edit") => "Edição",
        (Language::PortugueseBrazil, "app.tools") => "Ferramentas",
        (Language::PortugueseBrazil, "app.brush") => "Pincel",
        (Language::PortugueseBrazil, "app.canvas") => "Canvas",
        (Language::PortugueseBrazil, "app.selection") => "Seleção",
        (Language::PortugueseBrazil, "app.transform") => "Transformação",
        (Language::PortugueseBrazil, "app.help_description") => {
            "Um estúdio de desenho raster multiplataforma, feito em Rust."
        }
        (Language::PortugueseBrazil, "app.help_navigation") => {
            "Roda: zoom · botão do meio: mover vista · abas: trocar canvas"
        }
        (Language::PortugueseBrazil, "app.quick_export") => "Exportar PNG rápido",
        (Language::PortugueseBrazil, "app.settings_button") => "Configurações",
        (Language::PortugueseBrazil, "app.brushes") => "Pincéis",
        (Language::PortugueseBrazil, "app.navigation") => "NAVEGAÇÃO",
        (Language::PortugueseBrazil, "tool.brush") => "Pincel",
        (Language::PortugueseBrazil, "tool.eraser") => "Borracha",
        (Language::PortugueseBrazil, "tool.fill") => "Preencher",
        (Language::PortugueseBrazil, "tool.picker") => "Conta-gotas",
        (Language::PortugueseBrazil, "tool.line") => "Linha",
        (Language::PortugueseBrazil, "tool.rectangle") => "Retângulo",
        (Language::PortugueseBrazil, "tool.ellipse") => "Elipse",
        (Language::PortugueseBrazil, "tool.hand") => "Mover tela",
        (Language::PortugueseBrazil, "tool.zoom") => "Zoom",
        (Language::PortugueseBrazil, "ui.pressure_value") => "Pressão: {}",
        (Language::PortugueseBrazil, "tool.selection") => "Seleção retangular",
        (Language::PortugueseBrazil, "tool.lasso") => "Laço de seleção",
        (Language::PortugueseBrazil, "panel.tools") => "Ferramentas",
        (Language::PortugueseBrazil, "panel.canvas") => "Canvas",
        (Language::PortugueseBrazil, "panel.properties") => "Propriedades",
        (Language::PortugueseBrazil, "panel.brushes") => "Pincéis",
        (Language::PortugueseBrazil, "panel.colors") => "Cores",
        (Language::PortugueseBrazil, "panel.layers") => "Camadas",
        (Language::PortugueseBrazil, "panel.navigation") => "Navegação",
        (Language::PortugueseBrazil, "panel.brush_sizes") => "Tamanhos de pincel",
        (Language::PortugueseBrazil, "panel.files") => "Arquivos",
        (Language::PortugueseBrazil, "panel.help") => "Ajuda",
        (Language::PortugueseBrazil, "panel.settings") => "Configurações",
        (Language::PortugueseBrazil, "placement.left") => "À esquerda",
        (Language::PortugueseBrazil, "placement.right") => "À direita",
        (Language::PortugueseBrazil, "placement.above") => "Acima",
        (Language::PortugueseBrazil, "placement.below") => "Abaixo",
        (Language::PortugueseBrazil, "placement.tab") => "Como aba do canvas",
        (Language::PortugueseBrazil, "ui.properties") => "PROPRIEDADES",
        (Language::PortugueseBrazil, "ui.brushes_section") => "PINCÉIS",
        (Language::PortugueseBrazil, "ui.brush_sizes_hint") => "Abrir a paleta com 1.000 tamanhos",
        (Language::PortugueseBrazil, "ui.modified_brush") => "Pincel modificado",
        (Language::PortugueseBrazil, "ui.save_as_preset") => "Salvar como novo preset…",
        (Language::PortugueseBrazil, "ui.brush_sizes") => "Tamanhos de pincel…",
        (Language::PortugueseBrazil, "ui.opacity") => "Opacidade",
        (Language::PortugueseBrazil, "ui.hardness") => "Dureza",
        (Language::PortugueseBrazil, "ui.stamp_spacing") => "Espaçamento entre marcas",
        (Language::PortugueseBrazil, "ui.stabilization") => "Estabilização",
        (Language::PortugueseBrazil, "ui.stabilization_hint") => {
            "0%: traço direto. 100%: suavização forte, com cerca de 240 ms de atraso. A linha guia conecta a caneta à ponta estabilizada."
        }
        (Language::PortugueseBrazil, "ui.pen_dynamics") => "Dinâmica da caneta",
        (Language::PortugueseBrazil, "ui.pressure_size") => "Pressão controla o tamanho",
        (Language::PortugueseBrazil, "ui.pressure_curve") => "Curva: leve ← → firme",
        (Language::PortugueseBrazil, "ui.minimum_size") => "Tamanho mínimo",
        (Language::PortugueseBrazil, "ui.fill_tolerance") => "Tolerância",
        (Language::PortugueseBrazil, "ui.fill_active_layer") => "Preenche a camada ativa.",
        (Language::PortugueseBrazil, "app.new") => "Novo…",
        (Language::PortugueseBrazil, "app.open") => "Abrir…",
        (Language::PortugueseBrazil, "app.close_document") => "Fechar documento",
        (Language::PortugueseBrazil, "app.close_all") => "Fechar todos os documentos",
        (Language::PortugueseBrazil, "app.save") => "Salvar projeto",
        (Language::PortugueseBrazil, "app.save_as") => "Salvar como…",
        (Language::PortugueseBrazil, "app.export_png") => "Exportar PNG rápido…",
        (Language::PortugueseBrazil, "app.export_as") => "Exportar como…",
        (Language::PortugueseBrazil, "app.copy_layer") => "Copiar camada/seleção",
        (Language::PortugueseBrazil, "app.copy_merged") => "Copiar mesclado",
        (Language::PortugueseBrazil, "app.paste_image") => "Colar imagem",
        (Language::PortugueseBrazil, "app.window") => "Janela",
        (Language::PortugueseBrazil, "app.restore_layout") => "Restaurar layout padrão",
        (Language::PortugueseBrazil, "app.dock_help") => {
            "Arraste as abas para acoplar. Clique com o botão direito para mais opções."
        }
        (Language::PortugueseBrazil, "app.undo") => "Desfazer",
        (Language::PortugueseBrazil, "app.redo") => "Refazer",
        (Language::PortugueseBrazil, "app.undo_hint") => "{count} ações disponíveis para desfazer",
        (Language::PortugueseBrazil, "app.redo_hint") => "{count} ações disponíveis para refazer",
        (Language::PortugueseBrazil, "app.help") => "Ajuda",
        (Language::PortugueseBrazil, "app.layer_count") => "{} camadas",
        (Language::PortugueseBrazil, "settings.render_backend") => "Backend de renderização",
        (Language::PortugueseBrazil, "settings.gpu_opengl") => {
            "Placa de vídeo para renderização OpenGL"
        }
        (Language::PortugueseBrazil, "settings.diagnostics") => "Diagnóstico",
        (Language::PortugueseBrazil, "settings.active_renderer") => "Renderizador ativo:",
        (Language::PortugueseBrazil, "settings.restart_backend") => {
            "Reinicie o PaintAtlas para aplicar o backend de renderização."
        }
        (Language::PortugueseBrazil, "settings.vulkan_note") => {
            "Vulkan usa wgpu. Efeitos exclusivos do OpenGL (desfoque de vidro e composição personalizada) ficam indisponíveis."
        }
        (Language::PortugueseBrazil, "status.brush_created") => "Pincel criado: {}",
        (Language::PortugueseBrazil, "status.create_canvas_error") => {
            "Não foi possível criar a tela:"
        }
        (Language::PortugueseBrazil, "status.history_limit") => {
            "Limite de memória do histórico atingido; esta edição não poderá ser desfeita."
        }
        (Language::PortugueseBrazil, "status.open_success") => "Arquivos abertos em abas",
        (Language::PortugueseBrazil, "status.open_error") => "Erro ao abrir: {}",
        (Language::PortugueseBrazil, "status.save_before_image") => {
            "Aplique ou cancele a operação de imagem antes de salvar."
        }
        (Language::PortugueseBrazil, "status.save_before_transform") => {
            "Aplique ou cancele a transformação antes de salvar."
        }
        (Language::PortugueseBrazil, "status.project_saved") => "Projeto salvo: {}",
        (Language::PortugueseBrazil, "status.save_error") => "Erro ao salvar: {}",
        (Language::PortugueseBrazil, "status.export_before_image") => {
            "Aplique ou cancele a operação de imagem antes de exportar."
        }
        (Language::PortugueseBrazil, "status.png_exported") => "PNG exportado: {}",
        (Language::PortugueseBrazil, "status.export_error") => "Erro ao exportar: {}",
        (Language::PortugueseBrazil, "status.nothing_to_undo") => "Nada para desfazer.",
        (Language::PortugueseBrazil, "status.nothing_to_redo") => "Nada para refazer.",
        (Language::PortugueseBrazil, "status.paste_error") => "Erro ao colar: {}",
        (Language::PortugueseBrazil, "status.undo_done") => {
            "Desfeito · {} para desfazer · {} para refazer"
        }
        (Language::PortugueseBrazil, "status.redo_done") => {
            "Refeito · {} para desfazer · {} para refazer"
        }
        (_, "settings.language") => "Language",
        (_, "settings.language_hint") => {
            "The language is applied immediately and saved in preferences."
        }
        (_, "settings.cursor") => "Cursor",
        (_, "settings.rendering") => "Rendering",
        (_, "settings.colors") => "Colors",
        (_, "settings.appearance") => "Appearance",
        (_, "settings.shortcuts") => "Shortcuts",
        (_, "settings.updates") => "Updates",
        (_, "settings.brush_eraser") => "Brush and Eraser",
        (_, "settings.cursor_type") => "Cursor Type",
        (_, "settings.graphics_device") => "Graphics Device",
        (_, "settings.color_selector") => "Color Selector",
        (_, "settings.accent_color") => "Accent Color",
        (_, "settings.glass_intensity") => "Glass Intensity",
        (_, "settings.motion") => "Motion",
        (_, "settings.reduce_motion") => "Reduce Motion",
        (_, "settings.restore_appearance") => "Restore Default Appearance",
        (_, "settings.saved_hint") => {
            "Preferences are saved automatically. Appearance and cursor update immediately."
        }
        (_, "settings.english") => "English",
        (_, "settings.portuguese_brazil") => "Português (Brasil)",
        (_, "app.file") => "File",
        (_, "app.image") => "Image",
        (_, "app.filters") => "Filters",
        (_, "app.edit") => "Edit",
        (_, "app.tools") => "Tools",
        (_, "app.brush") => "Brush",
        (_, "app.canvas") => "Canvas",
        (_, "app.selection") => "Selection",
        (_, "app.transform") => "Transform",
        (_, "app.help_description") => "A cross-platform raster drawing studio built with Rust.",
        (_, "app.help_navigation") => "Wheel: zoom · middle button: pan · tabs: switch canvas",
        (_, "app.quick_export") => "Quick PNG Export",
        (_, "app.settings_button") => "Settings",
        (_, "app.brushes") => "Brushes",
        (_, "app.navigation") => "NAVIGATION",
        (_, "tool.brush") => "Brush",
        (_, "tool.eraser") => "Eraser",
        (_, "tool.fill") => "Fill",
        (_, "tool.picker") => "Eyedropper",
        (_, "tool.line") => "Line",
        (_, "tool.rectangle") => "Rectangle",
        (_, "tool.ellipse") => "Ellipse",
        (_, "tool.hand") => "Pan",
        (_, "tool.zoom") => "Zoom",
        (_, "ui.pressure_value") => "Pressure: {}",
        (_, "tool.selection") => "Rectangular Selection",
        (_, "tool.lasso") => "Lasso Selection",
        (_, "panel.tools") => "Tools",
        (_, "panel.canvas") => "Canvas",
        (_, "panel.properties") => "Properties",
        (_, "panel.brushes") => "Brushes",
        (_, "panel.colors") => "Colors",
        (_, "panel.layers") => "Layers",
        (_, "panel.navigation") => "Navigation",
        (_, "panel.brush_sizes") => "Brush Sizes",
        (_, "panel.files") => "Files",
        (_, "panel.help") => "Help",
        (_, "panel.settings") => "Settings",
        (_, "placement.left") => "Left",
        (_, "placement.right") => "Right",
        (_, "placement.above") => "Above",
        (_, "placement.below") => "Below",
        (_, "placement.tab") => "As a canvas tab",
        (_, "ui.properties") => "PROPERTIES",
        (_, "ui.brushes_section") => "BRUSHES",
        (_, "ui.brush_sizes_hint") => "Open the 1,000-size palette",
        (_, "ui.modified_brush") => "Modified Brush",
        (_, "ui.save_as_preset") => "Save as New Preset…",
        (_, "ui.brush_sizes") => "Brush Sizes…",
        (_, "ui.opacity") => "Opacity",
        (_, "ui.hardness") => "Hardness",
        (_, "ui.stamp_spacing") => "Stamp Spacing",
        (_, "ui.stabilization") => "Stabilization",
        (_, "ui.stabilization_hint") => {
            "0%: direct stroke. 100%: strong smoothing with about 240 ms of delay. The guide connects the pen to the stabilized tip."
        }
        (_, "ui.pen_dynamics") => "Pen Dynamics",
        (_, "ui.pressure_size") => "Pressure Controls Size",
        (_, "ui.pressure_curve") => "Curve: soft ← → firm",
        (_, "ui.minimum_size") => "Minimum Size",
        (_, "ui.fill_tolerance") => "Tolerance",
        (_, "ui.fill_active_layer") => "Fill the active layer.",
        (_, "app.new") => "New…",
        (_, "app.open") => "Open…",
        (_, "app.close_document") => "Close Document",
        (_, "app.close_all") => "Close All Documents",
        (_, "app.save") => "Save Project",
        (_, "app.save_as") => "Save As…",
        (_, "app.export_png") => "Quick PNG Export…",
        (_, "app.export_as") => "Export As…",
        (_, "app.copy_layer") => "Copy Layer/Selection",
        (_, "app.copy_merged") => "Copy Merged",
        (_, "app.paste_image") => "Paste Image",
        (_, "app.window") => "Window",
        (_, "app.restore_layout") => "Restore Default Layout",
        (_, "app.dock_help") => "Drag tabs to dock. Right-click for more options.",
        (_, "app.undo") => "Undo",
        (_, "app.redo") => "Redo",
        (_, "app.undo_hint") => "{count} undo actions available",
        (_, "app.redo_hint") => "{count} redo actions available",
        (_, "app.help") => "Help",
        (_, "app.layer_count") => "{} layers",
        (_, "settings.render_backend") => "Rendering Backend",
        (_, "settings.gpu_opengl") => "Graphics card for OpenGL rendering",
        (_, "settings.diagnostics") => "Diagnostics",
        (_, "settings.active_renderer") => "Active renderer:",
        (_, "settings.restart_backend") => "Restart PaintAtlas to apply the rendering backend.",
        (_, "settings.vulkan_note") => {
            "Vulkan uses wgpu. OpenGL-only effects (glass blur and custom compositing) are unavailable."
        }
        (_, "status.brush_created") => "Brush created: {}",
        (_, "status.create_canvas_error") => "Could not create canvas:",
        (_, "status.history_limit") => "History memory limit reached; this edit cannot be undone.",
        (_, "status.open_success") => "Files opened in tabs",
        (_, "status.open_error") => "Error opening: {}",
        (_, "status.save_before_image") => "Apply or cancel the image operation before saving.",
        (_, "status.save_before_transform") => "Apply or cancel the transform before saving.",
        (_, "status.project_saved") => "Project saved: {}",
        (_, "status.save_error") => "Error saving: {}",
        (_, "status.export_before_image") => {
            "Apply or cancel the image operation before exporting."
        }
        (_, "status.png_exported") => "PNG exported: {}",
        (_, "status.export_error") => "Error exporting: {}",
        (_, "status.nothing_to_undo") => "Nothing left to undo.",
        (_, "status.nothing_to_redo") => "Nothing left to redo.",
        (_, "status.paste_error") => "Paste error: {}",
        (_, "status.undo_done") => "Undone · {} undo steps · {} redo steps",
        (_, "status.redo_done") => "Redone · {} undo steps · {} redo steps",
        // During migration, untranslated keys remain usable in English.
        (_, key) => key,
    }
}

/// Map existing shortcut categories to stable translation keys while old
/// preference files and shortcut definitions are migrated.
pub fn category_key(category: &str) -> Option<&'static str> {
    match category {
        "File" | "Arquivo" => Some("app.file"),
        "Image" | "Imagem" => Some("app.image"),
        "Filters" | "Filtros" => Some("app.filters"),
        "Edit" | "Edição" => Some("app.edit"),
        "Tools" | "Ferramentas" => Some("app.tools"),
        "Brush" | "Pincel" => Some("app.brush"),
        "Canvas" => Some("app.canvas"),
        "Selection" | "Seleção" => Some("app.selection"),
        "Transform" | "Transformação" => Some("app.transform"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portuguese_has_native_name_and_falls_back_for_unknown_keys() {
        assert_eq!(
            Language::PortugueseBrazil.native_name(),
            "Português (Brasil)"
        );
        assert_eq!(
            text(Language::PortugueseBrazil, "settings.cursor"),
            "Cursor"
        );
        assert_eq!(
            text(Language::PortugueseBrazil, "not.translated"),
            "not.translated"
        );
    }

    #[test]
    fn core_menu_translations_are_complete_and_help_is_cross_platform() {
        for key in [
            "app.file",
            "app.edit",
            "app.new",
            "app.open",
            "app.save",
            "app.export_as",
            "app.undo",
            "app.redo",
            "app.help",
            "settings.render_backend",
            "settings.restart_backend",
        ] {
            assert_ne!(text(Language::English, key), key, "missing English: {key}");
            assert_ne!(
                text(Language::PortugueseBrazil, key),
                key,
                "missing Portuguese: {key}"
            );
        }
        assert!(text(Language::English, "app.help_description").contains("cross-platform"));
        assert!(
            text(Language::PortugueseBrazil, "app.help_description").contains("multiplataforma")
        );
    }
}
