# Filter inventory and implementation scope

Research snapshot: 2026-10-24. PaintAtlas currently exposes **73 filter commands across 14 categories**. This inventory covers Adobe's documented built-in filter families and specialized filter workspaces. Availability in Photoshop varies with version, platform, document mode, and installed models; third-party plug-ins are outside this inventory.

## Sources

- [Adobe filter effects reference](https://helpx.adobe.com/photoshop/desktop/effects-filters/blur-sharpen-filters/filter-effects-reference.html)
- [Adobe Filter Gallery overview](https://helpx.adobe.com/photoshop/desktop/effects-filters/get-started-with-filters/filter-gallery.html)
- [Adobe Blur Gallery](https://helpx.adobe.com/photoshop/desktop/effects-filters/blur-sharpen-filters/use-blur-gallery-depth-of-field-effects.html)
- [Adobe Neural Filters gallery](https://helpx.adobe.com/photoshop/desktop/effects-filters/neural-filters/neural-filters-gallery.html)
- [Adobe GPU feature requirements](https://helpx.adobe.com/photoshop/desktop/get-started/technical-requirements-installation/photoshop-and-graphics-processor-gpu-card-usage.html)

The source references establish the filter names and families. PaintAtlas's implementation choices and compatibility assessment below are project-specific engineering decisions. No Adobe code, presets, models, or proprietary filter algorithms are included.

## Available in PaintAtlas

All commands process the active visible raster layer, respect the current selection, and share live preview, cancellation, an effect-strength control, and one-step undo. A single filter is applied at a time; there is no saved multi-effect stack or Smart Object architecture.

| Category | Available commands |
|---|---|
| Blur | Average, Blur, Blur More, Box Blur, Gaussian Blur, Motion Blur, Radial Blur, Zoom Blur (radial zoom mode), Shape Blur, Surface Blur |
| Blur Gallery | Iris Blur, Tilt-Shift |
| Noise | Add Noise, Despeckle, Dust & Scratches, Median, Reduce Noise |
| Distort | Displace, Pinch, Polar Coordinates, Ripple, Shear, Spherize, Twirl, Wave, Zig Zag, Glass |
| Pixelate | Color Halftone, Crystallize, Facet, Fragment, Mezzotint, Mosaic, Pointillize |
| Render | Clouds, Difference Clouds, Fibers, Lens Flare |
| Sharpen | Sharpen, Sharpen More, Sharpen Edges, Adaptive Sharpen, Unsharp Mask |
| Stylize | Diffuse, Emboss, Find Edges, Oil Paint (Kuwahara approximation), Solarize, Trace Contour, Wind |
| Artistic | Colored Pencil, Dry Brush, Film Grain, Neon Glow, Poster Edges, Watercolor |
| Brush Strokes | Crosshatch, Ink Outlines |
| Sketch | Graphic Pen, Photocopy, Stamp, Halftone Pattern |
| Texture | Grain, Mosaic Tiles, Patchwork, Stained Glass, Texturizer |
| Video | De-Interlace |
| Other | Custom (3 × 3), High Pass, Maximum, Minimum, Offset |

## Implementation details and differences

- Gaussian blur uses three separable sliding-box passes as a fast Gaussian approximation. Box blur uses a single separable box kernel. Both process high-precision premultiplied pixels so transparency does not produce dark fringes.
- Motion, radial, zoom, and shape blur use bounded sampling. Shape blur offers disk, hexagon, and square kernels; radial and zoom effects expose a center and sample count. Iris and Tilt-Shift blend a preblurred image with a focus mask; they do not reproduce Adobe's lens, bokeh, or interactive pin systems.
- Surface Blur and Reduce Noise use a sparse range-weighted neighborhood. Despeckle and Dust & Scratches are median/edge/threshold-based approximations, not Adobe's proprietary denoisers.
- Median uses alpha-weighted RGB sliding histograms. Minimum and Maximum use separable monotonic queues. Their cost does not grow quadratically with radius.
- Displace can use the current layer's red/green channels or a user-imported image map. Coordinate filters expose clamp, wrap, and transparent boundary policies.
- Adaptive Sharpen provides thresholding and a local halo limiter; it is a practical alternative to Smart Sharpen, not its deconvolution algorithm.
- Oil Paint uses a sampled Kuwahara-style neighborhood. Watercolor, Dry Brush, colored pencil, neon, and other artistic effects use independent PaintAtlas constructions. Lens Flare, glass distortion, procedural clouds, and textures are also original approximations.
- Custom convolution currently supports a **3 × 3** kernel with divisor and offset, rather than Photoshop's larger custom matrix.
- Random effects use a deterministic seed, with a button to change it. Repeating a filter reuses its parameters and seed, and rebases dimensions and alpha-lock behavior to the current layer.
- Gallery cards show a built-in demonstration image. A dedicated preview pane shows the active layer with a transparency checkerboard, and the canvas also shows the document preview. Thumbnail generation and full-resolution processing run in background tasks.
- Spatial filters may move or spread alpha. Clouds and Fibers can fill an empty layer. The layer's alpha lock always wins; an optional Preserve Transparency control is also available. Partial selections and effect strength mix results in premultiplied RGBA.

## Adobe features not exposed yet

These are deliberately absent from the app menu, rather than nonfunctional placeholders. The available commands above and the deferred commands below form the research inventory for the classic filter families.

| Adobe family | Deferred commands or capabilities | Main work still required |
|---|---|---|
| Artistic | Cutout, Fresco, Paint Daubs, Palette Knife, Plastic Wrap, Rough Pastels, Smudge Stick, Sponge, Underpainting | More distinct region, stroke, and material models; avoid relabeling the same effect as multiple filters |
| Brush Strokes | Accented Edges, Angled Strokes, Dark Strokes, Spatter, Sprayed Strokes, Sumi-e | Directional stroke analysis and more convincing brush rendering |
| Distort | Diffuse Glow, Ocean Ripple | Dedicated glow and water-surface models |
| Blur | Lens Blur, Smart Blur | Depth/bokeh controls and a separate precision edge-preserving model |
| Blur Gallery | Field Blur, Path Blur, Spin Blur | Editable on-canvas pins, paths, multiple centers, and masks; basic radial spin is already available under Blur |
| Render | Flame, Picture Frame, Tree, Lighting Effects | Paths, procedural scene/asset systems, normals, and interactive lights |
| Sharpen | Full Smart Sharpen parity | Deconvolution and shadow/highlight controls beyond the available adaptive alternative |
| Stylize | Extrude, Tiles, Glowing Edges | Geometry and tiling models; Mosaic Tiles is available separately under Texture |
| Sketch | Bas Relief, Chalk & Charcoal, Charcoal, Chrome, Conté Crayon, Note Paper, Plaster, Reticulation, Torn Edges, Water Paper | Material synthesis and additional sketch algorithms |
| Texture | Craquelure; imported Texturizer surfaces | Crack networks and custom texture mapping |
| Video | NTSC Colors | A defined broadcast color-space pipeline; PaintAtlas currently edits RGB pixels |
| Other | HSB/HSL channel encoding; larger Custom kernels | Explicit channel-encoding controls and a larger matrix editor |
| Specialized workspaces | Adaptive Wide Angle, Camera Raw Filter, Lens Correction, Liquify, Vanishing Point | Camera/lens profiles, mesh brushes, perspective planes, and separate editors |
| Smart Filters | Convert for Smart Filters, editable filter stacks | Smart Objects or equivalent persistent effect nodes, masks, and serialization |
| Neural Filters | Skin Smoothing, Smart Portrait, Makeup Transfer, Landscape Mixer, Style Transfer, Harmonization, Color Transfer, Colorize, Super Zoom, Depth Blur, JPEG Artifacts Removal, Photo Restoration | Independently licensed models, inference runtime, downloads, and document integration; Adobe model/cloud services are not available to this app |
| Plug-ins | External filter plug-ins | A documented plug-in ABI and sandbox/lifecycle design; Photoshop binaries are not supported |

## Limits

Filtering currently targets 8-bit RGB(A), not CMYK, HDR, RAW sensor data, or 16/32-bit editing. Results are baked into pixels on Apply; only pixels and layer properties are stored in `.patlas`. Filter settings and the last-filter command are session state. Applying another filter starts from the latest committed pixels.

Large sampling filters can take time at full resolution, even though the interface stays responsive. Radius and sample controls are bounded, stale preview jobs are canceled after edits, and no worker is allowed to apply pixels after its editor has been canceled.
