# File system › git

Backend module for the git browser: repo discovery/info/status, the per-repo watcher, and the virtual `.git` portal
(`branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`, `submodules/` browsable as virtual trees), with cross-volume
copy "for free" because git blobs flow through the existing `VolumeReadStream` abstraction.

Frontend counterpart: `apps/desktop/src/lib/file-explorer/git/CLAUDE.md`.

## Module map

- `volume.rs`: `GitPortalVolume`, the read-only `Volume` the resolve routes virtual paths to. `overlay.rs`: the
  `ListingOverlay` contributor that puts the six category rows into a repo's `.git/` listing. `portal.rs`:
  `GitPortal`, which owns the `RepoCache` and mints one volume per repo. `mod.rs`: public API and the portal toggle.
  `repo.rs`: discovery, `repo_info`, `RepoCache`. `path.rs`: `VirtualGitPath` / `classify_in` parser and the lexical
  `portal_route`. `virtual_listing.rs`, `log.rs`, `stash.rs`, `worktrees.rs`, `submodules.rs`,
  `tree.rs`, `snapshot_dates.rs`: per-category listing + tree walks. `status.rs`: cached status walk.
  `read_blob.rs`: `GitBlobReadStream`. `watcher.rs`: per-repo notify debouncer, reporting through `state_sink.rs`;
  `wiring.rs` is the app's answer to a report. `column_meta.rs`: Modified/Size column helpers, which hand back numbers
  and ids on a typed `git_meta`, never words. `FriendlyGitError` is in `cmdr-fs`, aliased here as `git::friendly`.
- Tauri commands, the watcher path set, the column tables, and the decision record are in `DETAILS.md`.

## Must-knows
- **Two seams, no hooks: `LocalPosixVolume` names git nowhere, and ❌ must never again.** Below `.git/` is a ROUTE
  (`resolve` → `GitPortalVolume`); `.git/` itself is a listing OVERLAY (`overlay.rs`) that reaches a PANE and nothing
  else. `DETAILS.md` § "Two seams, no hooks".
- **❌ Never widen either seam.** The volume serves the six categories and nothing under them; the overlay claims only
  a DIRECTORY called `.git` on a volume `gix` can open. So a linked worktree's gitlink FILE has no landing listing, and
  a `.git` on a direct-SMB share isn't the portal's.
- **Real files under `.git` are ordinary local files**, portal on or off, which is what lets a repo-folder delete walk
  `.git/` to the end. ❌ Never add a guard back: the last one refused `.git/config` too and half-deleted repos.
- **Flipping the toggle must refresh open listings.** `set_show_virtual_git_portal` flips the atomic AND calls
  `wiring::refresh_all_virtual_listings_after_toggle`, across every volume; the atomic alone leaves stale children on
  screen.
- **The watcher holds no `AppHandle`**: it reports through `GitStateSink`, and the event plus the pane refreshes are
  `wiring.rs`'s.
- **A virtual listing is unwatchable by TYPE** (`can_watch_listings()` false), and `.git/` itself IS watched. So a
  `FullRefresh` there must re-run the overlays, and the fresh-listing oracle must keep declining a decorated listing.
  `DETAILS.md` § Gotchas.
- **A path that isn't in a snapshot is `NotFound`, ❌ never `CorruptRepo`.** Lookups that can find nothing answer
  `Lookup<T>` (`Result<Option<T>, FriendlyGitError>`) and `found_or_not_found` folds a `None` into
  `VolumeError::NotFound` carrying the path. See `DETAILS.md` § "A miss is not a damaged repo".
- **Use typed `VolumeError::FriendlyGit(FriendlyGitError)`, ❌ never a sentinel string in `IoError::message`.** Same
  rule keeps `list_status` on `gix::Repository::status()` rather than parsing porcelain output.
- **`GitBlobReadStream` costs one blob of RAM**; its chunks are an API shape, not memory streaming. Blobs over
  `tree::MAX_BLOB_BYTES` are refused. `DETAILS.md` § "Honest blob streaming".
- **`repo_info` is the expensive call in the chip pipeline** (`is_dirty()` walks the worktree), so don't add work to
  the chip-refresh path without re-benchmarking. `DETAILS.md` § "Performance"; § "Decisions" also covers `list_status`
  caching, the flat `feature/foo` ref rendering, and the 5000-entry log cap.
- **The Size column carries a FACT, ❌ never a sentence.** Every virtual row sets `FileEntry.git_meta`
  (`cmdr_fs::git_meta::GitEntryMeta`) and the frontend words it from the catalog. A new row shape means a variant plus
  its two catalog keys.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
