# Project architecture

PaintAtlas is a single Rust application. The source tree is grouped by feature responsibility while `main.rs` keeps stable crate-level module names so existing `crate::...` imports remain readable.

## Source map

| Directory | Responsibility |
|---|---|
| `src/ui/` | Application shell and egui panels/dialogs, including selection, filters, import, export, and layer UI. `app.rs` owns these UI submodules. |
| `src/document/` | Document data, layer groups, selection operations, drawing rulers, and undo/redo history. |
| `src/brush/` | Brush-tip library, stroke/pressure processing, and QuickShape geometry recognition. |
| `src/image/` | Image adjustments and pixel filters. |
| `src/graphics/` | GPU shaders and compositing helpers. |
| `src/input/` | Tablet and pen input backends. |
| `src/io/` | Clipboard and drag-and-drop image input. |
| `src/settings/` | Preferences, presets, shortcuts, theme, canvas view, workspace, and localization. |
| `assets/` | Embedded fonts and artwork. |
| `assets/default_brushes/` | Bundled brush-tip images. |
| `vendor/` | Patched third-party dependencies; retain their upstream layout. |

## Module conventions

- Add modules to the responsibility-appropriate directory and register them in `src/main.rs` with an explicit `#[path]`. This preserves stable crate-level names such as `crate::document`.
- UI modules that are private implementation details of the application belong in `src/ui/` and are declared by `src/ui/app.rs`.
- Keep platform-specific input code under `src/input/`; the tablet XInput backend is a child module of `tablet.rs`.
- Resolve `include_bytes!` paths relative to the Rust source file containing the macro. Update these paths when moving a module.
- Persisted settings should use `#[serde(default)]` for newly added fields so existing user preference files continue to load.
- Put user-visible translations in `src/settings/i18n.rs` under stable keys. Keep English as the fallback and add new language variants there rather than embedding locale conditionals throughout UI code.
- The app can run with eframe's OpenGL (`glow`) renderer or the wgpu renderer restricted to Vulkan. Keep custom effects optional: modules under `src/graphics/` currently use OpenGL and are skipped when wgpu is selected.

## Adding a feature

Keep pixel/document operations out of egui panels where practical. Put reusable behavior in the corresponding document, brush, image, input, or graphics module; let UI modules collect input and call that behavior. Add focused unit tests next to the implementation and update this map when introducing a new top-level source area.

## Drawing and reference modules

- `src/document/rulers.rs` owns layer-based symmetry and directional ruler geometry; `src/ui/ruler_ui.rs` provides editing and snapping controls.
- `src/brush/quickshape.rs` recognizes geometric forms. `src/ui/quickshape_ui.rs` manages draw-and-hold input, the animated transition, editing, and the shared stroke undo transaction.
- `src/ui/reference.rs` displays linked reference files. Only paths and per-image view transforms are persisted in preferences; image pixels are not embedded in projects. The active preview loads asynchronously, and remove/clear operations require confirmation.
- `.patlas` v8 adds layer-owned rulers. Versions v1–v7 remain readable, including projects saved before the repository reorganization.
