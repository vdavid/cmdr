# Image-index status indicators

Replace the text strip (`FolderIndexStatus` in `SelectionInfo`) with three quiet, glanceable indicators at the natural
granularities: per-image icon overlay, per-folder icon overlay, per-drive dot. Words move into tooltips. The strip text
is removed.

Granularity truth: the "indexed" bit is stored **per image** in `media_status` (rows only for `done` / `failed`, no
pending rows, no `parent_dir` column, no prefix index). The eligible _set_ per folder lives in the drive index and is
already cached per-directory by `coverage.rs` (`FolderImageCounts.per_folder`, the denominator). We add the matching
per-directory **accounted** count (numerator) and subtree rollups.

## Backend (Rust, `src-tauri/src/media_index/`)

### 1. Per-directory accounted aggregate + subtree rollups (`coverage.rs`)

Extend the existing per-directory coverage structure so each directory carries both:

- `eligible`: images under this dir that qualify for indexing (already maintained via `walk_image_entries` +
  `qualify_dir`; reflects scope/exclude/format/size, so out-of-scope dirs are naturally `eligible == 0`).
- `accounted`: images under this dir whose `media_status` row is `done` **or** `failed` (both count: a failed image
  can't progress, so completion is `accounted == eligible`, else a folder with one corrupt file never reads complete).

Maintenance invariants (mirror how `eligible` is already seeded and patched):

- **Seed** `accounted` at init from one `SELECT path, state FROM media_status` scan, bucketed by parent dir (same pass
  that seeds `eligible`).
- **Increment** on enrich completion: in the `media_status` writer, when a `done`/`failed` row is written for a path
  that had **no prior row**, `accounted[parent_dir] += 1`. Do one PK existence check before the upsert to distinguish
  insert from update (completions are already expensive, so the extra point lookup is free). A `done↔failed` transition
  or re-enrich of an existing path does not change `accounted`.
- **Decrement** on row deletion (GC, prune, reclaim, vanished-file cleanup): `accounted[parent_dir] -= 1` per deleted
  path. These paths are already enumerated at deletion time.
- **Subtree rollup**: expose `eligible_subtree(dir)` and `accounted_subtree(dir)` = sum over `dir` and all descendant
  dirs. Compute via a rollup cached alongside the per-dir map and invalidated when the map changes (recompute is
  `O(dirs)`, fine at the throttled tick rate). Do not scan `media_status` per query.

Staleness caveat (documented, accepted first cut): a `done` row whose file changed since indexing still counts as
`accounted` until re-enriched, so a folder/drive can briefly read "complete" while a changed file awaits re-work.
Excluding stale rows would need a per-row `(mtime, size)` compare against the live index; out of scope here.

### 2. Tauri commands

- `media_index_file_status(volume_id, paths: string[]) -> FileIndexStatus[]`, one per input path, in request order.
  **Backend classifies** each path (smart-backend/thin-frontend: no client-side mtime/size compare). `FileIndexStatus` =
  `{ path, state }` where `state`:
  - `indexed`: `done` row, `(mtime, size)` + engine stamp current.
  - `stale`: row exists but the live file changed since indexing (needs re-enrich). Uses the existing `needs_enrichment`
    predicate against the live drive index.
  - `failed`: `failed` row.
  - `pending`: eligible (passes the gate) but no current `done` row yet.
  - `excluded`: not eligible (out of scope, excluded folder, unsupported type, too big).
  - `notApplicable`: not an indexable media type → frontend renders no badge. Bounded to the visible rows the frontend
    passes; a per-path drive-index lookup for the bounded set is cheap.
- `media_index_folder_coverage(volume_id, folder_paths: string[]) -> FolderCoverage[]`, one per folder:
  `{ path, eligible, accounted }` (subtree). Frontend derives the two-state badge and the `accounted/eligible` tooltip
  fraction. `eligible == 0` → frontend shows no folder badge (nothing here is on the to-be-indexed list).

Register both in `bindings.ts` (specta) and add frontend wrappers in `src/lib/tauri-commands/media-index.ts`.

### 3. Drive dot state

Reuse existing data; add a small helper (frontend-side, like `driveIndexState()`), no new command needed. Derive from
`media_index_volume_state` (qualifying vs covered/enriched counts) + `getVolumeEnrichActivity(volumeId)`:

- `off` (gray): master toggle off, or this volume not image-indexed.
- `indexing` (yellow): enriching now, or `accounted < eligible` at the volume level.
- `done` (green): idle and all eligible accounted.

## Frontend

### #1 File-icon overlay (`FileIcon.svelte`, top-right)

- New `{#if imageIndexBadge}` block inside `.icon-wrapper`, positioned `position: absolute; top: -2px; right: -2px`
  (top-right is free; bottom-right and top-left are taken by sync/symlink badges). Subtle: gray, no background fill,
  small (~10px), never shouty.
- State → icon (lucide, gray, David reviews visuals): `indexed` → `circle-check`; `pending` → hollow/`circle-dashed`;
  `stale` → `rotate-cw`; `failed` → `circle-x`; `excluded` → most subtle (`circle-slash`/`circle-minus`);
  `notApplicable` → no badge.
- Tooltip carries the textual status (i18n).
- **Data flow**: mirror `syncStatusMap` — a path-keyed map owned by `FilePane.svelte`, populated for visible image paths
  via `media_index_file_status`, threaded through `FullList` / `BriefList` to `FileIcon` as a resolved prop. Refresh on
  directory-listing change and, debounced, on `media-enrich-progress` for the current volume; final refresh on
  `media-enrich-terminal`. (Event-driven, not the 3s poll.)
- **Setting** `mediaIndex.showFileStatusIcons` (boolean, default **on**), Settings › AI › Image search, gated under
  `{#if imageIndexEnabled}`. When off, the file overlay is not fetched or rendered. Folder overlay and drive dot are
  always on (inherently sparse). Wiring: `ai.ts` definition + `types.ts` key + `settings.json` intl +
  `SettingRow`/`SettingSwitch` in `ImageSearchSection.svelte`.

### #2 Folder-icon overlay (`FileIcon.svelte`, top-right, folders)

- Two states only: all-indexed (`accounted == eligible`, `eligible > 0`) vs some-pending (`accounted < eligible`).
  `eligible == 0` → no badge.
- Icons (gray): all-indexed → `circle-check`; some-pending → a distinct hollow/partial glyph. Tooltip shows the subtree
  fraction `accounted/eligible` (e.g. "12/50", "0/50", "50/50") plus a short phrase.
- **Data flow**: same path-keyed-map pattern via `media_index_folder_coverage` for the visible folders. Same refresh
  triggers as #1.

### #3 Drive dot (`DriveIndexBadge` sibling in `VolumeBreadcrumb.svelte`)

- A second 10px dot (`.image-index-badge`) immediately after the filesystem `DriveIndexBadge`, both the active-drive
  breadcrumb and the volume-dropdown rows. Colors: gray `off`, yellow (pulse) `indexing`, green `done` — reuse the
  existing dot's color tokens (`--color-text-tertiary` / `--color-apple-blue`-or-`--color-warning` / `--color-allow`).
- Tooltip: live "N of M images indexed on this drive" + state phrase, from volume state + enrich activity.

### Strip removal (`SelectionInfo.svelte`)

- Remove the `FolderIndexStatus` import (line ~39) and its markup block (~366-370). Drop the now-dead `currentPath` prop
  and its upstream feeders in `FilePane.svelte` (`imageIndexFolderPath` derived + the prop pass). Keep `volumeId` (still
  used by the scan hourglass).
- Delete `FolderIndexStatus.svelte`, `folder-index-state.ts`, and their tests (only consumer removed). Their
  coverage-vs-completion logic is superseded by the honest per-folder command.

## Tests

- **Backend (TDD, red first)**: aggregate seed; increment only on genuinely-new `done`/`failed`; no double-count on
  re-enrich; `done↔failed` keeps `accounted` stable; decrement on delete; subtree rollup correctness (nested dirs);
  `failed` counts toward `accounted`; both commands' classification and ordering.
- **Frontend**: state→icon/tooltip mapping for file and folder badges; drive-dot state derivation; setting gates the
  file overlay; SelectionInfo no longer renders the strip text.

## Docs

- `media_index/CLAUDE.md` + `DETAILS.md`: the per-folder accounted aggregate now exists (update the "no cheap per-folder
  count" open item to describe what's maintained and the staleness caveat); the two new commands.
- Colocated `CLAUDE.md` for the file-explorer selection/views area if a must-know changes (new per-path map prop).
- Remove/replace stale references to `FolderIndexStatus`.
