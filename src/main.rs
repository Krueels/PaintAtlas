#![cfg_attr(not(test), windows_subsystem = "windows")]

#[path = "ui/app.rs"]
mod app;
#[path = "graphics/blend.rs"]
mod blend;
#[path = "graphics/blend_gpu.rs"]
mod blend_gpu;
#[path = "ui/brush_sizes.rs"]
mod brush_sizes;
#[path = "brush/brush_tip.rs"]
mod brush_tip;
#[path = "io/clipboard.rs"]
mod clipboard;
#[path = "ui/color_selector.rs"]
mod color_selector;
#[path = "document/document.rs"]
mod document;
#[path = "image/filters.rs"]
mod filters;
#[path = "graphics/glass_gpu.rs"]
mod glass_gpu;
#[path = "document/history.rs"]
mod history;
#[path = "settings/i18n.rs"]
mod i18n;
#[path = "io/image_drop.rs"]
mod image_drop;
#[path = "image/image_ops.rs"]
mod image_ops;
#[path = "document/layer_groups.rs"]
mod layer_groups;
#[path = "ui/layer_motion.rs"]
mod layer_motion;
#[path = "brush/painting.rs"]
mod painting;
#[path = "settings/preferences.rs"]
mod preferences;
#[path = "settings/presets.rs"]
mod presets;
#[path = "brush/quickshape.rs"]
mod quickshape;
#[path = "ui/reference.rs"]
mod reference;
#[path = "document/rulers.rs"]
mod rulers;
#[path = "document/selection.rs"]
mod selection;
#[path = "graphics/selection_gpu.rs"]
mod selection_gpu;
#[path = "ui/selection_motion.rs"]
mod selection_motion;
#[path = "settings/shortcut_settings.rs"]
mod shortcut_settings;
#[path = "settings/shortcuts.rs"]
mod shortcuts;
#[path = "input/tablet.rs"]
#[cfg(target_os = "linux")]
mod tablet;
#[path = "input/tablet_windows.rs"]
#[cfg(windows)]
mod tablet;
#[path = "settings/theme.rs"]
mod theme;
#[path = "settings/view.rs"]
mod view;
#[path = "settings/workspace.rs"]
mod workspace;

fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--tablet-diagnostics") {
        let mut tablet = tablet::Tablet::new(eframe::egui::Context::default());
        eprintln!("Diagnóstico por 20 segundos: encoste a caneta e varie a pressão.");
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut previous = -1.0_f32;
        while std::time::Instant::now() < end {
            tablet.pressure();
            if (tablet.current - previous).abs() >= 0.01
                || (tablet.current == 0.0 && previous != 0.0)
            {
                println!("{}: {:.1}%", tablet.name, tablet.current * 100.0);
                previous = tablet.current;
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        return Ok(());
    }
    preferences::configure_gpu();
    let settings = presets::settings_path()
        .and_then(|path| presets::Settings::load(&path).ok())
        .unwrap_or_default();
    let mut options = eframe::NativeOptions {
        renderer: match settings.render_backend {
            presets::RenderBackend::OpenGl => eframe::Renderer::Glow,
            presets::RenderBackend::Vulkan => eframe::Renderer::Wgpu,
        },
        #[cfg(target_os = "linux")]
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            if std::env::var_os("DISPLAY").is_some() {
                builder.with_x11();
            }
        })),
        viewport: eframe::egui::ViewportBuilder::default()
            .with_maximized(true)
            .with_inner_size([1440.0, 940.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    if settings.render_backend == presets::RenderBackend::Vulkan
        && let eframe::egui_wgpu::WgpuSetup::CreateNew(create_new) =
            &mut options.wgpu_options.wgpu_setup
    {
        create_new.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN;
    }
    let viewport = options.viewport.clone();
    options.viewport = match app_icon() {
        Ok(icon) => viewport.with_icon(icon),
        Err(error) => {
            eprintln!("Could not load PaintAtlas application icon: {error}");
            viewport
        }
    };
    eframe::run_native(
        "PaintAtlas",
        options,
        Box::new(|cc| Ok(Box::new(app::PaintAtlas::new(cc)))),
    )
}

fn app_icon() -> Result<eframe::egui::IconData, String> {
    const ICON_SIZE: u32 = 64;
    let rgba = render_official_svg(ICON_SIZE)
        .or_else(|svg_error| {
            eprintln!("Could not render primary SVG icon; using PNG backup: {svg_error}");
            image::load_from_memory(include_bytes!("../assets/PaintAtlas.png"))
                .map(|image| {
                    image
                        .resize_exact(ICON_SIZE, ICON_SIZE, image::imageops::FilterType::Lanczos3)
                        .to_rgba8()
                })
                .map_err(|png_error| format!("SVG: {svg_error}; PNG backup: {png_error}"))
        })?
        .into_raw();
    Ok(eframe::egui::IconData {
        rgba,
        width: ICON_SIZE,
        height: ICON_SIZE,
    })
}

fn render_official_svg(icon_size: u32) -> Result<image::RgbaImage, String> {
    let tree = resvg::usvg::Tree::from_data(
        include_bytes!("../assets/PaintAtlas.svg"),
        &resvg::usvg::Options::default(),
    )
    .map_err(|error| error.to_string())?;
    let size = tree.size();
    let scale = (icon_size as f32 / size.width()).min(icon_size as f32 / size.height());
    let x = (icon_size as f32 - size.width() * scale) * 0.5;
    let y = (icon_size as f32 - size.height() * scale) * 0.5;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(x, y);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(icon_size, icon_size)
        .ok_or_else(|| "could not allocate SVG icon raster".to_owned())?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut rgba = pixmap.data().to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = pixel[3] as u16;
        for channel in &mut pixel[..3] {
            if let Some(value) = (*channel as u16 * 255 + alpha / 2).checked_div(alpha) {
                *channel = value.min(255) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(icon_size, icon_size, rgba)
        .ok_or_else(|| "SVG renderer returned invalid icon pixels".to_owned())
}

#[cfg(test)]
mod app_icon_tests {
    use super::*;

    #[test]
    fn official_svg_renders_to_a_valid_native_icon() {
        let icon = app_icon().expect("official PaintAtlas SVG should render");
        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.rgba.len(), (64 * 64 * 4) as usize);
        assert!(
            icon.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] > 0)
        );
    }
}

// Test the exact native URI parser without winit's unrelated example/dev dependencies.
#[cfg(test)]
#[cfg(target_os = "linux")]
#[path = "../vendor/winit/src/platform_impl/linux/x11/paintatlas_drop.rs"]
mod native_drop_parser;
