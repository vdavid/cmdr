# Git browser details

Pull-tier docs for `file_system/git/`: architecture, flows, and decision rationale. Must-know invariants and gotchas
live in `CLAUDE.md`.

Frontend counterpart: `apps/desktop/src/lib/file-explorer/git/CLAUDE.md` for the breadcrumb chip, status column, and the live `RepoInfo` store.

Backend module for the git browser. Provides:

- Repo discovery, repo info, status, and the per-repo watcher.
- The virtual `.git` portal: `branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`, `submodules/` browsable as virtual
  trees, with cross-volume copy "for free" because git blobs flow through the existing `VolumeReadStream` abstraction.
  Branches/tags/commits/stash browse a commit tree; worktrees/submodules surface `redirectToPath` so the frontend opens
  the working dir directly.
- A live toggle for the portal so `cd .git` can fall through to raw on-disk contents.
- Typed git-error classification end-to-end: every git failure reaches `ErrorPane` as a typed `FriendlyGitErrorKind`,
  which the frontend renders into a warm title + explanation + suggestion (`src/lib/error-messages/git-error-messages.ts`).

**The portal root listing mixes real `.git/*` entries (HEAD, config, hooks/, objects/, refs/, …) with the six virtual
categories so the user sees everything in one place.**

## File map

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` § Module
map. What each piece DOES is in the sections below: the hook order in § "Volume hook contract", the toggle in § Decisions
("Live-toggleable portal"), `classify`'s greedy ref matching in § "Ref-name flat rendering", the status cache, the
snapshot-date walk, the 5000-commit cap, the stash shell-out, the `worktrees()` / `submodules()` choices, `RepoCache`'s
longest-root lookup, and the typed `VolumeError::FriendlyGit` variant all in § Decisions, blob memory in § "Honest blob
streaming", and the column semantics in § "Modified + Size columns". Only the layout facts that none of those carry live
here:

- **`virtual_listing.rs` reads real `.git/*` entries with `std::fs`, never through a `Volume`.** Going through the
  volume would recurse straight back into the git hook that called it.
- **`tree.rs` reflects `EntryKind::BlobExecutable` into the entry's permissions**, which is what makes a cross-volume
  copy out of the portal preserve the executable bit.
- **`friendly.rs` is classification only, deliberately word-free.** `kind.category()` maps a variant to an
  `ErrorCategory` and `raw_detail()` builds the technical-details string (kind token + path/raw); ALL user-facing copy
  lives on the frontend in `src/lib/error-messages/git-error-messages.ts`, and so do the writing-rules tests
  (`friendly-error-style.test.ts`, every kind × rendered output). Adding a variant means touching both sides.
- **`watcher.rs` does more than emit `git-state-changed`**: on a relevant `.git/*` mutation it also calls
  `notify_directory_changed(.., FullRefresh)` for any cached `.git/{branches,tags}/` listing on the local volume, so an
  open portal pane refreshes rather than showing stale children.
- **`column_meta.rs`'s count + noun formatting goes through `crate::pluralize`**, not inline string building.
- **`log.rs::resolve_commit_id` resolves a SHA prefix even for an UNREACHABLE commit**, so browsing
  `commits/<sha>` works for something the rev-walk would never list.
- **`walker_exposure_tests.rs` pins which non-pane walkers can reach a virtual entry**, because that set is what the
  portal's blast radius IS. See § "What each walker sees" below.
- **`bench.rs` never runs in a normal suite**: every bench in it is `#[ignore]`d because each builds its own synth
  fixture (a 50k-file repo, a 100-branch repo, a 5k-commit history), cached once under `target/test-fixtures/git/`. The
  run command lives in the module's own header, so it can't drift from the test names.

## What each walker sees

A virtual entry is a name with no inode. A pane renders one; anything that stats, copies, or removes one meets a path
that isn't there. Four walkers matter, and only one of them can reach a virtual entry today (verified on macOS 26.6,
`cargo test --lib file_system::git::walker_exposure_tests`, 2026-09-05):

- **The volume-aware delete walker CAN.** It lists through `Volume::list_directory`, so on any non-boot volume (an
  external disk, a share, a phone) a repo delete meets all six categories and refuses each with `NotSupported`.
- **The same guard also refuses the REAL files.** `is_virtual` matches any `.git` path SEGMENT, not a virtual category,
  so `delete`, `rename`, `create_file`, `create_directory`, and `write_from_stream` all refuse `.git/config` and
  `.git/HEAD` too, with the portal off as much as on. A volume-routed delete of a repo folder therefore stops with
  `.git/` still on disk. The routing work in `docs/specs/git-portal-volume.md` removes the guards along with the hooks.
- **The copy scan CAN'T.** `local_posix/scan.rs` walks with `walkdir` against the resolved path and never asks the
  volume for a listing.
- **The LOCAL delete walker CAN'T.** It prefers a cached listing over `read_dir` only when the listing's watch covers
  every writer, and `listing/streaming.rs` arms no watch anywhere under `.git`, so `listing_watch_coverage` answers
  `None` and the oracle declines. That one fact is what keeps the boot-volume delete and the scan preview honest; a
  future watch armed under `.git` would hand six phantom rows straight to a delete walker.
- **The drive index CAN'T.** Local volumes are walked by `cmdr-index`'s guarded walker with raw syscalls, and that crate
  can't name the app's git module. SMB and MTP go through the trait scanner, but the portal hooks live only in
  `LocalPosixVolume`, so a `.git` on a share never routes.

## Linked worktrees

`git worktree add` writes `.git` as a FILE holding `gitdir: <common>/worktrees/<name>`. `path::classify` splits on the
path segment and never stats, so the portal answers there exactly as in the main worktree. `read_real_dot_git` follows
the gitlink and lists `<common>/worktrees/<name>/`'s real entries (`HEAD`, `index`, `refs/`, `commondir`, `gitdir`,
`logs/`) under rewritten `<linked>/.git/…` paths.

**Gotcha**: those rewritten paths don't resolve. `<linked>/.git/HEAD` is a path THROUGH a file, so reading it fails with
`ENOTDIR`; the rows are visible but not openable.
**Why**: the rewrite exists so the pane's breadcrumb stays inside the worktree the user is standing in. Fixing it means
either mapping each row back to its real gitdir path or dropping the real rows in a linked worktree; both are decisions
the routing plan makes, so don't paper over one here.

## Tauri commands

Wired from `commands/file_system/git.rs`:

- `get_git_repo_info(path) -> TimedOut<Option<RepoInfo>>` – one-shot lookup, 2 s timeout
- `subscribe_git_state(repo_root) -> Result<RepoInfo, GitSubscribeError>` – registers a subscriber, returns current `RepoInfo` synchronously, then emits `git-state-changed` events. 2 s timeout (the synchronous handshake calls `discover_repo` + `repo_info` so a hung repo would otherwise freeze IPC). `GitSubscribeError` (in `commands/file_system/git.rs`) is `Git { error: FriendlyGitError }` / `TimedOut` / `Unexpected { detail }`, so git's own typed kind reaches the frontend intact and `src/lib/error-messages/git-error-messages.ts` words it per locale
- `unsubscribe_git_state(repo_root) -> ()` – drops one subscriber; tears down the watcher when refcount hits zero
- `get_git_status_for_paths(repo_root, dir) -> TimedOut<Vec<EntryStatus>>` – gix status walk, 5 s timeout
- `set_show_virtual_git_portal(enabled)` (in `commands::settings`) – flips the live portal toggle. Pushed by `settings-applier.ts` whenever `fileExplorer.git.showVirtualGitPortal` changes

## Watcher path set

- `<repo>/.git/HEAD`
- `<repo>/.git/ORIG_HEAD`
- `<repo>/.git/MERGE_HEAD`
- `<repo>/.git/FETCH_HEAD`
- `<repo>/.git/refs/` (recursive)
- `<repo>/.git/packed-refs`
- `<repo>/.git/index`
- `<repo>/.git/logs/HEAD`

Plus a non-recursive watch on `.git` itself so creating optional files
(`MERGE_HEAD` during a merge) still triggers a recompute. Linked worktrees
have their `.git` as a file (gitlink); the watcher resolves the gitdir
through it.

Per-worktree `HEAD` watches: at subscribe time we enumerate
`<common-dir>/worktrees/<name>/HEAD` files and register one watch each.
That keeps the chip live for every linked worktree. New worktrees added
later are picked up indirectly via the main-HEAD watch (`git worktree
add` writes to the main repo's `HEAD` too).

## Performance

Bench result on a 50k-file synth repo (Apple M-series, release build):

| Metric | Budget | Measured |
|---|---|---|
| `discover_repo + repo_info` p50 | 50 ms | ~61 ms |
| `discover_repo + repo_info` p95 | 50 ms | ~64 ms |
| `list_status` p50 | 100 ms | ~73 ms |
| `list_status` p95 | 100 ms | ~75 ms |

`list_status` lands well inside budget. `discover + repo_info` runs ~14 ms
over the aspirational 50 ms target – `is_dirty` does a full worktree walk,
and even shelling out to `git status --untracked-files=no` (the lightest
is-dirty check the CLI offers) takes ~75 ms on the same fixture, so the
target is a hair tighter than what any tool can deliver here. The hard cap
in the bench is 100 ms; we land well under. Subsequent calls hit the
process-wide repo handle cache and run in microseconds.

## Volume hook contract

The hook order inside `LocalPosixVolume` is fixed and load-bearing:

1. `resolve(path)` runs first (existing). It normalizes absolute vs. relative paths against the volume root.
2. After `resolve`, the volume method calls `git::try_route_*(resolved_path)`. If `Some`, that result is the volume method's return. Otherwise the existing real-FS path runs.

Three hook points:

- `list_directory` → `git::try_route_listing(resolved_path) -> Option<Result<Vec<FileEntry>, VolumeError>>`
- `get_metadata` → `git::try_route_metadata(resolved_path) -> Option<Result<FileEntry, VolumeError>>`
- `open_read_stream` → `git::try_open_blob_stream(resolved_path) -> Option<Result<Box<dyn VolumeReadStream>, VolumeError>>`

All mutation methods (`create_file`, `create_directory`, `delete`, `rename`, `write_from_stream`) detect virtual paths via `git::is_virtual(path)` and return `VolumeError::NotSupported` immediately. `notify_mutation` early-returns for virtual paths since git mutations happen out-of-band; cache invalidation flows through the `.git`-watcher pipeline (`watcher.rs`).

## Honest blob streaming

gix 0.81 returns whole-blob `Vec<u8>` for `Object::data` – there's no chunked loose-object reader exposed at the public surface yet. So `GitBlobReadStream` owns the full `Vec<u8>` and yields 256 KB chunks for the consumer API shape. **Memory cost equals blob size; chunked yield is for the consumer API, not memory streaming.** We refuse blobs over `tree::MAX_BLOB_BYTES` (256 MB) up-front via `FriendlyGitErrorKind::BlobTooLarge` rather than OOM. Revisit when gix exposes a chunked loose-object reader.

## Ref-name flat rendering

Branches like `feature/foo` show as a single entry called `feature/foo`, not nested `feature/` then `foo`. The classifier (`path::classify`) greedy-matches ref names against the repo's known refs (longest-first) before treating any remainder as a tree sub-path. The inverse (`to_path`) splits ref names on `/` so OS-native separators are used in the on-disk representation. This is the only place where the URL → path round-trip needs the repo open.

## Modified + Size columns for virtual entries

Every virtual entry carries a real `modified_at` and most carry a `display_size` string that the frontend renders verbatim in the Full mode Size column. Backend-built; frontend is dumb.

| Path | `modified_at` | `display_size` | `size` (sort key) |
|---|---|---|---|
| `.git/branches/` | newest branch tip date | `12 branches` | branch count |
| `.git/tags/` | newest tag/commit date | `5 tags` | tag count |
| `.git/commits/` | HEAD committer date | `123 commits` | commit count (capped at 5000) |
| `.git/stash/` | newest stash creation date | `3 stash entries` | stash count |
| `.git/worktrees/` | newest linked worktree HEAD | `2 linked worktrees` | worktree count |
| `.git/submodules/` | newest pinned commit | `1 submodule` | submodule count |
| `branches/<name>/` | branch tip committer date | `+12 / -3` vs upstream (or fallback `main`/`master`) | ahead-count |
| `tags/<name>/` | annotated tag date or commit date | short SHA | 0 |
| `commits/<sha>/` | commit committer date | `5 files` (or `1 file`) | files-changed count |
| `stash/<n>/` | stash creation date | `on main` (parsed from stash subject) | 0 |
| `worktrees/<name>` (redirect) | worktree HEAD date | `on feature-x` or short SHA | 0 |
| `submodules/<name>` (redirect) | pinned commit date | short SHA | 0 |
| inside snapshots: files | most recent commit that touched the file (fallback: snapshot commit date) | None (blob bytes) | blob bytes |
| inside snapshots: subdirs | most recent commit that touched any file underneath (fallback: snapshot commit date) | None (recursive bytes) | recursive blob bytes |

Cross-category Size sort is meaningless (ahead-count vs files-changed vs item count); that's an honest tradeoff. Each cell is self-explaining via `display_size_tooltip` (also used as the aria-label).

The frontend reads `display_size` / `display_size_tooltip` from `FileEntry`; the Full mode renderer (`FullList.svelte`) calls `pickSizeDisplay` from `full-list-utils.ts`, and `measure-column-widths.ts` already widens the Size column to fit the override string.

**Decision**: Eager-load ahead/behind for branches; eager-load files-changed for commits
**Why**: Bench (release build, M-series): 100 branches with ahead/behind takes p50=33 ms / p95=36 ms, well under the 300 ms p95 budget the spec sets for the listing pipeline. Files-changed for 200 commits: p50=37 ms / p95=40 ms (200 µs / commit), so the typical Cmdr-sized repo (~3000 commits) lands ~600 ms and the 5000-commit cap lands ~1 s. We accept the worst-case 1 s on the cap because (1) Cmdr's own repo never hits the cap, (2) the listing pipeline runs the hook in `spawn_blocking` so the UI stays responsive, and (3) the alternative (lazy-load via a streamed IPC) would mean another round-trip per row and a placeholder `…` in the cell while it resolves. Worth re-checking if a user reports the 5000-commit cap feeling slow; the bench harness in `bench.rs` covers 1000 commits and `bench_list_commits_files_changed` covers 200.

## Decisions

**Decision**: Mixed real + virtual portal root listing; `raw/` escape hatch dropped
**Why**: Hiding real `.git/*` contents behind a separate `raw/` category meant two extra clicks (open `.git/`, open `raw/`) for anyone wanting to peek at `HEAD`, `config`, `hooks/`, `objects/`, etc. The virtual entries already cover the friendly view; surfacing the real entries in the same listing gives power users one-click access without the `raw/` indirection. The classifier (`path::classify`) returns `None` for any `.git/*` segment that isn't a virtual category name, so the volume hook falls through to the real-FS path automatically. No new code on the read side: the existing LocalPosixVolume handles it. Real entries whose name collides with a virtual category get filtered out: the deprecated `.git/branches/` directory (git itself stopped writing to it years ago) and `.git/worktrees/` in linked-worktree setups (its internals belong to git, not to the user) hide behind the friendly virtual entries. Power users who really want the raw bytes open the gitdir from the terminal. Sort order: real entries dirs-first alphabetical (matching the listing pipeline default), then the six virtual categories in fixed order (branches, tags, commits, stash, worktrees, submodules).

**Decision**: Per-file Modified dates inside snapshot listings via walk-once batching
**Why**: The snapshot date ("when this commit landed") is the same value for every file inside a `branches/main/`, `commits/<sha>/`, etc. listing: semantically correct as a "frozen point in time", but useless as a "when did I last work on this?" hint. We now run a single rev-walk per `(commit_id, dir_path)` listing: from the snapshot commit backwards by commit time, first-parent only, diffing each commit against its first parent (gix's `Tree::changes()::for_each_to_obtain_tree`). Each `Change.location` is matched against the directory's top-level entries; the first-seen commit's committer time wins. The walk stops early when every entry is dated, after `MAX_COMMITS_PER_WALK` (1000), or when the rev-walk exits. Initial commits short-circuit. Cache is process-global, FIFO-bounded at 50 keys, content-addressable so it never invalidates. Bench: 100 entries × 5000 commits cold p95=21 ms (budget 200 ms), warm p95=2 µs. 50k-commit fixture sits inside the 500 ms budget too. Entries that don't surface within the cap fall back to the snapshot date so the cell never reads as blank.

**Decision**: Cache `list_status` results keyed by `.git/index` mtime
**Why**: A naive implementation would walk the worktree on every `listing-complete` (every nav,
every diff). On a 50k-file repo that's ~75 ms per nav. We run one full-repo
walk per index change, store the result in a process-global
`RwLock<HashMap<RepoRoot, CachedStatus>>`, and slice by `dir_in_worktree` on
each call. Cached calls land sub-millisecond on the same fixture (warm p95 in
the bench is bounded by an arbitrary 5 ms ceiling so a busy CI doesn't flake).
The watcher (`watcher.rs::recompute_and_emit`) drops the cache entry on every
`.git/*` mutation it observes, so the next call repopulates. The
`unsubscribe`-on-last-pane path also drops the entry so an unwatched repo
doesn't pin a full-repo-sized snapshot.

**Decision**: Always run with `--untracked-files=normal`, no
"skip untracked outside the worktree root" trick
**Why**: Passing `--untracked-files=no` for sub-path listings would avoid the full untracked walk per call, but with
the index-mtime cache above, the untracked walk runs once per index change anyway and the cost is amortized
across every subsequent listing. The extra complexity (two code paths,
mismatched cache keys for the same repo) buys nothing measurable. We always
walk the full worktree with `--untracked-files=normal` and let the cache do
the work.

**Decision**: Live-toggleable portal via a process-global `AtomicBool`
**Why**: `try_route_listing` / `try_route_metadata` / `try_open_blob_stream`
each early-return `None` when the toggle is off, falling through to the
real-FS path. This keeps the toggle a no-op cost (one atomic load per
hook call). The setter is wired live from the frontend
(`set_show_virtual_git_portal`) and seeded at startup from
`Settings::show_virtual_git_portal`. Mutation guards (`is_virtual` in
`local_posix`) intentionally don't consult the toggle: even with the
portal off we don't want Cmdr to write to `.git/HEAD` from a copy
dialog. Power users who really want to mutate `.git` use a terminal.

**Toggle invalidates open virtual listings.** Flipping the atomic alone
isn't enough: panes already showing a virtual `.git/...` listing keep
their cached children until the next navigation. So
`set_show_virtual_git_portal` also calls
`watcher::refresh_all_virtual_listings_after_toggle`, which iterates
the watcher registry's subscribed repos and emits a `FullRefresh` for
every cached listing under any worktree's `.git/{branches,tags,commits,
stash,worktrees,submodules}/...` (plus `.git/` itself). The helper
`refresh_local_listings_under` is shared with the watcher's
`invalidate_virtual_listings`, so both paths use the same prefix-match
logic and only touch the local volume (SMB / MTP volumes can't be
inside the host's `.git`).

**Decision**: Typed `VolumeError::FriendlyGit(FriendlyGitError)` variant
**Why**: The volume hooks return `Result<_, VolumeError>` and the
streaming pipeline calls `listing_error_from_volume_error` to compute
the `ErrorPane` payload. We carry the structured payload through a
typed enum variant so the path from "git layer detected something" to
"frontend renders the git error pane" is type-checked end-to-end. Don't
revert to stuffing a sentinel-tagged string into `VolumeError::IoError::message`
and parsing it in the friendly mapper – that's string-shaped data inside a
typed enum, a maintenance hazard, and violates the no-error-string-match rule.

**Decision**: Shell out to `git stash list` rather than driving gix
**Why**: gix 0.81 doesn't expose a public stash-list API. We could parse
the `refs/stash` reflog by hand, but `git stash list -z --format=%H%x09%gd%x09%s%x09%ct`
gives us git's canonical ordering, the exact `stash@{n}` indices users
see in the terminal, and the commit-time / subject in one shot. The
`git` CLI is already a system requirement. Resolution of `stash@{n}` to a commit ID also goes through
`git rev-parse stash@{n}` for the same reason – gix can't expand the
`stash@{n}` syntax.

**Decision**: Browse the **W (working-tree) commit** for stash entries
**Why**: `git stash` records the dirty worktree as a merge commit (the
"W" commit in git docs); its first parent ("B") is HEAD at stash time
which is the *clean* tree, not the stashed changes. Browsing W matches
what `git stash show <n>` shows. Verified against fixture: the file
listing under `.git/stash/0/` matches `git stash show 0 --name-only`.

**Decision**: gix `Repository::worktrees()` for the linked-worktree list
**Why**: gix exposes a `worktrees() -> Vec<worktree::Proxy>` that reads
`<common-dir>/worktrees/*/gitdir` and gives us the working-tree base
path via `proxy.base()`. No shell-out needed. We skip proxies whose
`base()` is missing – orphaned linked worktrees stay invisible rather
than break the listing.

**Decision**: gix `Repository::submodules()` for submodule listing
**Why**: gix reads `.gitmodules` and yields one `Submodule` per entry
with name + path. We resolve the submodule's working dir as
`<repo_root>/<rel-path>` and set it on `redirect_to_path` so the
frontend opens the working dir directly. The submodule itself is a
git repo so the portal experience cascades for free.

**Decision**: Streaming log capped at 5000 entries, silent cap
**Why**: Hard cap at 5000 keeps even pathological monorepos
inside the listing pipeline's responsive window. Cmdr's own ~3000-commit
history walks in ~7 ms, so the cap is a safety net, not a UX entry point.
When the cap is hit the walk stops silently (no "Load more" affordance,
because tapping it would do nothing useful: pagination IPC isn't
wired). When the first user reports hitting the cap, add the IPC + a
real Load-more entry together so the affordance actually works.

**Decision**: Volume hook stays single-shot; cancellation via task abort + polled flag
**Why**: The `Volume::list_directory` contract is "compute Vec, return", and the git hook honours that – no
`ListingEventSink` streaming here. Cancellation
works two ways: (1) the listing pipeline's `spawn_blocking` task can be
aborted on cancel, dropping the iterator; (2) we poll a per-process
`AtomicBool` (`log::cancel_flag()`) inside the rev-walk callback every
commit so a *cooperative* cancel takes effect within one commit decode
(microseconds). The flag is opt-in for tests and unused by production
listings (which rely on task abort). Changing to streaming would require
revisiting the hook contract everywhere.

**Decision**: Per-worktree HEAD watch registration on enumeration
**Why**: notify-debouncer-full doesn't natively glob, so
`<common-dir>/worktrees/*/HEAD` can't be expressed as a single watch. We
enumerate worktree gitdirs via `std::fs::read_dir(<common>/worktrees)`
at subscribe time and register one watch per existing `HEAD`. Worktrees
added later are picked up indirectly: `git worktree add` always touches
the main repo's `HEAD` too, which fires our existing main-HEAD watch
and re-emits `git-state-changed`. The cost is a few extra watcher
entries (typical worktree counts are 1-5) – negligible.

**Decision**: `Cat::browses_commit_tree()` covers branches/tags/commits/stash
**Why**: All four categories browse a commit tree, just resolved differently. Branches/tags peel through
refs, commits resolve a SHA prefix, stash expands `stash@{n}`, but the
*tree-walking* code path is identical. The method name describes
the contract. The dispatch lives in `mod.rs::resolve_commit_for_cat`.

**Decision**: Use `gix::Repository::status()` for `list_status` (not a `git status --porcelain=v2 -z` shell-out)
**Why**: In gix 0.81, `Repository::status().into_iter()` runs both a `TreeIndex` leg (HEAD vs index, for staged changes) and an `IndexWorktree` leg (index vs worktree, for unstaged changes) in parallel. The `TreeIndex` leg surfaces staged additions correctly even in single-commit repos. The gix approach is fully typed (no string parsing of stderr), which keeps us inside the no-error-string-match rule. Error mapping is entirely through typed enum variants. Don't reintroduce a shell-out: parsing `stderr.contains(...)` for error classification was the previous failure mode and it's banned.

**Decision**: `discover_repo` rejects bare repos via `BareRepo`
**Why**: The whole UX (chip, status column, future portal) is anchored on
a working tree. Showing a chip for a bare repo is meaningless. The
`FriendlyGitErrorKind::BareRepo` variant tells the user clearly what's up
without claiming a problem.

**Decision**: `RepoCache::lookup_for_path` returns the *longest* matching root
**Why**: HashMap iteration is unordered. With nested submodules both the
parent and the child match `canonical.starts_with(root)`. Picking the
shortest (parent) would surface the wrong repo for paths inside the
child; picking the first match (HashMap order) is non-deterministic.
We pick the longest matching root – that's always the deepest enclosing
worktree, which is the right answer for both submodules and linked
worktrees.

**Decision**: `RepoCache` is process-global, evicted only on the last
unsubscribe (no idle TTL)
**Why**: Re-opening a `gix::Repository` is cheap (~10 ms on warm caches)
but not free; the cache pins one handle per active subscriber so back-to-
back chip lookups skip the open. Eviction stays simple – adding an idle TTL would
need a timer thread for nearly no gain.

**Decision**: Watcher uses `notify-debouncer-full` rather than a custom
poll loop
**Why**: The rest of the codebase already depends on `notify` and
`notify-debouncer-full` for filesystem watching (see `file_system/listing/`
and the SMB share watcher). Reusing it gives us 200 ms debounce, OS-level
event coalescing, and a battle-tested teardown path.

## Gotchas

**Gotcha**: gix's `ThreadSafeRepository::work_dir()` is deprecated but the
new name (`workdir`) only exists on `Repository`, not `ThreadSafeRepository`
**Why**: We hold an `Arc<ThreadSafeRepository>` for the cache (it's `Send + Sync`) and call
`work_dir()` on it once. The deprecation is suppressed inline with a
`#[allow]` carrying that exact reason.

**Gotcha**: The bench tests share one fixture dir (`target/test-fixtures/
git/synth-50k/`). Without a `BUILD_LOCK` mutex, they raced each other into
half-built `.git` dirs when run in parallel
**Why**: `cargo test` defaults to threads-per-core. The fixture builder
checks `dir.join(".git").exists()` to skip rebuild, but the check raced
with the actual `git init`. The mutex serializes the build; concurrent
runs of the test bodies themselves are fine because they only read.

**Gotcha**: `is_dirty()` runs a worktree walk, so `repo_info` is the
expensive call in the chip pipeline
**Why**: On 50k files it dominates the ~60 ms total. Don't add more work
on the chip refresh path without re-benchmarking.

**Gotcha**: `repo_info` reads `.git/config` from the cached repo, but refs from disk, so config changes don't reflect
live while ref changes do
**Why**: The process-global `RepoCache` hands out a long-lived `Arc<ThreadSafeRepository>` that loaded `.git/config`
once at open. Config-derived fields (the upstream, hence `ahead`/`behind`) keep using that snapshot until the cache is
dropped (app restart) — a live `git branch --unset-upstream` won't change the chip. But `repo_info` resolves the
upstream *ref* via `find_reference` on every call, so moving `refs/remotes/origin/main` (for example
`git update-ref refs/remotes/origin/main main` to zero the ahead count) IS picked up on the next chip refresh. This is
the lever the screenshot guide uses to force a clean `main` chip; see `docs/guides/screenshots.md`.

**Gotcha**: Listings on virtual portal paths must skip `start_watching`
**Why**: `listing/streaming.rs` starts a `notify` watcher on the listing's
directory. For virtual paths (`.git/branches/...` etc.) the on-disk path
doesn't exist, so `notify` errors with "No path was found" and the warn
log spams every navigation. The fix: skip the watcher start when
`git::is_virtual(path)`. Cache invalidation for virtual listings flows
through `git::watcher::invalidate_virtual_listings` (via the per-repo
`.git/HEAD`, `refs/`, `packed-refs` watchers), so no notify watch is
needed on the virtual side.
