# File system › git

Backend module for the git browser: repo discovery/info/status, the per-repo watcher, and the virtual `.git` portal
(`branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`, `submodules/` browsable as virtual trees), with cross-volume
copy "for free" because git blobs flow through the existing `VolumeReadStream` abstraction.

Frontend counterpart: `apps/desktop/src/lib/file-explorer/git/CLAUDE.md`
for the breadcrumb chip, status column, and the live `RepoInfo` store.

## Module map

- `volume.rs`: `GitPortalVolume`, the read-only `Volume` the resolve routes virtual paths to. `portal.rs`:
  `GitPortal`, which owns the `RepoCache` and mints one volume per repo. `mod.rs`: public API + the three volume hooks
  (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`) + `is_virtual` for the mutation guards.
  `repo.rs`: discovery, `repo_info`, `RepoCache`. `path.rs`: `VirtualGitPath` / `classify` parser and the lexical
  `portal_route`. `virtual_listing.rs`, `log.rs`, `stash.rs`, `worktrees.rs`, `submodules.rs`,
  `tree.rs`, `snapshot_dates.rs`: per-category listing + tree walks. `status.rs`: cached status walk.
  `read_blob.rs`: `GitBlobReadStream`. `watcher.rs`: per-repo notify debouncer. `column_meta.rs`: Modified/Size column
  helpers, which hand back numbers and ids on a typed `git_meta`, never words. `FriendlyGitError` lives in `crates/cmdr-fs/src/volume/friendly_error/git.rs`
  (`VolumeError::FriendlyGit` carries it), aliased here as `git::friendly`.
- Tauri commands, the watcher path set, the column tables, and the decision record are in `DETAILS.md`.

## Must-knows

- **A virtual path is a ROUTE, not a hook: `resolve` sends `.git/<category>/…` to `GitPortalVolume`.** The hooks in
  `mod.rs` still serve the portal ROOT (`.git/` itself) and the mutation guards, and both halves answer from the same
  `virtual_listing` code. `DETAILS.md` §§ "The portal is a routed volume", "Volume hook contract".
- **The portal volume serves the six categories and NOTHING else.** Real `.git/*` entries stay on the parent volume,
  which is what keeps `.git/config` editable and a repo-folder delete able to walk `.git/`. ❌ Never widen its
  namespace to real entries.
- **Mutation guards don't consult the portal toggle.** All mutation methods reject virtual paths via `git::is_virtual`
  even with the portal off: don't let a copy dialog write to `.git/HEAD`. Power users mutate `.git` from a terminal.
- **Flipping the portal toggle must invalidate open virtual listings.** `set_show_virtual_git_portal` flips the atomic
  AND calls `watcher::refresh_all_virtual_listings_after_toggle`; the atomic alone leaves stale cached children on
  screen. `DETAILS.md` § "Live-toggleable portal".
- **Listings on virtual portal paths must skip `start_watching`.** The on-disk path doesn't exist, so `notify` errors
  ("No path was found") and spams the warn log every navigation. Skip when `git::is_virtual(path)`; virtual-listing
  invalidation flows through `git::watcher::invalidate_virtual_listings` instead.
- **A path that isn't in a snapshot is `NotFound`, ❌ never `CorruptRepo`.** Lookups that can find nothing answer
  `Lookup<T>` (`Result<Option<T>, FriendlyGitError>`) and `found_or_not_found` folds a `None` into
  `VolumeError::NotFound` carrying the path. See `DETAILS.md` § "A miss is not a damaged repo".
- **Use typed `VolumeError::FriendlyGit(FriendlyGitError)`, ❌ never a sentinel string in `IoError::message`.** The
  same rule keeps `list_status` on `gix::Repository::status()` rather than parsing `git status --porcelain`.
- **`GitBlobReadStream` costs one blob of RAM**; its chunks are an API shape, not memory streaming. Blobs over
  `tree::MAX_BLOB_BYTES` are refused up-front. `DETAILS.md` § "Honest blob streaming".
- **`repo_info` is the expensive call in the chip pipeline** (`is_dirty()` walks the worktree), so don't add work to
  the chip-refresh path without re-benchmarking. `DETAILS.md` §§ "Performance", "Decisions" (also: `list_status`
  caching, the flat `feature/foo` ref rendering, and the silent 5000-entry log cap).
- **The Size column carries a FACT, ❌ never a sentence.** Every virtual row sets `FileEntry.git_meta`
  (`cmdr_fs::git_meta::GitEntryMeta`); the frontend words it from the catalog, so all 10 translations get the cell and
  its tooltip. A new row shape means a variant here plus its two catalog keys.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
