# Multi-precision color documents

## Scope

The goal is document-level RGBA precision selection with **8-bit integer,
16-bit integer, and 32-bit floating-point channels**. Precision must be retained
through the full editing workflow, not only during file decoding or encoding.
All layers in a document use the selected precision unless a later design proves
mixed-precision layers safe and useful.

32-bit float support must preserve values beyond the display range when the
chosen working space allows it. That requires an explicit working color space,
transfer function, alpha policy, and display transform; merely storing floats
in [0, 1] would not provide an HDR workflow. CMYK and ICC profile conversion are
not implied and need their own tested design.

## Current implementation audit

- `document::Layer::pixels` and stroke tiles are `Vec<u8>` RGBA.
- `Document::composite`, source-over blend, painting, fill, and alpha lock use
  8-bit channel arithmetic and `[u8; 4]` colors.
- Selection/floating-transform data and group-effect masks use byte buffers.
- Image adjustments, filters, resize, and previews consume/produce byte RGBA.
- Canvas texture uploads and CPU fallback composition use 8-bit pixels.
- `.patlas` currently writes magic/version 7 and stores layer bytes in the
  serialized document; versions 1–7 must remain readable.
- Image import currently decodes through `into_rgba8`; PNG export is built from
  8-bit pixels. JPEG and WebP export paths are also currently byte-based.

These assumptions mean a storage-only change would either fail to compile or
silently quantize data in one of the edit paths. No high-precision claim should
be made until the completion criteria below pass.

## Design constraints

1. Define an explicit document precision and pixel-buffer representation for
   RGBA8, RGBA16 integer, and RGBA32 float. Keep component count separate from
   allocated bytes: a pixel uses 4, 8, or 16 bytes respectively.
2. Define the RGB working space and transfer behavior for integer modes, the
   float range/HDR policy, alpha as linear coverage, and display conversion
   before implementing float painting or compositing. Do not conflate channel
   precision with gamut or color-profile support.
3. Keep 8-bit documents memory-efficient. A derived 8-bit display/GPU preview
   may be used by the current renderer, but it is a disposable cache, never the
   saved or edited source of truth.
4. Every advertised operation must preserve the document's precision. If a
   path is not implemented yet, report that before starting/mutating; never
   silently quantize a high-precision document to make the operation succeed.
5. Recalculate document, history, temporary-operation, import, and preview
   limits in actual allocated bytes. Include conversions and simultaneous
   buffers in peak-memory estimates before changing existing safeguards.
6. Add a new `.patlas` version with an explicit precision tag and payload.
   Continue loading v1–v7 as RGBA8 with exact integer promotion (`u8 * 257` for
   RGBA16); define an explicit conversion for opening legacy files as float.
   Older app versions cannot be expected to open the new format.
7. Start with lossless import/export where codecs support the chosen precision
   (PNG16; a suitable 32-bit float format to be selected and tested). Export to
   lower precision must be an explicit conversion and must not mutate the
   document.

## Implementation sequence

1. **Color/data contract:** choose working color space, transfer, float/HDR
   range, pixel-buffer API, document precision rules, and byte-budget policy.
2. **Serialization:** versioned `.patlas` precision tags, legacy promotion,
   corruption checks, and exact conversion tests.
3. **Core document operations:** painting, erasing, fill, blend/composite, alpha
   lock, group effects, and an 8-bit display preview derived from source pixels.
4. **History and selection:** precision-aware tile deltas, masks, floating
   selections, transforms, undo/redo, and cancellation with exact preservation.
5. **Image operations:** adjustments, all filters, resize/geometry, and previews;
   test each operation at each supported precision, including float HDR values.
6. **I/O and UI:** precision-preserving import/export, explicit down-conversion,
   document precision indicator/selection, and clear unsupported-path errors.
7. **Stress/compatibility:** memory peaks, large documents, legacy projects,
   save interruption, repeated edit/undo/redo, and modest-hardware testing.

## Completion criteria

- 8-bit, 16-bit integer, and 32-bit float test documents can be created or
  imported, edited through every advertised path, saved, reopened, and exported
  without unintended precision loss. Float tests include values outside [0, 1]
  if the selected working-space/HDR contract supports them.
- Undo/redo, cancel, and failed operations preserve exact source values.
- Legacy v1–v7 projects load and round-trip; promotion to higher precision is
  exact where mathematically possible and documented otherwise.
- Every feature either works at the selected precision or clearly refuses
  before mutating data; there are no silent 8-bit fallback edits.
- Peak memory is measured and bounded for documents, history, previews, GPU
  conversion, and temporary operations; no limits are raised by assumption.
- UI and documentation state precision, working space, export conversion, and
  HDR/color-management capabilities accurately.
