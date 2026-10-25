# PaintAtlas

A raster drawing studio for Linux, built in **Rust**, with a Portuguese-language interface. A personal project inspired by Clip Studio Paint's illustration workflow: pressure-sensitive brushes, layers, selections, and transforms, with dockable panels and translucent glass styling.

[Source code on GitHub](https://github.com/Krueels/PaintAtlas)

![PaintAtlas startup screen with original artwork by Krueels](docs/splash.png)

Startup screen featuring original artwork by [Krueels](https://github.com/Krueels).

## Build and run

Requirements: stable Rust with Cargo, a C compiler/linker, a Linux desktop session, and OpenGL-capable graphics drivers. File dialogs use `xdg-desktop-portal` and your desktop's portal backend; GTK is not required to build. Install the system `curl` utility to import images from URLs.

From the project directory:

```bash
./compilar.sh
./target/debug/paintatlas
```

Or run directly with Cargo:

```bash
cargo run
```

For an optimized build:

```bash
./compilar.sh --release
./target/release/paintatlas
```

Keep `default_brushes/` next to the executable when distributing a local copy with the brush tip pack. The splash background and Inter font are embedded in the executable.

## AppImage (Linux x86_64)

Build an optimized portable package with:

```bash
./gerar-appimage.sh
```

The executable package, SHA256 checksum, and build information are written to `dist/`. The script downloads and caches [linuxdeploy](https://github.com/linuxdeploy/linuxdeploy) under `target/appimage-tools/`; it does not install system-wide tools. Use `--skip-build` to package an existing release executable. The package includes the default brush pack, Inter license, curl for browser image downloads, and the libraries loaded dynamically for X11, Wayland, and tablet input. User settings remain in the usual user configuration directory.

Launch the result directly, or without FUSE using:

```bash
./dist/PaintAtlas-0.1.0-x86_64.AppImage --appimage-extract-and-run
```

This local build uses the build machine's glibc baseline and the destination's graphics drivers. It is intended for the current CachyOS installation and compatible systems; compatibility with older distributions is not guaranteed. For broad distribution, build on an older supported Linux base. Packaging tools and generated packages are ignored by Git. The app's built-in Inter fallback is included; a locally installed SF Pro font is not copied into the package.

## Start drawing

The app opens maximized with a splash screen and a centered canvas creation menu. Choose the width, height, DPI, and paper color, open a project or image, or press **Ctrl+V** to create a canvas from a copied image at its original size, preserving transparency. The **Paste image** button does the same. Custom paste shortcuts are also respected. UI labels in this README are translated for reference; the app itself currently uses Portuguese.

Canvas creation settings are saved for future canvases. New canvases start with black as the drawing color. Multiple documents can be opened in tabs; closing the last one returns to the splash screen.

Save as **`.patlas`** to preserve layers, groups, and editable group effects. Exporting an image creates a flattened copy and does not replace the project.

### Export images

- **File → Export PNG quickly** (**Ctrl+E**): save the visible composition directly as PNG, including transparency and document DPI. Hide the paper layer when you want a transparent background.
- **File → Export as** (**Ctrl+Alt+Shift+E**): open a movable, resizable window with a checkerboard preview and the encoded file size. Choose PNG, JPEG, or lossless WebP; adjust JPEG quality, transparency, background color, scale, dimensions, aspect locking, and resampling. PNG and JPEG can include print DPI metadata.

Export settings affect only the exported copy. Encoding runs in the background, and the loading indicator has reserved space so controls stay in place. The JPEG preview shows the actual compression result. ICC profile conversion and embedding are not supported.

## Features

The navigation panel's **Fit** command smoothly animates both zoom and centering. Reduced Motion applies the view immediately.

### Drawing and brushes

- Brush and eraser controls for size, opacity, hardness, spacing, stabilization, and pressure dynamics.
- Textured tips from `default_brushes/` and support for importing brush tip images.
- Preset list with stroke previews, brush creation, and an option to save adjustments as a new preset.
- Brush size palette from **1 to 1000 px**, with buttons and circular previews.
- Brush outline, circle, or dot cursor. Pressure-based resizing is optional for the circle cursor and disabled by default.
- Contiguous fill with tolerance, eyedropper, lines, rectangles, and ellipses.
- Filling smoothly reveals the new color in an expanding circle from the clicked point, limited to the actual filled region. OpenGL animates the reveal before layer opacity and blending, including grouped layers. The document commits one undo action; no animation is baked into pixels. Reduced-motion settings show the final result immediately.
- Hold and drag the eyedropper (or Alt with a drawing tool) to sample continuously through an animated pixel magnifier, with a central pixel marker, exact hexadecimal color, and softly feathered edges. The magnifier respects reduced-motion settings.
- Rectangular and triangular HSV color selectors that adapt to the panel size.

Stabilization ranges from 0% to 100%. Higher values introduce an intentional delay to smooth the stroke; a guide between the pen and brush tip shows the effect. Pressure dynamics let you adjust the pressure curve and minimum size or disable thickness variation.

Choose the brush mask under **Properties → Tip / texture**. Transparent tips use their alpha channel; dark regions represent ink in opaque images. Imported tips are copied to `brush_tips/` alongside the preferences, and custom presets persist between sessions.

### Layers and blend modes

Layers support thumbnails, names, visibility, opacity, alpha lock, duplication, and deletion. Drag a thumbnail or layer name to reorder. Right-click a layer to rename, duplicate, delete, merge down, or assign an organizational color. Merge down is available for two visible layers in Normal mode; explicit group rasterization consolidates its children into pixels. Animated feedback follows the drag and can be disabled with **Reduce motion**.

The selector above layer opacity offers 18 modes: Normal, Multiply, Screen, Overlay, Soft Light, Hard Light, Darken, Lighten, Color Dodge, Color Burn, Difference, Exclusion, Add, Subtract, Hue, Saturation, Color, and Luminosity. Blend mode changes can be undone.

**Alpha lock** lets you recolor existing pixels without changing their alpha. Erasing and clearing are blocked while it is enabled.

### Selections and transforms

Rectangular and lasso selections support adding, subtracting, and inverting areas. Brushes, erasers, shapes, and fills respect the selection. A contextual toolbar below the selection provides transform, duplicate selection content, invert, delete, and deselect actions.

**Ctrl+T** transforms the current selection, or the active layer's content bounds when no selection exists. Transforming a whole layer preserves its transparency-lock setting. Canceling restores the previous selection state and the original pixels.

During a transform, drag inside to move, drag handles to resize, and drag outside the corners to rotate. Corner handles preserve proportions by default; Shift toggles this behavior and also enables rotation snapping. While moving, hold Shift to constrain the drag to horizontal, vertical, or 45° diagonal directions; release it to move freely. These modifiers can be reassigned in Settings. Horizontal/vertical flipping and reset controls are available. **Enter** applies a single history action; **Esc** cancels. The preview avoids repeatedly resampling the pixels.

Selections are temporary. Permanent layer masks are not available yet.

### Copy, paste, and import images

**Ctrl+C** copies the active layer's selected region or the entire layer. **Ctrl+Shift+C** copies the visible composite, restricted to the selection when present. Copy a region, switch tabs, and paste to transfer content between canvases.

On an open canvas, **Ctrl+V** starts a provisional new layer: move, resize, or rotate it, then apply or cancel. Large images are scaled down to fit the editing area. On the splash screen, pasting creates a canvas with the image's original dimensions.

Drag images from a file manager or browser to import them. Supported formats are PNG, JPEG, WebP, BMP, and the first frame of GIF files. Drag-and-drop import accepts up to eight images at a time and 32 MiB per file; each image waits for the previous one to be applied or canceled. Downloads and decoding run outside the UI thread, and canceling stops a pending download.

The browser must provide a file or an HTTP/HTTPS or `data:image` image URL. For `blob:` URLs and sites requiring authentication or blocking downloads, use **Copy image** in the browser and paste into the app. Dragging does not change the system clipboard.

### Layer groups and multiple selection

Use **Ctrl+click** to toggle layers and **Shift+click** to select a range. **Group** (**Ctrl+G**) creates an isolated group from selected siblings; groups can nest, expand/collapse, move with their children, duplicate, and receive organizational colors through the context menu. Select a child to paint; creating a layer while a group is active inserts it inside that group. **Merge Selected** (**Ctrl+Shift+E**) combines selected siblings or groups into one raster layer. These shortcuts are configurable under **Settings → Shortcuts → Layers**.

Groups composite as a single image with their own opacity and blend mode. Image adjustments and filters preview on the group's combined content. Applying effects stores a live effect stack on the group and preserves every child layer for individual editing. Editing a child recomputes the group's effects. The context menu can remove the group's effects; Undo restores them. Only explicit rasterization or merging consolidates children. Groups, their caches, and child layers count toward the existing layer and memory limits.

### Image menu

![Image menu and adjustments](docs/image-menu.png)

The **Image** menu groups destructive RGB adjustments and document operations. Adjustable effects use movable, resizable floating windows without dimming the canvas, with live preview, reset, and apply/cancel controls. Loading indicators have reserved space so controls stay in place during processing. Processing runs in a cancellable background task; moving a slider briefly delays recomputation so intermediate requests do not build up. Applying creates one undo step, while canceling restores the original pixels and dirty state.

Color adjustments affect the **active visible layer**, restricted to the current selection when present. They preserve alpha, including soft selection coverage. Available adjustments:

- Brightness/Contrast, Levels with input/output ranges and gamma, and Curves with five control values and RGB or individual channel editing.
- Exposure, Vibrance, Hue/Saturation/Lightness, and Color Balance for shadows, midtones, or highlights.
- Black & White, Photo Filter, Channel Mixer, Invert, Posterize, and Threshold.
- Two-color Gradient Map, Shadows/Highlights, Desaturate, Replace Color, and Equalize.
- Auto Tone, Auto Contrast, and Auto Color, using histograms weighted by opacity and selection coverage.

Curves can be edited directly: drag control points in both axes, click the graph to add a point, and right-click an interior point to remove it. Expand **Adjust with sliders** for precise input/output controls. Both interfaces edit the same mapping.

![Curves adjustment with live canvas preview](docs/image-curves.png)

These are PaintAtlas implementations for 8-bit RGB images; they do not reproduce Photoshop's algorithms exactly. CMYK, HDR, adjustment layers, and color-profile conversion are not implemented.

Document operations affect **all layers** and preserve their properties:

- **Image Size:** resize with optional aspect locking and nearest-neighbor, bilinear, bicubic, or Lanczos resampling. Smooth filters use premultiplied alpha to avoid dark transparent edges. Disable resampling to change only print DPI without altering pixels.
- **Canvas Size:** expand or crop the canvas with a nine-position content anchor; new areas are transparent.
- **Image Rotation:** exact 90°/180° rotations, arbitrary rotation with expanded transparent bounds, and horizontal/vertical flips. These alter pixels, unlike view rotation and mirroring.
- **Crop to Selection:** crop every layer to the selection's rectangular bounds.
- **Trim:** remove transparent borders or borders matching either corner color, with tolerance and individual edge controls. Bounds come from the visible composite.
- **Duplicate Image:** create an independent document tab with all layers.

Dimension and pixel-memory limits still apply. Geometry operations clear the temporary selection after applying. Apply or cancel an image operation before drawing, switching documents, saving, or exporting.

### Filter gallery

![Filter gallery with active-layer preview](docs/filter-gallery.png)

Open **Filter → Filter Gallery** to browse **73 filters in 14 categories**, with search, demonstration thumbnails, a dedicated active-layer preview, and a live preview on the actual canvas. Individual filters are also available through category submenus. Controls include reset, effect strength, apply/cancel, deterministic random seeds, and boundary policies where relevant.

The catalog includes blur and focus effects, noise reduction, distortion, pixelation, procedural clouds/fibers, sharpening, stylization, artistic/sketch effects, textures, de-interlacing, custom convolution, high pass, morphology, and offset. Displacement supports importing an image map. Approximate effects are identified in the controls; these are independent PaintAtlas implementations, not copies of Adobe algorithms.

Filters affect the active visible layer and respect selections. Spatial effects can move or soften alpha, while procedural generators can fill an empty layer. Layer alpha lock is always respected, and applicable filters also offer a Preserve Transparency option. Apply creates one undo action; Cancel restores the original pixels and dirty state. Processing and thumbnail generation run in background tasks. Gaussian blur uses parallel, cache-friendly passes for large images and skips transparent margins around sparse drawings; its three-box Gaussian approximation remains the same.

Use **Ctrl+Alt+F** to repeat the last applied filter with its settings. Filter commands and the gallery can be remapped under **Settings → Shortcuts → Filters**. Last-filter settings persist for the current app session.

See the [filter inventory and compatibility assessment](docs/FILTER_CATALOG.md) for the available effects, the Adobe references used during research, algorithm differences, and deferred features such as Smart Filters, Neural Filters, Liquify, Camera Raw, and perspective tools.

### Interface and settings

- Panels can dock at canvas edges, share tabs, float, or open in native windows, with a persistent layout.
- Tools switch to icon-only buttons in narrow panels.
- Smooth zoom and rotation, panning, and view mirroring. Rotation pivots around the center of the canvas viewport to keep zoomed details in focus.
- Translucent glass with OpenGL blur: the canvas can appear behind panels when zooming or panning.
- Settings for the cursor, GPU, colors, appearance, and shortcuts.
- Accent colors, glass intensity, and reduced motion.

The selected GPU takes effect after restarting the app. The canvas and UI share the OpenGL context. If the saved GPU is unavailable, the app uses automatic selection.

The bundled font is Inter, under the license in `assets/fonts/OFL.txt`. For local use, the UI prefers `SF-Pro-Text-Regular.otf` in `$XDG_DATA_HOME/paintatlas/fonts/` or `~/.local/share/paintatlas/fonts/`. This local font is not included in the repository or executable.

## Default shortcuts

All shortcuts can be changed under **Settings → Shortcuts**, including alternative bindings and gesture modifiers. The UI identifies conflicts and saves changes automatically.

| Action | Shortcut |
|---|---|
| Brush / eraser | B / E |
| Fill / eyedropper | G / I |
| Line / rectangle / ellipse | L / R / O |
| Rectangular selection / lasso | S / A |
| Transform selection or active layer | Ctrl+T |
| Select all / deselect | Ctrl+A / Ctrl+D |
| Apply / cancel transform | Enter / Esc |
| Delete selection content | Delete |
| Move object during transform | Arrow keys; Shift+arrows for 10 px |
| Constrain a transform drag to axes or 45° diagonals | Hold Shift while dragging |
| Hand / temporarily pan | H / Space+drag |
| Temporary eyedropper | Alt+click; hold and drag to sample continuously |
| Decrease / increase brush size | [ / ] |
| Brush sizes | F6 |
| Zoom | Mouse wheel, Ctrl++ / Ctrl+- |
| 100% zoom / fit to window | Ctrl+0 / F |
| Rotate view −15° / +15° | Q / W |
| Mirror view | M |
| Copy / copy merged / paste | Ctrl+C / Ctrl+Shift+C / Ctrl+V |
| Undo / redo | Ctrl+Z / Ctrl+Shift+Z or Ctrl+Y |
| New / open / save | Ctrl+N / Ctrl+O / Ctrl+S |
| Close canvas / quick PNG export | Ctrl+W / Ctrl+E |
| Export as | Ctrl+Alt+Shift+E |
| Levels / Curves / Hue-Saturation | Ctrl+L / Ctrl+M / Ctrl+U |
| Color Balance / Invert / Desaturate | Ctrl+B / Ctrl+I / Ctrl+Shift+U |
| Image Size / Canvas Size | Ctrl+Alt+I / Ctrl+Alt+C |
| Repeat last filter | Ctrl+Alt+F |
| Auto Tone / Auto Contrast / Auto Color | Ctrl+Shift+L / Ctrl+Alt+Shift+L / Ctrl+Shift+B |

## Drawing tablets on Linux

When `DISPLAY` is available, the app uses X11 or XWayland for pen input. Pressure is primarily read through **XInput2** from the stylus exposed by the driver, including the `xwayland-tablet stylus` used by XP-Pen's PenTablet driver. Monitor mapping remains under desktop/driver control.

If pressure is unavailable from that source, the app tries evdev without exclusive capture. This requires read access to the device in `/dev/input`. The app does not change permissions and should not be run as root. Device discovery is repeated while the app is running.

To observe the input source and pressure for 20 seconds:

```bash
./target/debug/paintatlas --tablet-diagnostics
```

Automated validation does not replace testing contact, pressure, and the dial with a physical tablet.

## Projects, preferences, and limits

- **`.patlas` v7** projects preserve pixels, layer groups, live group effects and hierarchy, opacity, visibility, alpha lock, DPI, blend modes, and organizational label colors. Versions v1–v6 are migrated when opened; older app versions cannot open v7.
- Per-document limits: **4096 × 4096 pixels**, up to **32 layers**, and **256 MiB** of pixel data. The memory limit may prevent reaching both the maximum dimensions and layer count at the same time.
- History: up to **30 actions**, with a target budget of **128 MiB**. Older actions are removed when the budget is exceeded; at least one is retained. History belongs to each document's session and is not saved in the project.
- DPI is print metadata and does not multiply the pixel count. Exported PNG files include resolution metadata.
- Preferences and presets: `$XDG_CONFIG_HOME/paintatlas/brushes.json` or `~/.config/paintatlas/brushes.json`. The panel layout also persists.
- Automatic crash recovery, `.clip`/PSD compatibility, text/vector tools, and pen tilt support are not available yet. Save regularly.

Brushes rasterize on the CPU; previews and blend modes use OpenGL. During freehand drawing, only changed blocks are uploaded to the GPU and recorded in history. Shapes and some structural operations can still be expensive on large documents.

## Development

| File or directory | Responsibility |
|---|---|
| `src/main.rs`, `src/app.rs` | Startup, UI, splash screen, and commands |
| `src/document.rs`, `src/painting.rs`, `src/history.rs` | Pixels, drawing, and history |
| `src/brush_tip.rs`, `src/presets.rs`, `src/brush_sizes.rs` | Tips, presets, and sizes |
| `src/documents_ui.rs`, `src/workspace.rs` | Documents and docking |
| `src/selection*.rs` | Masks, transforms, and previews |
| `src/blend.rs`, `src/blend_gpu.rs` | CPU/GPU blend modes and radial fill composition |
| `src/fill_reveal.rs`, `src/picker_ui.rs` | Expanding fill reveal and eyedropper magnifier |
| `src/export_ui.rs` | Export options, background encoding, and file preview |
| `src/clipboard*.rs`, `src/image_drop*.rs` | Clipboard and drag-and-drop import |
| `src/image_menu.rs`, `src/image_ops.rs` | Image adjustments, geometry, previews, and background jobs |
| `src/filter_ui.rs`, `src/filters.rs` | Filter catalog, gallery, and raster processing |
| `src/tablet.rs`, `src/tablet/` | Pen input and pressure |
| `src/shortcuts.rs`, `src/shortcut_settings.rs` | Shortcut registry and configuration |
| `src/theme.rs`, `src/glass_gpu.rs`, `src/canvas_backdrop.rs` | Appearance, glass, and canvas backdrop |
| `assets/`, `default_brushes/` | Visual assets and brush tip pack |
| `vendor/` | Dependencies with local modifications |
| `gerar-appimage.sh`, `packaging/` | Local AppImage build and launcher assets |

**Every future feature with a shortcut must register its action in `src/shortcuts.rs` and use the configurable shortcut system.** This requirement is also highlighted in the source code.

The copies in `vendor/egui_dock`, `vendor/egui-winit`, and `vendor/winit` are part of the build. Do not remove them: they include docking, image clipboard, and Linux drag-and-drop fixes. Vendored dependency licenses remain in their respective directories.

Validation:

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Capture the splash screen in a separate run without using your preferences:

```bash
PAINTATLAS_SETTINGS=/tmp/paintatlas-preview-settings.json \
PAINTATLAS_WORKSPACE=/tmp/paintatlas-preview-workspace.bin \
PAINTATLAS_SMOKE_SCENE=startup \
PAINTATLAS_SCREENSHOT=/tmp/paintatlas-splash.png \
cargo run
```

The app exits this run after saving the screenshot. Other screenshots and an earlier analysis are in `docs/`; older materials may show previous versions of the interface.

## Git

The local repository uses the `main` branch. Source code, assets, documentation, `Cargo.lock`, and modified dependencies in `vendor/` should be versioned. The `target/` directory, caches, and local environment files are ignored; rebuilding recreates the executables.

```bash
git status
git diff
git diff --cached
```

To record changes, configure your identity if it is not already set and create a commit:

```bash
git config user.name "Your name"
git config user.email "your-email@example.com"
git add .
git commit -m "Update PaintAtlas"
```

The project is hosted at [Krueels/PaintAtlas](https://github.com/Krueels/PaintAtlas). In an existing checkout with `origin` configured:

```bash
git remote -v
git push -u origin main
```

For a fresh checkout:

```bash
git clone https://github.com/Krueels/PaintAtlas.git
cd PaintAtlas
./compilar.sh
```

GitHub pushes over HTTPS require a personal access token or an authenticated credential helper; an account password cannot be used for Git authentication. An SSH remote is also supported if an SSH key is configured for your GitHub account.
