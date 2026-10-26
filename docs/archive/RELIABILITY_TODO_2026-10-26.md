# Historical snapshot: reliability TODO (2026-10-26)

This file preserves the detailed checklist, audit notes, and benchmark results
that were consolidated into the active [`TODO.md`](../../TODO.md). It is an
archive, not the current task queue; unresolved items below may be superseded.

## Goal and operating rules

Build a dependable art application for beginners and professionals, including
long sessions and large projects on modest and modern hardware. The document is
the source of truth; renderer/GPU resources are disposable acceleration.

### Minimum hardware target

- Use **4 GB RAM and a dual-core CPU** as the initial minimum reference for
  performance measurements and memory-budget decisions. The CPU baseline is
  Intel 4th generation or equivalent AMD hardware and newer; raise this floor
  gradually only after practical testing.
- Target NVIDIA GeForce GTX 1000-series and newer GeForce cards (including RTX),
  AMD and Intel discrete graphics,
  and integrated graphics from the CPU generations above through current
  hardware. Actual rendering support depends on the OS driver and required
  graphics APIs/features; verify and document capability rather than promising
  support based only on the GPU brand/model.
- Keep measurements on stronger hardware too. Do not impose requirements below
  this baseline if doing so would constrain maintainable development or make
  normal app use unacceptably slow.

- Make one focused change at a time; estimate peak memory and test behavior
  before increasing limits or broadening scope.
- Preserve edits on allocation, cancellation, save, and renderer failures; report
  degraded capabilities rather than silently losing precision or history.
- Keep compatibility and regression tests with every document/history change.
- Do not start a broad renderer rewrite until measurements identify a renderer
  bottleneck separate from CPU work, memory copies, and GPU uploads.

## Completed in this workstream

- [x] Add fallible document constructors and route canvas creation, image import,
  drag-and-drop, and paste through validated allocations.
- [x] Bound history entries and avoid creating a full pending structure snapshot
  when its estimated size already exceeds the history budget.
- [x] Notify users in English and Brazilian Portuguese when a change cannot be
  recorded because of the history memory limit.
- [x] Release the full-canvas composite after automatic-trim bounds are found.
- [x] Reduce sparse-blur peak memory; make cropped-blur buffers fallible and
  release the crop before allocating its full-canvas result.
- [x] Transform live effect masks using validated buffers without allocating a
  blank layer that would immediately be replaced.
- [x] Make non-resampling geometry allocation fallible and calculate arbitrary
  rotation trigonometry once per operation.
- [x] In non-nearest resizing, release consumed byte pixels before resampling
  intermediates and use fallible reservations for owned float/output buffers.
- [x] When a geometry job succeeds, move the old live document into history
  instead of cloning its full pixels at apply time; cover ownership and
  undo/redo roundtrip in a test.
- [x] Establish the initial audit of 8-bit assumptions for future
  multi-precision support; see [HIGH_BIT_DEPTH.md](../reference/HIGH_BIT_DEPTH.md).
- [x] Make layer insertion reserve both pixel storage and the layer list before
  changing active-layer metadata; use checked document-byte accounting.
- [x] Use fallible reservations for selection-transform source/crop buffers and
  pasted-selection buffers; report layer-merge allocation failures before
  changing document/history state.
- [x] Make selection-transform rasterization fallible and keep the floating
  transform available if allocation fails; the render fallback uses the last
  committed composite rather than panicking.
- [x] Prepare layer duplicates with fallible pixel/name/vector reservations
  before checkpointing or mutating the layer stack; preserve selection hierarchy
  and group references on insertion.
- [x] Add fallible selection mask construction, combination, and inversion with
  document-dimension checks; selection UI preserves the previous mask and shows
  an error on failure.
- [x] Add fallible rectangle, lasso, mask-combination, and inversion paths;
  selection UI reports allocation failures and retains the last valid mask.
- [x] Serialize `.patlas` directly into one fallibly reserved magic-prefixed
  buffer; verify an invalid save cannot replace a known-good project.
- [x] Add retained CPU-memory estimates for active/inactive documents, selection
  buffers, in-progress stroke tiles, and undo/redo snapshots; expose the estimate
  when hovering document tabs and clarify that GPU/worker buffers are excluded.
- [x] Track estimated bytes reserved by image workers and include them in the
  tab memory diagnostic; geometry, adjustments, and gallery jobs report estimates.
- [x] Reject `.patlas` files truncated at the header/payload boundary and files
  above the bounded-reader limit before attempting to read their contents.
- [x] Verify successful replacement of an existing project and cleanup/preservation
  when the final atomic rename/persist cannot replace its destination.
- [x] Extend the repeatable 3000×3000 stroke/history benchmark to report stage
  timings and estimated retained document/history CPU memory for snapshot and
  incremental-history paths.

## Active work: audit operation memory peaks

**Next action:** validate these estimates with repeatable RSS/GPU measurements on
the minimum reference hardware (4 GB RAM, dual-core CPU), then design an
enforceable global budget. The current tab estimate is diagnostic, not a hard
process-memory limit. Do not begin multi-precision or renderer work until P0/P1
foundations are ready.

### Audit findings (current implementation)

- Geometry runs on a full `Document::clone()`, so the live document and worker
  input coexist for the whole job (approximately 2× source pixel storage before
  outputs). On success, the worker input becomes the result; the former live
  document is moved into history, avoiding an additional apply-time clone.
- Geometry validates output bytes as `4 * new_width * new_height * layer_count`
  against 256 MiB, but that is an output bound, not a process peak bound: source
  clone, output layers, effect masks, existing history, and renderer buffers are
  outside that estimate. Non-resampling geometry temporarily overlaps source
  and destination for the current layer.
- The `image 0.25.10` non-nearest resampler first builds an RGBA32F horizontal/
  vertical intermediate of `source_width * target_height` pixels (16 bytes per
  pixel), then a target RGBA32F image (also 16 bytes per output pixel). Our code
  additionally holds the converted RGBA32F input while producing the intermediate;
  source bytes are dropped before the library call. Our final RGBA8 buffer is
  allocated while the target float image still exists. These are substantial
  per-layer peaks, and library output allocations are not fallible through this
  API. Nearest resize keeps the moved RGBA8 source alive while allocating output.
- Each live effect mask currently creates an RGBA8 temporary document and
  recursively runs geometry, then allocates the transformed byte mask. Multiple
  masks are processed serially, but each mask path has source + RGBA conversion +
  transformed RGBA + extracted mask overlap; output dimensions are already
  checked, though the mask working-set peak is not part of the byte estimate.
- Filter gallery thumbnails use a fixed 112×72 demonstration image; the gallery
  preview is capped at 384 px on its longest edge. These are not document-sized
  thumbnail buffers. A gallery worker and one adjustment worker can coexist.
- Adjustment source/original use `Arc`-backed sharing. A shared eight-worker
  admission limit bounds image jobs across windows/documents. Dropping a job sets
  cancellation but does not join it; its slot remains occupied until it exits.
- Saving previously held a serialized payload and a second magic-prefixed copy
  simultaneously. Serialization now sizes first, reserves one output buffer
  fallibly, and writes directly after the magic. Opening holds file bytes and the
  decoded document together, but input now uses a fallibly reserved bounded
  reader, including if the file grows after the initial metadata check.
- The current retained-memory estimate covers CPU-owned document/history and
  selection buffers. It does not deduplicate shared `Arc` allocations in all
  structures, include `Vec` capacity/allocator overhead, GPU texture residency,
  most transient CPU composites, renderer staging, all exports outside the
  current export dialog, or every in-flight worker capture/result.
  It is a partial estimate, not RSS or a reliable upper/lower bound for enforcement.
- In-flight image jobs now reserve estimated CPU bytes in a shared counter that
  remains charged until the worker exits, including canceled jobs. Estimates are
  conservative heuristics (geometry resize uses 14× document pixel bytes,
  non-resampling geometry 3×, adjustments 4× source bytes, gallery based on all
  thumbnail outputs); they are not allocator-enforced budgets and do not cover
  every library intermediate.
- Worker-estimate aggregation uses a checked 64-bit atomic counter so several
  large per-worker estimates cannot silently wrap the displayed total on
  32-bit platforms or at the aggregate boundary. This remains diagnostic only;
  successful estimation does not prove the allocator will stay within that size.
- `Document::MAX_BYTES` (256 MiB) accounts for layer pixels and retained group
  effect data, including isolated group raster caches. At 4096×4096, one RGBA8
  full-canvas buffer is 64 MiB. This document limit does not include the separate
  CPU/GPU copies listed below, history, or job inputs/results.
- The active document owns one full-resolution RGBA8 texture per layer. Its
  nominal texel storage is at most the document pixel-byte total (256 MiB), but
  backend allocation granularity, staging, and driver copies are not observable
  here. Converting each layer to `ColorImage` creates another 4 bytes/texel
  temporary while the source layer and existing GPU textures remain live; initial
  synchronization can therefore transiently add up to one layer (64 MiB) of CPU
  staging at maximum dimensions. Dirty group uploads allocate a packed RGBA
  rectangle; active-layer uploads allocate `Color32` entries for each dirty run.
- CPU blend fallback creates a full-canvas composite and then a `ColorImage`
  copy before updating its cached texture. At 4096×4096 this is up to 128 MiB of
  temporary CPU buffers in addition to document layers and renderer textures;
  the texture itself is another nominal 64 MiB. Selection transform separately
  retains a full-canvas `Floating::base` and a base texture, plus the cut pixels
  and cut texture; the CPU base is included in selection diagnostics, GPU storage
  is not.
- `Document::try_composite` now checks the dimension arithmetic and reserves its
  full-canvas RGBA buffer fallibly. PNG export and the export-dialog snapshot use
  this path and report allocation errors instead of panicking; the legacy
  infallible `composite()` wrapper remains for existing internal callers.
  Merged clipboard capture also uses the fallible composite and reserves the
  cropped output fallibly, returning an actionable error on failure.
  The eyedropper's retained merged snapshot also uses the fallible API and only
  activates after allocation succeeds; normal UI reports the failure. Its
  retained CPU snapshot and nominal magnifier texture size are now separately
  included in the per-tab diagnostic while active.
- Animated fill tries to fallibly reserve its full-layer before-image; if that
  reserve fails, the fill still proceeds without its optional animation. It then
  converts the copy to `ColorImage` and retains a before texture for up to 0.58 s.
  For a 4096² layer, the before-image, conversion staging, and texture each
  approach 64 MiB. This short operation does not acquire the shared image-job
  permit and overlaps existing document/history state; the `ColorImage` conversion
  allocation is still controlled by egui's infallible API.
- Bucket fill now allocates its visited map fallibly and discovers the complete
  connected region before writing any pixels. Queue growth is also fallible; if
  it fails, the operation returns an error and leaves the canvas unchanged. Its
  queue stores `u32` indices (up to 64 MiB at 4096² rather than 128 MiB for
  `usize` on 64-bit systems), while the visited map is another 16 MiB maximum;
  region discovery can therefore still use substantial memory before any edit.
- Full-canvas blur working buffers (two 16-bit RGBA planes), vertical running
  sums, and the bounded worker-handle list now use fallible reservations. Blur
  worker allocation errors propagate through the existing image-job error path;
  median/morphology/mosaic buffer copies and morphology queues also reserve
  fallibly. The pinned/library resampler, generic filter state/results, and some
  library or renderer allocations remain outside this control.
- Export snapshots retain one full-canvas composite in an `Arc`. Export work now
  uses the shared eight-job admission limit and charges a source-based peak-memory
  estimate while the worker runs, including the cross-aspect float resampler
  intermediate. Same-size export makes a pixel copy; resized export also uses
  the resampler float intermediates documented above. JPEG additionally
  materializes RGB bytes and decodes the encoded result to RGBA for its preview
  before thumbnailing to at most 560×560. Encoded output, pixel/resampler
  intermediates, and preview conversion can overlap. The estimate is diagnostic,
  not a hard byte budget. The tab diagnostic now includes the retained export
  source composite and completed encoded bytes, and separately estimates the
  preview texture as four bytes per texel; actual backend/driver residency may
  differ. Other app textures remain outside that estimate. Multiple app processes
  are not globally coordinated.
- Filter gallery thumbnails are bounded: 112×72 RGBA per filter, while the live
  preview is capped at 384 px on its longest edge. Clipboard-flight thumbnails
  are also downscaled. These are small relative to document-sized previews, but
  their egui texture memory is still absent from the tab diagnostic. Splash,
  brush-tip, picker, fill, selection, export, and layer textures have separate
  lifetimes; inactive document tabs release active renderer textures when stashed.

### Local benchmark baseline (above minimum target; 2026-10-25)

- Machine: AMD Ryzen 5 5600X (6 cores / 12 logical processors), 32 GiB RAM,
  NVIDIA GeForce RTX 3050, Windows. This is a modern reference, **not** the
  minimum dual-core/4-GiB profile.
- Existing release-mode manual benchmarks (`cargo test --release --locked -- --ignored --nocapture`):
  Gaussian blur on 3000×3000, radius 24, took
  120–138 ms across two runs. The 30-stroke 3000×3000 history scenario took
  736–768 ms with full snapshots and 18.1–18.3 ms with incremental history.
  Reported retained estimates were 68.66 MiB for the document and 1.88 MiB
  (snapshots) / 3.75 MiB (incremental) for history.
- Sampling the test process working set every 50 ms while both ignored benchmarks
  ran concurrently observed a 321.9 MiB peak. This is a process working-set
  sample, not a guaranteed RSS peak; short spikes may be missed. The benchmark
  test process does not create the app's renderer textures, so this is not a GPU
  memory measurement. GPU residency and minimum-target hardware remain unmeasured.
- The release-mode 3000×3000 full-region bucket fill took 201 ms on this machine
  (`cargo test --release --locked document::tests::benchmark_bucket_fill_3000 -- --ignored --nocapture`).
  A separate 10 ms process working-set sample observed 49.9 MiB; short spikes may
  be missed and this is not a GPU measurement. The test is retained as an ignored
  repeatable benchmark; the minimum-target hardware result remains pending.

- [x] Geometry: cover cancellation, invalid crop, and output-limit rejection
  without changing the caller's original document. Allocation failure in UI is
  likewise isolated to the worker's owned document clone.
- [x] Resize: inspect the pinned `image 0.25.10` implementation and estimate
  horizontal/final float buffers plus nearest-neighbor source/output overlap.
  Preserve filter quality; do not replace the resampler without tests/benchmarks.
- [x] Filters/previews: inspect gallery sizing, adjustment sharing, worker
  cancellation, and possible overlap of canceled jobs. Gallery buffers are
  bounded; the remaining global concurrency issue is tracked in P0 below.
- [x] Export: route encoding through shared worker admission and estimate
  same-size, resampling, JPEG, encoded-output, and preview buffers. The estimate
  remains diagnostic; retained source/output bytes and the nominal preview
  texture size are included in the tab diagnostic after the worker completes.
- [x] Record source-based estimates and findings here before related
  implementation; hardware measurements remain pending.
- [x] Make the optional animated-fill before-image copy fallible so allocation
  failure does not prevent the actual fill; add a focused copy regression test.
- [x] Add a fallible composite constructor and use it for export snapshots and
  PNG export; add a regression test for parity with the existing wrapper.
- [x] Route merged clipboard capture through fallible composite/output buffers.
- [x] Route the eyedropper snapshot through fallible composite allocation and
  keep picker state inactive if that snapshot cannot be created.
- [x] Include the eyedropper's retained composite and nominal magnifier texture
  in the tab memory diagnostic.
- [x] Make bucket-fill region discovery allocations fallible and defer all pixel
  writes until discovery succeeds, preserving the canvas on allocation errors.
- [x] Convert the large blur planes, vertical sums, and bounded blur worker list
  to fallible allocation with overflow regression coverage.
- [x] Convert remaining large median/morphology/mosaic copies and morphology
  queues to fallible allocation, preserving the worker's error propagation.
- [ ] For each safe candidate, add a focused regression test; after changes run
  the full suite and formatting/diff checks.

## Priority roadmap

### P0 — Bound memory and preserve edits

- [ ] Complete the active memory-peak audit above.
- [ ] Make remaining large intermediate allocations fallible where APIs can
  report failure without partially changing the live document.
- [ ] Continue the allocation audit beyond geometry/filters, especially
  document serialization and remaining unchecked buffers.
  Convert remaining unchecked large buffers to fallible allocation and expose
  actionable UI errors where the operation currently fails silently.
- [ ] Define/account for a global memory budget across open documents, undo/redo,
  worker inputs/results, CPU composites, previews, and GPU caches. Document the
  distinction between per-document limits and process peak memory.
- [ ] Ensure heavy operations stay cancellable and do not create unbounded
  concurrent jobs; verify every failure/cancel path leaves document and history
  in a valid state.
- [x] Add a shared admission limit of eight image-operation workers across
  documents/windows, including export workers. A canceled worker keeps its slot
  until it exits, preventing rapid close/reopen from bypassing the limit. This
  bounds job count, not bytes; per-job memory estimates and a global byte budget
  remain necessary.

### P1 — Recovery and long-session confidence

- [ ] Review save/load atomicity, interrupted writes, corrupt/truncated files,
  and preservation of all unsaved document tabs.
- [ ] Design and implement crash recovery/autosave with explicit retention,
  cleanup, disk-space handling, and user-controlled recovery; never overwrite a
  known-good project with an incomplete recovery write.
- [x] Define the recovery contract and implementation gates in
  [AUTOSAVE_RECOVERY.md](../reference/AUTOSAVE_RECOVERY.md); implementation,
  platform data paths, UI, and failure-injection tests remain open.
- [ ] Add repeatable stress/benchmark scenarios for large canvases, many layers,
  long stroke sessions, repeated undo/redo, filters, imports, and constrained
  memory. Record runtime and peak memory, not just pass/fail.
- [ ] Test recovery and performance on modest Windows/Linux hardware as well as
  modern systems; include tablet/pen drawing and long-session checks.
- [x] Record an initial repeatable release-mode CPU/history baseline and sampled
  process working set on the available modern Windows machine; 4-GiB/dual-core,
  GPU-residency, and broader stress scenarios remain pending.

### P2 — Professional workflow foundations

- [ ] Use stress and profiling results to plan tiled document storage and
  incremental history, preserving old project compatibility and exact undo.
- [ ] Review tablet pressure/tilt/dial consistency, brush latency, selection and
  transform workflows, and export correctness as separate measurable work.
- [ ] Improve documentation of current limits and unsupported features; do not
  claim reliability or format support beyond tested behavior.

### P3 — Multi-precision color (deferred until P0/P1 foundations)

- [ ] Implement document-level 8-bit integer, 16-bit integer, and 32-bit float
  RGBA with end-to-end parity across painting, filters, effects, selection,
  history, rendering, save/load, and export. Import/export conversion alone does
  not count as support.
- [ ] Follow the contract and acceptance criteria in
  [HIGH_BIT_DEPTH.md](../reference/HIGH_BIT_DEPTH.md): working color space,
  transfer function, float/HDR range, alpha, display conversion, project
  versioning, legacy promotion, and precision-aware memory budgets.
- [ ] Ensure every operation either preserves selected document precision or
  refuses before mutation; never silently quantize high-precision pixels.
- [ ] Test exact undo/redo, cancellation, legacy versions, PNG16 and a selected
  32-bit float interchange format before declaring support complete.

### P4 — Renderer strategy (deferred until measured)

- [ ] Benchmark CPU rasterization, composition, texture upload, GPU effects, and
  presentation separately on weak and modern hardware.
- [ ] Based on data, evaluate automatic `wgpu` backend selection, completing
  WGSL equivalents for app effects, and a software compatibility fallback.
- [ ] Keep document pixels independent from renderer resources and exercise GPU
  loss/recreation without data loss.

## Validation checklist

- [ ] After each code change: `cargo test --all-targets --locked`,
  `cargo fmt --all -- --check`, and `git diff --check`.
- [ ] Run available Windows/Linux checks when platform paths or shared document
  code change; keep both build paths in CI.
- [ ] For memory changes, state which buffers overlap, estimate the peak, and
  identify any remaining allocator/library buffers not controlled by the app.
