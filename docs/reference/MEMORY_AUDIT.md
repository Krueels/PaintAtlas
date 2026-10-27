# Memory audit and benchmark notes

This is supporting evidence for the active queue in [`TODO.md`](../../TODO.md),
not a process-memory budget. Estimated CPU bytes omit some allocator capacity,
library intermediates, renderer staging, GPU residency, and unrelated app
textures. Working-set samples can miss short peaks.

## Known peaks and remaining limits

- Geometry workers clone the document, so source pixels coexist with worker
  input and outputs. Output validation enforces the 256 MiB per-document limit,
  not a process peak limit. Non-nearest resize uses RGBA32F horizontal and output
  intermediates (16 bytes/texel each); some `image` crate allocations are not
  fallible through its API. Live effect masks are transformed serially but add
  source, conversion, transformed, and extracted buffers.
- Image workers share an eight-job admission limit. Estimated job bytes remain
  diagnostic heuristics, not allocator-enforced limits; a canceled worker keeps
  its slot until it exits. Multiple app processes are not coordinated.
- `Document::MAX_BYTES` is 256 MiB for document-owned pixels and group caches.
  At 4096², one RGBA8 canvas is 64 MiB. This excludes history, workers,
  composites, CPU upload staging, and GPU copies.
- Initial layer texture synchronization and partial layer/group updates now use
  fallible CPU `ColorImage` reservations; pending damage is retained for retry
  when staging fails. Effect-bearing group composites use a fallible candidate
  buffer and only replace the cached pixels after the complete effect stack
  succeeds. For a 4096² canvas, the candidate, an adjustment output, and an
  optional copied mask can overlap (at least 128 MiB, plus up to 16 MiB for the
  mask), before filter-specific working buffers. Grouping layers now moves
  selected layer buffers rather than cloning their full pixel arrays. CPU blend fallback can still overlap a full-canvas
  composite and a `ColorImage` copy (up to 128 MiB), besides renderer textures.
  Egui/backend texture allocations and driver residency are not fallible or
  currently observable through this app path.
- Floating-selection previews and clipboard-paste previews now reserve their
  CPU upload images fallibly before installing preview state. Creating the GPU
  textures remains controlled by egui/backend APIs.
- Brush-tip thumbnail conversion and import staging now reserve their explicit
  CPU buffers fallibly; decoding/resizing inside the `image` crate remains
  outside this allocation control.
- Export uses worker admission and a source-based estimate, including float
  resize intermediates and JPEG conversion. Estimates are not hard limits;
  completed output and preview are retained by the dialog and counted separately
  in the tab diagnostic.
- Animated fill reserves its before-image and staging `ColorImage` fallibly and
  skips animation on failure. Bucket fill discovers the whole region before
  changing pixels and uses fallible allocations; at 4096², its `u8` map is 16
  MiB and its worst-case `u32` queue may reach 64 MiB.
- Blur planes, row sums, worker handles, and median/morphology/mosaic copies use
  fallible reservations. Remaining review areas are allocations internal to
  third-party filter/image libraries, legacy callers of group rebuilding, and
  renderer/backend resource creation.
- Fallible composite allocation is used for PNG/export snapshots, merged
  clipboard capture, eyedropper snapshots, and CPU blend fallback. Other
  internal callers may still use the compatibility `composite()` wrapper.

## Existing local baseline

Machine: AMD Ryzen 5 5600X (6 cores / 12 threads), 32 GiB RAM, GeForce RTX 3050,
Windows. This is **above** the minimum target.

| Scenario (release) | Result |
|---|---:|
| Gaussian blur, 3000², radius 24 | 120–138 ms |
| 30 strokes, 3000², full snapshots | 736–768 ms |
| 30 strokes, 3000², incremental history | 18.1–18.3 ms |
| Full-region bucket fill, 3000² | 201 ms |
| Bucket-fill test process working set, 10 ms sampling | 49.9 MiB |
| Blur + history test processes, 50 ms sampling | 321.9 MiB |

The bucket-fill and blur/history working-set figures came from separate test
process measurements. They are sampled process working sets, not guaranteed peak
RSS; the test process does not create the application's renderer textures.
GPU residency and minimum-target hardware remain unmeasured. Repeatable manual
benchmarks:

```text
cargo test --release --locked document::tests::benchmark_bucket_fill_3000 -- --ignored --nocapture
cargo test --release --locked -- --ignored --nocapture
```

## Memory-safety work already in place

Document construction, geometry, selection masks/transforms, save serialization,
bounded project reads, worker admission/estimates, export buffers, clipboard
capture, eyedropper snapshots, bucket-fill region discovery, and major blur/filter
buffers have fallible or bounded paths. History is bounded; atomic saves preserve
the previous destination on failure. Refer to source tests for exact guarantees.
