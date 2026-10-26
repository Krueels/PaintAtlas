# Autosave and crash recovery design

This document defines the recovery contract before autosave is implemented.
The implementation must treat the in-memory documents as authoritative while
the application is running; recovery files are disposable snapshots, never the
only copy of a project and never written over a user's `.patlas` file.

## What is recovered

- Snapshot every open tab, including inactive tabs, with a stable session ID,
  tab order, original project path (if any), and whether it was dirty.
- Persist each document using the existing `.patlas` serializer so current
  projects can be validated by the normal reader. Keep the session manifest
  separate from project documents.
- Restore document pixels, layers, groups, effects, and project metadata. The
  current project format does not persist undo/redo history, active selection,
  floating transforms, or view state. A recovered tab therefore starts with a
  clean history, no selection/transform, and is marked dirty until explicitly
  saved by the user.
- Never replace the original project path during recovery. Offer recovery as a
  separate unsaved document, and require the user to choose a save destination.

## Snapshot transaction

1. Write a complete new generation in a fresh directory beneath the
   application data directory. Use the existing atomic project writer for every
   document and verify each snapshot by opening it again.
2. Write the manifest last. It contains a version, generation ID, document
   entries, and enough metadata to restore tab order and source paths. Validate
   all paths as relative children of the recovery root; never trust paths read
   from the manifest to escape that root.
3. Flush files before publishing the generation. Publish with an atomic
   same-volume rename or an atomic pointer/manifest replacement, retaining the
   previous complete generation until publication succeeds.
4. On any serialization, disk-full, or rename error, keep the last complete
   generation and report the autosave failure. Do not delete or modify user
   projects, and do not mark the live document saved.

## Lifecycle and retention

- Autosave should be debounced after committed edits and also run periodically
  while dirty documents exist. Do not snapshot every pointer event; finish or
  cancel transient gestures before taking a snapshot.
- At startup, validate the newest published generation and ask whether to
  recover it. Do not restore automatically or silently discard it. If invalid,
  retain it for diagnostics and offer an older complete generation when one is
  available.
- Keep at least the last known-good generation until a newer one has been
  published and validated. Cleanup may remove older generations only after
  publication succeeds. Bound retention by both generation count and bytes;
  when disk space prevents a new snapshot, preserve the prior generation and
  show an actionable warning instead of silently pruning the only recovery.
- On a normal exit, remove recovery data only after the user has saved or
  explicitly discarded every dirty tab. If the app exits with dirty documents,
  preserve the latest complete generation for the next launch.
- Prevent two application instances from cleaning or publishing the same
  recovery session concurrently. Use an OS-level lock where available or
  per-process session directories and cleanup only sessions proven stale.

## Implementation gates

- Choose cross-platform application-data and locking APIs; do not derive the
  recovery path from the existing settings path, which currently uses a
  configuration directory and is not a portable data-directory abstraction.
- Add a versioned manifest parser with strict size/count/path limits before
  reading or allocating recovery data.
- Test multi-tab restore, invalid/missing snapshots, interrupted generation
  publication, simulated disk-full/write failures, retention, and user
  cancellation. Every failure must leave the original project and the previous
  complete recovery generation untouched.
- Add recovery UI only after persistence and validation tests pass. Ensure all
  strings are localized and the user can inspect the affected document names.

## Not promised

Autosave is not a replacement for explicit saves and cannot guarantee recovery
of edits made after the most recent completed snapshot. Until transactional
multi-document generations, validated startup recovery, and disk-failure tests
are implemented, PaintAtlas does not provide crash recovery.
