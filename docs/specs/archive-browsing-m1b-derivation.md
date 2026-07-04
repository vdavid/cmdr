# Archive browsing — routing-milestone derivation (M1b ground truth)

Companion to [archive-browsing-plan.md](archive-browsing-plan.md) § M1b. Two exhaustive derivation passes (backend
routing sites, frontend pane-volumeId readers) run against the post-drain code, plus the lead decisions they produced.
Implementation agents: treat this as the authoritative site list; the plan holds intent and sequencing.

## Lead decisions (refining plan decision 1)

1. **The tab keeps ONE id: the parent drive (display).** No `archive-<hash>` ever enters frontend state, history,
   persistence, or MCP sync. Rationale (from the FE derivation): all display chrome resolves from the volumes store off
   `tab.volumeId` (archive ids aren't in the store); the unmount-redirect compare (`edge-flow-handlers.ts`
   `getPaneVolumeId(pane) === unmountedId`) must match the parent drive or dead-mount recovery breaks; persistence +
   restore (`initialization.ts::resolveVolumeId` re-derives from path) become archive-safe with zero new code.
2. **All I/O routing happens backend-side in `VolumeManager::resolve(volume_id, path)`.** The FE keeps sending
   `(parentDriveId, /path/to/foo.zip/inner)`; resolve detects the boundary, `register_if_absent`s the `ArchiveVolume`,
   and returns `(archive_volume, inner_path)`. The FE derives archive-NESS (not an id) from the path for capability
   gating: `pathInsideArchive(path)` → capabilities kind `'archive'`.
3. **Boundary detection is ONE shared helper** (`path → Option<(zip_path, inner_path)>`; extension check at listing
   time, magic-byte confirmation at resolve/navigation time). Used by `VolumeManager::resolve` AND
   `commands/volumes.rs::resolve_path_to_volume` (and mirrored by the FE's `pathInsideArchive` extension check). Two
   drifting detectors = pane label and I/O target disagree.
4. **Lifecycle = backend LRU only, no FE refcount.** Since the FE never holds archive ids, the search-results
   refcount/release machinery is NOT copied. `VolumeManager` gets an archive LRU (cap ~16): register on first resolve,
   `unregister` + index-cache drop on eviction. Eviction is harmless — the next listing re-resolves and re-registers
   lazily (`ArchiveVolume::new` is cheap; the index re-parses on demand). This replaces the plan's FE-refcount sketch.
5. **Viewer preview-in-zip = bounded temp-extract.** The viewer core is 100% `std::fs::File` (no Volume seam — the
   biggest hidden cost the plan missed). `viewer_open*` detects an archive path, streams the entry via
   `open_read_stream` to a `.cmdr-` temp (size-capped, typed error beyond the cap; startup reaper covers leftovers,
   delete on session close), and opens that. Threading a Volume byte-source through the viewer is a later refactor.
6. **`resolve_location` / `resolve_path_to_volume` return the PARENT drive id** for archive-inner paths (display
   semantics, consistent with decision 1). They gain the boundary check only to validate/normalize and to keep the
   'category' correct — they do NOT mint archive ids for the FE.

## Backend: resolve-adoption sites (all paths under `apps/desktop/src-tauri/src/`)

Adopt `VolumeManager::resolve(volume_id, path)` (returns the archive volume + inner path when the path crosses a
`.zip`):

- `file_system/listing/operations.rs:58` `list_directory_start_with_volume` — the primary funnel. **Cache the listing
  under the RESOLVED identity (archive id + inner path)**, or the downstream re-`get()` sites (`refresh_listing`,
  `caching.rs:671` `notify_full_refresh`, `caching.rs:794` `get_watched_listing_entries`, `watcher.rs:369`) resolve a
  different volume than the read. Index enrich/verify no-ops for archives.
- `file_system/listing/streaming.rs:535,545` — the second listing entry point; consume the SAME resolved pair. Archive
  `supports_watching()=false` skips the watcher hookup; `root()` = the `.zip` path feeds the FE `volume_root`.
- `commands/file_system/listing.rs:113` `path_exists` — mechanical.
- `commands/file_system/listing.rs:409` `refresh_listing` — mechanical (archive re-reads, correct).
- `commands/file_system/volume_copy.rs` `copy_between_volumes:39,45`, `move_between_volumes:84,90`,
  `scan_volume_for_copy:126,129`, `scan_volume_for_conflicts:166,182` — resolve as safety net; **the real read-only
  guard: dest-resolves-to-archive ⇒ typed rejection** (extract-out with source=archive is the supported path). One
  `source_volume_id` per batch, no straddle risk.
- `file_system/write_operations/rename.rs:138,388` — post-success notify + sibling-conflict metadata read; mechanical.
- `commands/file_system/drag.rs:26` — locality only; archive → Virtual. `native_drag/fulfillment.rs:111` works once
  registered.

**Not adoption sites** (false positives): `indexing/mtp_index.rs:40`, `indexing/smb_watch.rs:309` (archives get no index
DB), `space_poller.rs:232` (no path arg; archive delegates space to parent; just needs registration), `create.rs:192`
(`supports_local_fs_access` false already correct).

## Backend: no-`volume_id` commands (bypass VolumeManager)

- `commands/file_viewer.rs:32,49,196` (`viewer_open`, `viewer_open_as_text`, `viewer_write_range_to_file`) — the
  temp-extract design (lead decision 5). M1-critical.
- `commands/file_system/stat.rs:53` `stat_paths_kinds` — route archive-inner paths through resolve→`get_metadata`.
- `commands/file_system/write_ops.rs:80,101` local `copy_files`/`move_files` fast-paths — **reject** archive-inner
  source or dest (typed); real archive copies go via `copy_between_volumes`.
- `commands/file_system/write_ops.rs:121,142` `delete_files`/`trash_files` — **reject** archive-inner paths.
- `commands/rename.rs:93` root branch — covered by the `rename_managed` fork below.
- `go_to_path/mod.rs:132` `resolve` — today an inner path silently lands on the `.zip` (`NearestAncestor`). Add the
  archive branch: boundary-detect, magic-byte confirm, return `Directory{path}` so path-bar typing enters the zip.

## Backend: managed instant-op read-only forks

Fork on archive-inner target, returning typed not-supported (these same seams become the mutation routing later):

- `file_system/write_operations/rename.rs:65` `rename_managed` — fork at top (covers non-root and root branches).
- `file_system/write_operations/create.rs:119` `create_directory_core`, `:163` `create_file_core` — fork before `get()`.

## Backend: registration + lifecycle

- `register_if_absent(archive_id, ArchiveVolume::new(parent, zip_path))` on first resolve, after magic-byte
  confirmation. Precedents: `volumes/watcher.rs:202` (FSEvents), the SMB pre-registration.
- Archive LRU (lead decision 4) in/next to `VolumeManager`: cap ~16, `unregister` + `ArchiveIndexCache` drop on
  eviction. Modeled loosely on `indexing::retention::enforce_external_index_cap`.

## Frontend: what changes (kind-from-path model)

- `volume-capabilities.ts`: add `'archive'` to the capabilities `VolumeKind` union (NOT the tint union in
  `volume-tint.svelte.ts` — archive panes show parent-drive tint). Kind derivation needs the PATH: a pane whose path is
  inside an archive gets kind `'archive'` regardless of `volumeId`. New frozen `CAPABILITY_TABLE` row (read-only phase):
  `hasBackendListing:true, canBeSource:true, canPasteInto/canCreateChild/canRenameInPlace:false, supportsSystemClipboard:false, pathScheme:'filesystem', hasParentRow:true`;
  mutation later flips the three write flags. Decide `syncsToMcp` explicitly (archive HAS a backend listing; recommend
  true, reporting parent id + full path).
- **Caps-bypass fixes (the derivation's key catch)**: `file-operation-commands.ts` (`startRename:41`,
  `openNewFolder:70`, `openNewFile:102`) and `transfer-entry.ts::checkTransferDestinationGuard:71` gate writes on
  `VolumeInfo.isReadOnly`, which an archive pane doesn't have (falls through as writable!). These sites must consult
  capabilities (the `'archive'` kind) for the read-only decision, or the kind row is dead code.
- `createGitBrowserSync` (FilePane) is NOT gated off inside archives (`hasBackendListing` is true) — add an explicit
  archive opt-out.
- Space watch (`createVolumeSpace`, gate currently `getIsDiskImage`) — skip inside archives (no VolumeInfo; parent space
  is what the status bar should show; simplest: keep watching the DISPLAY volume's space, which is the parent — verify
  which id it keys on).
- `pane/navigate.ts`: in-place-vs-switch compare and `commitPathFromListing` keep working on parent ids (entering
  `/foo.zip` is same-volume in-place nav — correct). The foreign-drop guard (`isPathOnVolume` with parent volumePath)
  passes for archive-inner paths — keep display semantics, don't switch it to the archive root.
- `navigateToParent` boundary: at the archive root, parent = the zip's containing dir (same volume, plain path nav —
  simpler than the plan's cross-volume bubble since the tab id never changed).
- Path bar: `breadcrumbDisplayPath` / `enrichBreadcrumbSegments` receive parent `volumePath`, so `/foo.zip/inner`
  segments render with no prefix-stripping change; verify segment navigation targets inside the archive work.
- `FileEntry.is_archive` (backend-computed, extension-only at listing time) drives the `handleNavigate` fork: navigate
  INTO on Enter (this milestone enters directly; the Ask menu is the next milestone).
- Persistence: already archive-safe under this model (parent id + full path round-trip; `resolveVolumeId` re-derives the
  parent). Verify the unreachable-path timeout path treats a deleted zip sanely.
- Drag-out source (`resolveSourceVolumeId` longest-prefix over real volumes → parent drive): fine under this model —
  backend resolve re-routes by path; verify `scan_for_copy` routes into the archive from `(parentId, inner path)`.
- MCP `cmdr://state`: report parent id + full path (agents navigate by path).

## Watch items for review

- The two VolumeKind unions stay separate on purpose (capabilities vs tint).
- `isMtpVolumeId(displayId)` gates fire for archive-on-MTP — acceptable now, revisit with remote-backed archives.
- Listing-cache identity (backend item 1) is the highest-risk mechanical detail.
- Boundary detection false positives: a real DIRECTORY literally named `foo.zip` must win over archive routing (plan
  test list); the shared helper must stat-check kind before treating a component as an archive.
