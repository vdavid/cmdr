# cmdr-git

Everything Cmdr knows about a git repository, with no app around it: repo info and per-entry status for the breadcrumb
chip, and the read-only `Volume` that turns `.git/branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`, and
`submodules/` into browsable trees. The app half (the route, the `.git/` listing overlay, the toggle, and the
`git-state-changed` event) is `apps/desktop/src-tauri/src/file_system/git/wiring.rs`.

## Module map

- `portal.rs` (`GitPortal`, which owns the repo cache, the watcher registry, and the sink, and is what every portal
  volume is built over), `volume.rs` (`GitPortalVolume`), `path.rs` (the `VirtualGitPath` parser, the lexical
  `portal_route`).
- `repo.rs` (discovery, `RepoInfo`, `RepoCache`), `status.rs` (the cached status walk), `watcher.rs` (the refcounted
  per-repo registry, plus the backend that arms it: `notify` in production, scripted under `testing`), `state_sink.rs`
  (where it reports).
- `virtual_listing.rs`, `log.rs`, `stash.rs`, `worktrees.rs`, `submodules.rs`, `tree.rs`, `snapshot_dates.rs`: one
  category each, plus the tree walks. `column_meta.rs` and `read_blob.rs`: the Size/Modified numbers and the blob
  stream. `test_fixtures.rs`: repos to assert against, behind `testing`.

## Must-knows

- **❌ Nothing here may name `tauri`, `tauri_specta`, or `cmdr`.** `cargo check -p cmdr-git --all-targets` is the whole
  verification loop, and `index-crate-isolation` proves the tree stays app-free.
- **The public surface is capped** at what the app uses today, with no headroom: 12 root promises, and EVERY module is
  private, so a host can name no path into this crate. A new `pub` needs David's say-so, like a `file-length` entry. The
  item-by-item argument is in `DETAILS.md`.
- **Everything mutable is a field on `GitPortal`**, ❌ never a static: the repo cache, the watcher registry, the sink.
  The app parks one and a test builds its own. Two memos stay static and `DETAILS.md` says why they may.
- **❌ No English a user reads.** Every Size cell is a typed `GitEntryMeta` the host words from its catalog, and every
  failure is a typed `FriendlyGitError` kind. A sentence in a variant is a bug.
- **A path that isn't in a snapshot is `NotFound`, ❌ never `CorruptRepo`.** Lookups that can find nothing answer
  `Lookup<T>`, and `found_or_not_found` folds a `None` into `VolumeError::NotFound` carrying the caller's path.
- **The `.git/` landing listing is the HOST's, through `GitPortal::category_rows`.** ❌ Never serve those six rows from
  a `Volume`: the moment a copy scan or a delete walker sees a row with no inode behind it, a repo delete stops half-way
  with `.git/` still on disk.
- **Every `gix` call runs on `VolumeHost::runtime().spawn_blocking`**, ❌ never on the caller's async worker.
- **Anything a CONSUMER's test needs takes `any(test, feature = "testing")`, ❌ never `cfg(test)`**, which is off when
  the app compiles this crate as a dependency. `cfg(test)` alone is for doors only this crate's own cells open
  (`snapshot_dates::clear_cache`, `log::cancel_flag`).
- **A subscription cell builds `GitPortal::with_scripted_watcher`, ❌ never `new`.** Arming a real FSEvents stream over
  a repo's ~10 `.git/*` paths is most of what a subscribe costs; `fire_watcher` stands in for the OS. Exactly one cell
  in the repo pays for the real backend, and it's app-side (`wiring_tests`, for the debounce).
- **`missing_docs` is denied.** Every `pub` item says what a caller must know, and specta copies these into
  `bindings.ts`.

The boundary's rationale, the capped surface item by item, the performance table, the column catalog, and every
decision: `DETAILS.md`. Read it before any non-trivial work here.
