mod app;
mod blend;
mod blend_gpu;
mod brush_sizes;
mod brush_tip;
mod clipboard;
mod color_selector;
mod document;
mod filters;
mod glass_gpu;
mod history;
mod image_drop;
mod image_ops;
mod layer_groups;
mod layer_motion;
mod painting;
mod preferences;
mod presets;
mod quickshape;
mod reference;
mod rulers;
mod selection;
mod selection_gpu;
mod selection_motion;
mod shortcut_settings;
mod shortcuts;
mod tablet;
mod theme;
mod view;
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
    eframe::run_native(
        "PaintAtlas",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
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
        },
        Box::new(|cc| Ok(Box::new(app::PaintAtlas::new(cc)))),
    )
}

// Test the exact native URI parser without winit's unrelated example/dev dependencies.
#[cfg(test)]
#[path = "../vendor/winit/src/platform_impl/linux/x11/paintatlas_drop.rs"]
mod native_drop_parser;
