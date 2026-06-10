# File system › git

Backend module for the git browser: repo discovery/info/status, the per-repo watcher, and the virtual `.git` portal
(`branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`, `submodules/` browsable as virtual trees), with cross-volume
copy "for free" because git blobs flow through the existing `VolumeReadStream` abstraction.

Frontend counterpart: [`apps/desktop/src/lib/file-explorer/git/CLAUDE.md`](../../../../../src/lib/file-explorer/git/CLAUDE.md)
for the breadcrumb chip, status column, and the live `RepoInfo` store.

## Module map

- `mod.rs`: public API + the three volume hooks (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`) +
  `is_virtual` for the mutation guards. `repo.rs`: discovery, `repo_info`, process-global `RepoCache`. `path.rs`:
  `VirtualGitPath` / `classify` parser. `virtual_listing.rs`, `log.rs`, `stash.rs`, `worktrees.rs`, `submodules.rs`,
  `tree.rs`, `snapshot_dates.rs`: per-category listing + tree walks. `status.rs`: cached status walk.
  `read_blob.rs`: `GitBlobReadStream`. `watcher.rs`: per-repo notify debouncer. `friendly.rs`: `FriendlyGitError`.
  `column_meta.rs`: Modified/Size column helpers.
- Full per-file roles, Tauri commands, the watcher path set, and the column tables are in [DETAILS.md](DETAILS.md).

## Must-knows

- **Volume hook order is fixed and load-bearing: `resolve(path)` first, then `git::try_route_*(resolved_path)`.** If the
  route returns `Some`, that's the volume method's return; otherwise the real-FS path runs. Lets the user open `.git`
  from any volume-rooted path. See [DETAILS.md](DETAILS.md) § "Volume hook contract".
- **Mutation guards don't consult the portal toggle.** All mutation methods reject virtual paths via `git::is_virtual`
  even with the portal off: don't let a copy dialog write to `.git/HEAD`. Power users mutate `.git` from a terminal.
- **Flipping the portal toggle must invalidate open virtual listings.** `set_show_virtual_git_portal` flips the atomic
  AND calls `watcher::refresh_all_virtual_listings_after_toggle`; the atomic alone leaves panes showing stale cached
  children. See [DETAILS.md](DETAILS.md) § "Live-toggleable portal".
- **Listings on virtual portal paths must skip `start_watching`.** The on-disk path doesn't exist, so `notify` errors
  ("No path was found") and spams the warn log every navigation. Skip when `git::is_virtual(path)`; virtual-listing
  invalidation flows through `git::watcher::invalidate_virtual_listings` instead.
- **Use typed `VolumeError::FriendlyGit(FriendlyGitError)`; never stuff a sentinel string into `IoError::message` and
  parse it.** That violates the no-error-string-match rule. Same rule keeps `list_status` on `gix::Repository::status()`
  rather than a `git status --porcelain` shell-out (no stderr string parsing).
- **`GitBlobReadStream` memory cost equals blob size** (gix 0.81 has no chunked loose-object reader; the 256 KB chunks
  are for the consumer API shape, not memory streaming). Blobs over `tree::MAX_BLOB_BYTES` (256 MB) are refused up-front
  via `BlobTooLarge` rather than OOM.
- **`repo_info` is the expensive call in the chip pipeline** (`is_dirty()` runs a full worktree walk, ~60 ms on 50k
  files). Don't add work to the chip-refresh path without re-benchmarking.
- **`list_status` is cached keyed by `.git/index` mtime**; the watcher drops the entry on every `.git/*` mutation. A
  naive per-nav walk costs ~75 ms on a 50k-file repo. See [DETAILS.md](DETAILS.md) § "Decisions".
- **Streaming log is capped at 5000 entries, silently** (no "Load more": pagination IPC isn't wired, so the affordance
  would do nothing). Wire the IPC and the affordance together when a user first reports hitting the cap.
- **Ref names render flat**: `feature/foo` is one entry, not nested. The classifier greedy-matches known refs
  longest-first. See [DETAILS.md](DETAILS.md) § "Ref-name flat rendering".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
