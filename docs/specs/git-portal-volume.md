# The git portal becomes a routed volume in `crates/cmdr-git`, and `LocalPosixVolume` stops knowing about git

**Problem.** The virtual `.git` portal (browsable `branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`,
`submodules/`) is implemented as ten `if` sites inside `LocalPosixVolume`: three route hooks on list, metadata, and
read, plus seven `is_virtual` guards on every mutation method. Two more hand-enforced rules ride on top (the listing
layer must skip watching virtual paths; the toggle must manually refresh open listings), the docs call the hook order
"load-bearing", and the module holds English (`pluralize(n, "file")`, "Pinned at commit …") that 10 translations never
see. The volume-aware delete walker (every non-boot volume: an external disk, a share, a phone) lists through the hooked
`list_directory`, so a delete of a repo meets all six virtual folders and refuses each with `NotSupported`. The same
guard refuses the REAL `.git/config` and `.git/HEAD` as well, because `is_virtual` matches any `.git` path segment
rather than a virtual category, so that delete stops with the repo half-gone. The other three walkers are clean: the
copy scan walks with `walkdir` against the resolved path and never asks the volume; the LOCAL delete walker falls back
to `read_dir` because the listing oracle declines every `.git` listing (`listing/streaming.rs` arms no watch under
`.git`, so coverage reads `None`); and the drive index walks with raw syscalls inside `cmdr-index`, which can't reach
the git module at all. All of that is pinned by `apps/desktop/src-tauri/src/file_system/git/walker_exposure_tests.rs`
(verified on macOS 26.6, `cargo test --lib`, 2026-09-05). And the git module's first reason for `local_posix` being
permanently app-resident is these hooks.

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
- **A linked worktree's `.git` is a FILE, so the overlay can't key on "a directory named `.git`".** `classify` splits on
  the path SEGMENT and never stats, so the portal answers in a linked worktree exactly as in the main one, and
  `virtual_listing::list_root` follows the gitlink to `<common>/worktrees/<name>/` and lists its real entries under
  rewritten `<linked>/.git/…` paths (verified 2026-09-05, `a_linked_worktree_serves_the_portal_from_a_dot_git_file`).
  Two consequences for M2: the contributor's predicate is "the last segment is `.git`", ❌ never `is_dir`; and once the
  hooks come out, `Volume::list_directory(<linked>/.git)` is a `read_dir` on a file (`ENOTDIR`), so the overlay's six
  entries would land on an errored listing. Today's rewritten real entries are already un-openable (`<linked>/.git/HEAD`
  doesn't resolve), so dropping them is a fix rather than a loss; what M2 owes is a listing that succeeds. Simplest
  shape: route `.git` itself to the portal volume when it's a gitlink, or let the overlay stand in for a parent listing
  that failed `ENOTDIR` on one.
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

Done. The findings are in the Problem paragraph and the linked-worktree routing bullet above, pinned by
`apps/desktop/src-tauri/src/file_system/git/walker_exposure_tests.rs`, and the two decision paragraphs
(`apps/desktop/src-tauri/src/file_system/volume/backends/DETAILS.md` § "Per-backend decisions",
`crates/cmdr-fs/src/volume/host/DETAILS.md` § "Which backends move") now rest on the two reasons that outlive this plan.

**Open question for David.** The volume-delete `NotSupported` on real `.git/*` files is a live data bug, not only a
portal wart: on an external disk, deleting a repo folder leaves `.git/` behind. M2's routing fixes it structurally (the
guard disappears with the hooks), so M0 left it standing rather than patching seven sites that are about to be deleted.
If it should ship sooner, narrowing the guards from `is_virtual` to `classify(..).is_some()` is a small separate change.

### M1: English out (in place)

- `GitEntryMeta` in `cmdr-fs`, producers in `virtual_listing.rs` / `log.rs` / `submodules.rs` switched, `display_size`
  and its tooltip deleted, frontend wording from the catalog with a Vitest cell per variant and locale plural rule.
- Gate: `pnpm bindings:regen` diff shows exactly the field swap and nothing else. `pnpm check`.

### M2: route + overlay (in place)

Done. Steps 1 and 2 landed the routed volume and `RoutedKind`; steps 3-5 landed the seam, the removal, and the gate. The
shape as built, and the three things it decided that the plan left open:

- **The overlay's predicate is "the listed directory is called `.git`, on a volume answering `local_path().is_some()`,
  with the portal on"**, and the ROUTE asks the volume half of the same question. The plan worried about keying on
  `is_dir`; the answer is that the overlay contributes to a listing that already succeeded, so a linked worktree's
  gitlink FILE excludes itself (`ENOTDIR`) with no stat of our own. That worktree keeps every category below `.git/`,
  and loses only the landing listing, whose rewritten real rows were never openable anyway.
- **`.git/` itself is watched now**, since the `is_virtual` watch skip is gone and it's an ordinary local directory. Two
  consequences the plan didn't name: a watcher-driven `FullRefresh` has to re-run the overlays (it does, in
  `caching::notify_full_refresh_locked`), and a watched `.git/` listing would otherwise read as authoritative to the
  fresh-listing oracle. `CachedListing` records the contributed-row count and the oracle declines any listing carrying
  some, which is the general form of "a pane view is not a picture of a directory".
- **The watcher's refresh target was a non-question in the end.** Listings stay keyed on the FE-provided parent drive
  id, and the refresh now matches by PATH across every volume rather than only `DEFAULT_VOLUME_ID` (a repo on an
  external disk used to go stale).

Full rationale is in the code's own docs now: `file_system/git/DETAILS.md` § "Two seams, no hooks",
`file_system/volume/DETAILS.md` § "Architecture", `file_system/listing/DETAILS.md` § "The overlay step".

### M3: the move

Done. `crates/cmdr-git/` holds everything a repository can answer; the app keeps `overlay.rs` and `wiring.rs`. Four
things it decided that the plan left open:

- **The crate has NO public module.** All 12 promises arrive as root re-exports, so a host can name no path into it,
  which is tighter than any backend crate before it. `GitPortal`'s methods are therefore reachable but unmeasured, so
  what holds them is the item-by-item list in `crates/cmdr-git/DETAILS.md` § "The public surface is capped", not the
  ceiling.
- **`volume_holds_real_repos` landed app-side**, in `wiring.rs`. It reads a `Volume` capability
  (`local_path().is_some()`), both callers are the app's two seams, and nothing in the crate ever asks it.
- **Two statics stayed**, and the crate's `DETAILS.md` draws the line: a memo keyed by content (`snapshot_dates`) or by
  `(root, mtime)` (`status`) is correct for any number of portals, so it may; anything owning a resource's lifecycle
  (the `RepoCache`) may not, and became a portal field.
- **The test files moved with their subject**, which overlaps M4's split. Leaving them app-side would have forced a
  `pub` on `path::classify`, `Cat`, and every category lister, which is exactly the widening the ceiling exists to
  prevent. Six cells stayed behind or were folded into an app cell: the toggle, the two watcher-invalidation ones, and
  the walker-exposure set.
- **`gix` left the app manifest** (nothing else used it); `notify`, `notify-debouncer-full`, and `walkdir` stayed,
  because the local, downloads, file-viewer, and Linux-volume watchers still use them.

One unrelated fix rode along: `cmdr-fs` was borrowing `tokio/macros` from whichever consumer happened to enable it, so
`cargo check -p cmdr-git` was the first build to find it missing. It declares the feature itself now.

### M4: tests and docs

- The split by what a cell asserts largely landed with M3 (see above). What's left is a pass over both sides for cells
  that ended up on the wrong one, and `crates/cmdr-git/src/tests.rs`, whose name no longer says what it holds.
- The crate's `C+D.md` pair, the shrunk `file_system/git/` pair, `docs/architecture.md`, `AGENTS.md`, and the
  `lock-poison` allowlist carry-overs landed with M3. What's left: `file_system/volume/CLAUDE.md` + `DETAILS.md` (§
  "Architecture" gains the overlay seam beside the registry), `backends/DETAILS.md`, and
  `apps/desktop/src/lib/file-explorer/git/CLAUDE.md` (the typed meta). Anything new in an allowlist is a finding, not a
  silent bump.
- Manual QA (David): browse each of the six categories, copy a file out of a branch tree to another volume, edit
  `.git/config` in place, delete a repo folder, toggle the portal off and on with a `.git/` pane open, open a linked
  worktree's `.git`.

## Cost to finish

About three days of agent work. M2 is the half that matters and the half that can surprise (the 23 `is_archive` readers,
the watcher's refresh target); M1 is a morning; M3 and M4 are a day together, mostly `missing_docs` and the test split.
