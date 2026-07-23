# File operations details

The must-knows are in `CLAUDE.md`; per-dialog depth lives in each subdir's docs. This file holds only the umbrella-level
detail.

## Archive edits (`archive_edit`)

Copy/move/delete/mkdir/mkfile targeting a path INSIDE a zip run the backend's managed archive-edit op (an O(archive)
temp+rename rewrite), surfaced through the same transfer/queue UI as any write:

- **Routing.** Copy always goes through `copyBetweenVolumes` (backend resolves the archive dest), so it needs no
  special-casing. Move has a local same-FS fast-path (`moveFiles`) that would reject an archive-inner path, so
  `transfer-progress-state`'s `isVolumeMove` OR-s in `pathInsideArchive(destinationPath)` and `sourcePaths.some(...)` to
  force the cross-volume route for a move INTO or OUT of a zip — source and dest can share the parent drive's
  `volumeId`, so the id comparison alone misses it.
- **Op handle, not a path.** `create_directory`/`create_file` on an archive target return an operation id, and an in-zip
  rename starts an async op — the FE never treats these as a landed cursor target. The cursor lands via the durable
  `pendingCursorName` channel when the backing `.zip`'s live-watch refresh diff arrives (see the pane DETAILS). The
  create dialogs' return value is discarded either way (they forward the typed name), so no signature change was needed.
- **Permanent delete.** There's no Trash inside a zip (the backend rejects trashing an archive-inner path), so
  `openDeleteDialog` forces `isPermanent` + drops `supportsTrash` and passes `isArchive` for a source inside a zip;
  `DeleteDialog` then shows the archive warning banner and hides the "Move to trash" switch.
- **Presentation.** `archive_edit` is a `WriteOperationType` with the `file-archive` queue glyph (`operation-icon.ts`)
  and the "Editing archive" `queue.row.label` arm. It has no scan phase, so `TransferProgressDialog`'s `scanTitleMap`
  excludes it (the `scanTitle` derivation short-circuits for `archive_edit`).

## `scan-throughput.ts`

`ScanThroughput` turns scan-event tally deltas into a calm `filesPerSecond` / `bytesPerSecond` readout over a rolling
window (default 2 s, constructor-overridable). It exists because the backend `EtaEstimator` only covers write phases,
not the scan-preview pipeline, so the frontend computes its own scan-phase rate. The algorithm is deliberately tiny: a
number for the user to read, not a forecast. It returns nulls until two samples have arrived, drops samples older than
the window (always keeping the most recent so a long pause still has a baseline), clamps negative rates to zero, and
resets cleanly between scans.
