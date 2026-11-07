# Drop in (from other apps)

Drops from other apps that carry no file URLs: a browser image, a Mail attachment, a Photos item. wry's drop event reads
file URLs only, so for these its `paths` come back empty. This module says what a drag carries on enter, receives the
promised files into a per-drop folder on drop, and removes that folder when the copy out of it ends. The frontend half
is `apps/desktop/src/lib/file-explorer/pane/promise-drop.ts` (pairing, labels) and `promise-drop-controller.ts` beside it.

## Module map

- `events.rs`: the typed events (`drag-payload-kind`, `promise-drop-started` / `-ready` / `-failed`). Always compiled.
- `folders.rs`: the drop-folder registry (create, claim by a copy, release, startup sweep). Cross-platform.
- `macos.rs`: the AppKit glue, called from the `drag_image_detection.rs` swizzles. `payload.rs` (pure `classify`, image
  naming) and `tally.rs` (pure per-drop receive accounting) under it.

## Must-knows

- **Every AppKit call that can raise goes through `objc2::exception::catch`.** An Objective-C exception that reaches
  Rust aborts the app (CRASH-K35S2). `receivePromisedFilesAtDestination:` raises outside `performDragOperation:`.
- **An in-app drag is never received** (`-draggingSource` non-nil → `InApp`): a virtual pane's drag carries ONLY
  promises, and receiving it would round-trip the files through a temp folder instead of the recorded self-drag identity.
- **`promise-drop-started` goes out BEFORE wry's drop event** (the swizzle emits, then forwards). The frontend still
  pairs them in any order (`promise-drop.ts`).
- **The folder belongs to the copy that reads it**: `copy_between_volumes` claims it, `terminal_effects` releases it on
  complete or cancel. A FAILED copy keeps it, so Retry finds the files; the startup sweep takes it later. ❌ Don't
  release from the frontend after confirm.
- **`release_drop` refuses anything but a `drop-<n>-<n>` id**, so a frontend id can't name a path outside the root.

Flows and decisions: `DETAILS.md`. What Chrome, Safari, and Mail put on the pasteboard (verified vs. assumed) and the
manual QA plan: `apps/desktop/src/lib/file-explorer/drag/DETAILS.md` § "Drops from other apps". Read them before any
non-trivial work here: editing, planning, reorganizing, or advising.
