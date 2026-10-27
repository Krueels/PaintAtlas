# PaintAtlas — Next Steps

This file is the active work queue, not the project history. The initial
validation floor is **dual-core, 4 GB RAM, and 4th-generation Intel/equivalent
AMD hardware or newer**. For GPUs, consider GeForce GTX 1000-series and newer,
AMD/Intel discrete GPUs, and integrated GPUs compatible with available drivers
and APIs.

## Priorities

1. [ ] **Validate Zoom and brush HUD**: test mouse and pen input, configurable
   shortcuts, temporary zoom, and drawing sessions; record regressions.
2. [ ] **Complete the memory audit**: review remaining limits outside the now-
   fallible upload/composite paths, especially allocations in filter libraries
   and backend resources. Keep measurements and limits in
   [`MEMORY_AUDIT.md`](docs/reference/MEMORY_AUDIT.md).
3. [ ] **Measure on minimum-target hardware**: RSS, CPU peaks, and GPU residency
   on Windows/Linux, including large canvases, multiple layers, undo/redo,
   filters, and tablet/pen input. The current machine is not a substitute.
4. [ ] **Implement recovery/autosave** according to the contract in
   [`AUTOSAVE_RECOVERY.md`](docs/reference/AUTOSAVE_RECOVERY.md), including disk
   failures and recovery tests that never overwrite valid projects.
5. [ ] **Define a global memory budget** after item 3 measurements; include
   documents, history, workers, composites, and renderer resources.

## After the priorities

- Plan tiled storage and history evolution based on benchmarks.
- Multi-precision and renderer changes remain deferred. Follow the contracts in
  [`HIGH_BIT_DEPTH.md`](docs/reference/HIGH_BIT_DEPTH.md) and the
  [architecture guide](docs/reference/ARCHITECTURE.md); do not begin
  implementation before the foundations and measurements above are in place.
- Detailed history for this queue:
  [`RELIABILITY_TODO_2026-10-26.md`](docs/archive/RELIABILITY_TODO_2026-10-26.md).

## Validation for code changes

Run `cargo test --all-targets --locked`, `cargo fmt --all -- --check`, and
`git diff --check`. For memory changes, record overlapping buffers, peak
estimates, and allocation limits in libraries/renderers.
