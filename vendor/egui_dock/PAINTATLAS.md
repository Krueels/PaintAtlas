Local copy of egui_dock 0.18.0 (MIT).

PaintAtlas adds TabViewer::min_window_size and applies the maximum requested
minimum to floating windows. Upstream uses egui Window's hardcoded 96 x 32
minimum, with no hook to configure it. No serialized layout types are changed.

Stroke width literals are explicitly typed as f32 for compatibility with newer Rust compilers.
