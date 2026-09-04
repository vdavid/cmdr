# The git portal becomes a routed volume in `crates/cmdr-git`, and `LocalPosixVolume` stops knowing about git

**Problem.** The virtual `.git` portal (browsable `branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`,
`submodules/`) is implemented as ten `if` sites inside `LocalPosixVolume`: three route hooks on list, metadata, and
read, plus seven `is_virtual` guards on every mutation method. Two more hand-enforced rules ride on top (the listing
layer must skip watching virtual paths; the toggle must manually refresh open listings), the docs call the hook order
"load-bearing", and the module holds English (`pluralize(n, "file")`, "Pinned at commit …") that 10 translations never
see. The delete walker lists through the hooked `list_directory`, so a delete of a repo with the portal on may meet six
virtual folders it can't remove (verify at M0). And the git module's first reason for `local_posix` being permanently
app-resident is these hooks.

**Shape.** The same mechanism archives already use. A path crossing a `.zip` isn't a hook inside the local backend:
`VolumeManager::resolve` routes it to a read-only `ArchiveVolume` in its own crate. The portal is the same shape. A
read-only `GitPortalVolume` in `crates/cmdr-git`, routed by `resolve` when a path crosses `.git/<category>/`, with the
`.git/` root listing decorated by a small overlay seam in the pane's listing pipeline. Three rules become types: a
read-only volume refuses writes by default, a volume with `can_watch_listings() == false` is never watched, and the
toggle is "resolve doesn't route" plus "the overlay doesn't contribute". `local_posix.rs` loses all ten sites and never
names git again.

**Constraint from David.** `.git/` stays writable. Real files under it (`config`, `HEAD`, hooks) stay editable,
renamable, and deletable through Cmdr, and deleting a repo walks `.git/` as a plain local directory. Only the six
virtual trees are read-only, because they don't exist on disk. The design below has no compromise on this.

**Order.** After `mtp-crate-extraction.md` lands. This plan reuses that one's conventions (the two faces of a backend,
`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Architecture"; the `testing` feature; no `backends/` shim;
app-side tests beside what they assert on) and its worked example for a typed event trait replacing a
`tauri::AppHandle`. The two only touch the same lines in the volume docs and `backends/mod.rs`, so if MTP stalls, this
one can go first.

## The routing design

- **Routing is lexical and per virtual subtree.** `resolve(volume_id, path)` routes to the portal only when the path
  contains a `.git/<category>/` segment for one of the six categories (or is exactly one of those category directories).
  It does no disk I/O to decide: the portal volume discovers the repo on first use through its own `RepoCache` and
  answers `NotFound` for a `.git` that isn't a repository. `.git/` itself, `.git/config`, `.git/HEAD`, and everything
  non-virtual stays on the parent volume, so every mutation on real files keeps its current behavior with no guard
  anywhere. A real directory literally named `.git/branches/` is shadowed while the portal is on, which is today's
  behavior too (the classifier hides the deprecated `.git/branches/` and linked-worktree `.git/worktrees/`).
- **One `GitPortalVolume` per repo root**, registered on demand and LRU-capped the way `register_archive` does it in
  `file_system/volume/manager/archive_routing.rs`. It maps the full input path to `(repo, category, ref, tree path)`
  through the existing `path::classify`, so `ResolvedVolume.path` stays the input path verbatim, as for archives.
- **`ResolvedVolume.is_archive: bool` becomes `routed: Option<RoutedKind>`** with `Archive` and `GitPortal` variants.
  The 23 non-test readers of `is_archive` skip drive-index enrichment and the write guards for a routed volume; a git
  virtual listing needs exactly the same skips. A `fn is_routed(&self)` keeps most call sites one-line.
- **The `.git/` root listing is an overlay, not a route.** A new `ListingOverlay` seam (registry module beside
  `device_volumes.rs`, same registration shape) runs in `listing/streaming.rs::read_directory_with_progress` after the
  volume's entries arrive and before the cache insert:
  `fn extra_entries(&self, volume: &dyn Volume, path: &Path) -> Vec<FileEntry>`. The git contributor answers only for a
  local-FS volume and a directory whose name is `.git`, and contributes the six virtual directory entries. Merge rule:
  contributed entries shadow real ones of the same name. The overlay applies to PANE listings only: copy scans, the
  delete walker, and the indexer list through the volume and never see a virtual folder. That is the structural fix for
  the M0 bug above.
- **The toggle** (`set_show_virtual_git_portal`) flips one app-side switch that both the route and the overlay consult,
  then refreshes open `.git/` listings (existing `refresh_all_virtual_listings_after_toggle`). An open pane on a virtual
  path when the portal turns off gets `NotFound` from `resolve` and the pane's existing recovery runs.
- **Reads from a virtual tree keep streaming.** `GitPortalVolume::open_read_stream` is today's `GitBlobReadStream`, so
  cross-volume copy out of `.git/branches/main/` works as before. `is_writable` false, `can_watch_listings` false,
  `listing_watch_coverage` `None`; every mutation method keeps the trait default (`NotSupported`).
- **The git watcher's listing refresh targets the portal.** `watcher.rs` today walks the listing cache for
  `.git/{branches,tags}/` entries cached under the LOCAL volume id; after routing they're cached under the portal
  volume's id. `find_listings_for_path_on_volume` gets the portal id, or the lookup goes by path across volumes.

## What moves, what stays

- **Crate `crates/cmdr-git/`** (no `tauri`, no `cmdr`, no English): `volume.rs` (`GitPortalVolume`), `path.rs`,
  `virtual_listing.rs`, `log.rs`, `stash.rs` (its two `git stash` shell-outs are fine in a crate; `cmdr-adb` shells out
  the same way), `worktrees.rs`, `submodules.rs`, `tree.rs`, `snapshot_dates.rs`, `read_blob.rs`, `column_meta.rs`
  (typed, see decision 1), `repo.rs` (discovery, `RepoInfo`, `RepoCache`), `status.rs`, the watcher's `notify` half
  behind a `GitStateSink` trait (decision 3), `bench.rs`, and `test_fixtures.rs` under `testing`. Deps: `gix = "0.87"`,
  `notify = "8"`, `notify-debouncer-full = "0.7.0"`, `walkdir = "2"` at the versions the app pins today, ❌ no bumps as
  a side effect; `specta = "=2.0.0-rc.24"` for `RepoInfo` and `EntryStatus`. `FriendlyGitError` is already in `cmdr-fs`.
- **App `src-tauri/src/file_system/git/`** keeps: `wiring.rs` (registers the route in `VolumeManager`, the
  `ListingOverlay` contributor, the `GitStateSink` adapter that emits `git-state-changed` and refreshes listings, and
  the toggle switch), and `commands/file_system/git.rs` unchanged in shape.
- **`local_posix.rs`**: minus ten sites. `listing/streaming.rs`: minus the `is_virtual` watch skip. `notify_mutation`:
  minus its virtual early return.

## Decisions

1. **English leaves the backend as a typed field.** `FileEntry.display_size` / `display_size_tooltip` have no producer
   outside git (measured 2026-09-03). Replace both with `git_meta: Option<GitEntryMeta>` in `cmdr-fs`:
   `Count { kind: GitCountKind, n }` (branches, tags, commits, stash entries, linked worktrees, submodules, files
   changed), `AheadBehind { ahead, behind }`, `PinnedCommit { id }`, `TaggedCommit { id }`. The frontend
   (`file-explorer/views/full-list-utils.ts`, where `displaySize` is rendered verbatim today) words each variant from
   the catalog with plural rules per locale, keys under `src/lib/intl/messages/en/` with translator descriptions, per
   `docs/guides/i18n.md`. `crate::pluralize` loses its last backend caller here; leave it if others remain.
2. **Routing is lexical (no I/O in `resolve`).** Why: `resolve` runs on every path-bearing call, and archive routing
   already pays a `get_metadata` plus a magic read only for remote parents. Git classification is a path shape; repo
   discovery is the portal volume's job on first call, cached.
3. **The watcher moves with a typed sink.** `GitStateSink::repo_changed(repo_root, RepoInfo)` replaces the `AppHandle`
   and the `git-state-changed` emit; the app adapter emits the existing `GitStateChangedPayload` (the
   `tauri_specta::Event` stays app-side) and runs the listing refresh through the app's cache API. Same shape as the MTP
   plan's `MtpDeviceEvents`.
4. **`GitPortal` is a value the app parks.** It owns the `RepoCache` and the sink, is built by `wiring.rs` at startup
   (`VolumeHost` in, for `runtime()` and `listings()`), and `resolve` mints `GitPortalVolume`s from an `Arc<GitPortal>`.
   No process-global `RepoCache` in the crate.
5. **`local_posix` stays app-resident.** Its other two reasons hold (`listing/reading.rs` and the definitionally-local
   `patch_listing_after_local_mutation`). Revise `backends/DETAILS.md` § "Per-backend decisions" to drop the git reason.
6. **Overlay entries are pane-only, by construction.** ❌ Never move the overlay into a `Volume` impl or into the volume
   manager: the moment a scan or a walker can see a virtual folder, the M0 bug returns.

## Milestones

Worktree from `~/.claude/scripts/new-worktree.sh git-portal-volume`. `pnpm check --fast` while iterating, `pnpm check`
per milestone. M1 and M2 happen in place under today's paths; the move (M3) waits until M2 is green.

### M0: verify and record

- Verify the delete-walker exposure: with the portal on, delete a small repo directory through Cmdr (or the delete
  walker's unit harness) and record what happens to the six virtual entries. Verify `scan_for_copy_batch` and the
  indexer's BFS the same way. Write the finding into this spec's Problem paragraph as fact.
- Verify linked-worktree behavior (`.git` is a FILE there) so the overlay's "directory named `.git`" rule matches what
  `classify` does today.
- Revise the two decision paragraphs (decision 5; the host `DETAILS.md` § "Which backends move" line on `local_posix`).
- Green: the docs checks.

### M1: English out (in place)

- `GitEntryMeta` in `cmdr-fs`, producers in `virtual_listing.rs` / `log.rs` / `submodules.rs` switched, `display_size`
  and its tooltip deleted, frontend wording from the catalog with a Vitest cell per variant and locale plural rule.
- Gate: `pnpm bindings:regen` diff shows exactly the field swap and nothing else. `pnpm check`.

### M2: route + overlay (in place)

1. `GitPortalVolume` in `file_system/git/volume.rs` implementing `Volume` over today's hook bodies; the read-only subset
   of `volume::conformance` runs against it (TDD: the conformance cell first, red).
2. `RoutedKind` in `ResolvedVolume`; lexical git routing in `resolve` and `resolve_local_only`, LRU registration
   mirroring archives. Routing tests: category paths route, `.git/config` doesn't, toggle off doesn't.
3. `ListingOverlay` registry + the git contributor; the shadowing rule under test; a test that the delete walker and a
   copy scan of a repo see NO virtual entries (the regression anchor for the M0 bug).
4. Remove the ten `local_posix.rs` sites, the `is_virtual` watch skip, and the `notify_mutation` early return. Rewire
   the toggle and the watcher's refresh target (routing design, last bullet).
5. Gate: bindings zero-diff, `pnpm check`, the portal E2E spec (`test/e2e-playwright/git-portal.spec.ts`), and
   `bench.rs` numbers within the budgets in `git/DETAILS.md` § "Performance".

### M3: the move

- `crates/cmdr-git/` modeled on `crates/cmdr-adb/Cargo.toml` (workspace lints, `#![deny(missing_docs)]`, `testing`
  feature, self dev-dependency). Decisions 3 and 4 (the sink and the parked value) as their own commits before the
  `git mv`. `index-crate-isolation`: guarded crate plus surface ceilings justified in the crate's `DETAILS.md`.
  `cargo deny check`. Every `use super::*` prelude in the moving tests replaced first; every rustdoc link to an app
  symbol made prose.
- Gate: `cargo check -p cmdr-git --all-targets` with no app in the graph, bindings zero-diff,
  `pnpm check --include-slow`.

### M4: tests and docs

- Split by what a cell asserts: portal, category, column-meta, snapshot-date, and fixture cells to the crate; routing,
  overlay, toggle, watcher-adapter, and walker-regression cells stay app-side beside the code they exercise.
- `crates/cmdr-git/CLAUDE.md` + `DETAILS.md` from today's `git/` docs (the hook contract section is deleted, not
  rewritten; the performance table and the watcher path set move). `file_system/git/CLAUDE.md` shrinks to the wiring.
  `file_system/volume/CLAUDE.md` + `DETAILS.md` (the "Git delegation hooks" section goes; § "Architecture" gains the
  overlay seam beside the registry), `backends/DETAILS.md`, `docs/architecture.md`,
  `apps/desktop/src/lib/file-explorer/git/CLAUDE.md` (the typed meta). Allowlist entries for moved files carry over at
  their current numbers as a rename; anything new is a finding, not a silent bump.
- Manual QA (David): browse each of the six categories, copy a file out of a branch tree to another volume, edit
  `.git/config` in place, delete a repo folder, toggle the portal off and on with a `.git/` pane open, open a linked
  worktree's `.git`.

## Cost to finish

About three days of agent work. M2 is the half that matters and the half that can surprise (the 23 `is_archive` readers,
the watcher's refresh target); M1 is a morning; M3 and M4 are a day together, mostly `missing_docs` and the test split.
