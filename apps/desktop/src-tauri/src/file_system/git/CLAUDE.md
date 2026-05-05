# File system › git (complete: M1 + M2 + M3 + M4)

Backend module for the git browser. M1 shipped repo discovery, repo
info, status, the watcher, and the friendly-error skeleton. M2 added
the virtual `.git` portal – `branches/`, `tags/`, `raw/` browsable as
virtual trees, with cross-volume copy "for free" because git blobs flow
through the existing `VolumeReadStream` abstraction. M3 filled in
commits, stash, worktrees, and submodules: the first two browse a
commit tree just like branches/tags; the latter two surface
`redirectToPath` so the frontend opens the worktree's / submodule's
working dir directly. **M4 (this milestone) adds three things: a live
toggle for the portal so `cd .git` can fall through to raw on-disk
contents, FriendlyError integration end-to-end so every git failure
reaches `ErrorPane` with a warm title + explanation + suggestion, and
three new error variants (`ShallowBoundary`, `MissingObject`,
`GitDirPermissionDenied`).**

## File map

| File | Role |
|---|---|
| `mod.rs` | Public API + the three volume hooks (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`) plus `is_virtual` for the mutation guards |
| `repo.rs` | `discover_repo(path)` walking up via `gix::discover` (follows gitlinks). `repo_info(handle, root)` collects branch, detached SHA, unborn flag, upstream, ahead/behind, and `is_dirty`. Process-global `RepoCache` (`Arc<RwLock<HashMap>>`) keyed by canonical worktree root |
| `path.rs` | `VirtualGitPath` enum, `Cat` enum, `classify(path)` parser, `to_path` inverse, `is_virtual(path)` for the volume hook short-circuits. Greedy ref-name match against the repo's known refs so `feature/foo` parses as one ref |
| `virtual_listing.rs` | `list_root` (M2 exposes `branches/`, `tags/`, `raw/`), `list_branches`, `list_tags`, `list_raw` (real-FS passthrough), `get_metadata_for`, `resolve_ref_commit` (annotated tags peel through). Real-FS reads use `std::fs` to avoid recursing through the volume hook |
| `log.rs` | `list_commits` – gix `rev_walk` over HEAD-reachable commits; cap 5000, batch 200, `#[cfg(test)]` cooperative cancel flag (production relies on `spawn_blocking` task abort). `resolve_commit_id` resolves a SHA prefix even for unreachable commits |
| `stash.rs` | `list_stashes(repo_root)`, `resolve_stash_commit(handle, n)` – shells out to `git stash list -z` and `git rev-parse stash@{n}` (gix has no public stash API). `list_stashes` doesn't take a `RepoHandle` because the shell-out only needs `repo_root` for `git -C` |
| `worktrees.rs` | `list_worktrees` – gix `Repository::worktrees()`. Each entry sets `redirect_to_path` to the worktree's working dir |
| `submodules.rs` | `list_submodules` – gix `Repository::submodules()`. Each entry sets `redirect_to_path` to `<repo_root>/<rel-path>` |
| `tree.rs` | `list_tree`, `get_tree_entry`, `lookup_blob_id`, `read_blob` – gix tree walks. Permissions reflect `EntryKind::BlobExecutable` so cross-volume copy preserves the executable bit |
| `read_blob.rs` | `GitBlobReadStream` – owns the full `Vec<u8>` and yields 256 KB chunks. See *Honest blob streaming* below |
| `status.rs` | `list_status(repo, dir)` runs a full-repo `git status --porcelain=v2 -z` once per `.git/index` mtime, caches the result in a process-global `RwLock<HashMap<RepoRoot, CachedStatus>>`, and slices it by `dir`. The watcher invalidates the snapshot whenever `.git/*` changes. Parses porcelain v2 in `parse_porcelain_v2`. |
| `watcher.rs` | `GitWatcherRegistry` – per-repo notify-rs debouncer. `subscribe(app, root)` returns the current `RepoInfo` synchronously and emits `git-state-changed` on relevant `.git/*` mutations. 200 ms debounce. M2: also calls `notify_directory_changed(.., FullRefresh)` for any cached `.git/{branches,tags}/` listings on the local volume |
| `friendly.rs` | `FriendlyGitError`, `FriendlyGitErrorKind` – ten variants (M1's six, `BlobTooLarge` from M2, plus M4's `ShallowBoundary`, `MissingObject`, `GitDirPermissionDenied`). Active-voice copy, no "error" / "failed". `to_friendly_error()` builds a `volume::FriendlyError` for `ErrorPane`; `encode_for_volume_error()` + `try_decode_git_friendly()` carry the structured payload through `VolumeError::IoError` so the streaming pipeline rebuilds it on the way out |
| `column_meta.rs` | Per-row column-population helpers shared across `virtual_listing`, `log`, `tree`, etc. — `pluralize`, `ahead_behind_for_branch`, `commit_meta`, `files_changed_count`, `recursive_tree_size`, plus newest-of-set helpers for category-level Modified dates |
| `tests.rs` | M1 tests: discover, repo_info, status, friendly errors |
| `m2_tests.rs` | M2 tests: classify, list_branches/tags/root, list_tree, blob-read parity with `git show`, cross-volume copy round-trip |
| `m3_tests.rs` | M3 tests: list_commits + sha browsing + cancellation + 1000-commit walk (`#[ignore]`), list_stashes, list_worktrees + redirect, list_submodules + redirect, watcher invalidation for `commits/` |
| `m4_tests.rs` | M4 follow-up tests: Modified + Size column population per category — root counts, branches ahead/behind + sort key, tags short SHA, commits files-changed, stash branch parsing, worktree branch/SHA, submodule pinned SHA, snapshot-interior date + recursive bytes |
| `bench.rs` | `#[ignore]` benchmark over a 50k-file synth fixture. Run with `cargo test --release -- --ignored --test-threads=1 bench_50k` |

## Tauri commands

Wired from `commands/file_system/git.rs`:

- `get_git_repo_info(path) -> TimedOut<Option<RepoInfo>>` – one-shot lookup, 2 s timeout
- `subscribe_git_state(repo_root) -> Result<RepoInfo, IpcError>` – registers a subscriber, returns current `RepoInfo` synchronously, then emits `git-state-changed` events. 2 s timeout (the synchronous handshake calls `discover_repo` + `repo_info` so a hung repo would otherwise freeze IPC)
- `unsubscribe_git_state(repo_root) -> ()` – drops one subscriber; tears down the watcher when refcount hits zero
- `get_git_status_for_paths(repo_root, dir) -> TimedOut<Vec<EntryStatus>>` – porcelain v2 walk, 5 s timeout
- `set_show_virtual_git_portal(enabled)` (in `commands::settings`) – flips the live portal toggle. Pushed by `settings-applier.ts` whenever `fileExplorer.git.showVirtualGitPortal` changes

## Watcher path set

Per the plan § "Watcher and live invalidation":

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

M3 adds per-worktree `HEAD` watches: at subscribe time we enumerate
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

## Volume hook contract (M2)

The hook order inside `LocalPosixVolume` is fixed and load-bearing:

1. `resolve(path)` runs first (existing). It normalizes absolute vs. relative paths against the volume root.
2. After `resolve`, the volume method calls `git::try_route_*(resolved_path)`. If `Some`, that result is the volume method's return. Otherwise the existing real-FS path runs.

Three hook points:

- `list_directory` → `git::try_route_listing(resolved_path) -> Option<Result<Vec<FileEntry>, VolumeError>>`
- `get_metadata` → `git::try_route_metadata(resolved_path) -> Option<Result<FileEntry, VolumeError>>`
- `open_read_stream` → `git::try_open_blob_stream(resolved_path) -> Option<Result<Box<dyn VolumeReadStream>, VolumeError>>`

All mutation methods (`create_file`, `create_directory`, `delete`, `rename`, `write_from_stream`) detect virtual paths via `git::is_virtual(path)` and return `VolumeError::NotSupported` immediately. `notify_mutation` early-returns for virtual paths since git mutations happen out-of-band; cache invalidation flows through the `.git`-watcher pipeline (`watcher.rs`).

## Honest blob streaming

gix in 0.81 returns whole-blob `Vec<u8>` for `Object::data` – there's no chunked loose-object reader exposed at the public surface yet. So `GitBlobReadStream` owns the full `Vec<u8>` and yields 256 KB chunks for the consumer API shape. **Memory cost equals blob size; chunked yield is for the consumer API, not memory streaming.** We refuse blobs over `tree::MAX_BLOB_BYTES` (256 MB) up-front via `FriendlyGitErrorKind::BlobTooLarge` rather than OOM. Future work: revisit when gix exposes a chunked loose-object reader (track upstream).

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
| `.git/raw/` | real `.git/` mtime | None (real bytes) | real bytes |
| `branches/<name>/` | branch tip committer date | `+12 / -3` vs upstream (or fallback `main`/`master`) | ahead-count |
| `tags/<name>/` | annotated tag date or commit date | short SHA | 0 |
| `commits/<sha>/` | commit committer date | `5 files` (or `1 file`) | files-changed count |
| `stash/<n>/` | stash creation date | `on main` (parsed from stash subject) | 0 |
| `worktrees/<name>` (redirect) | worktree HEAD date | `on feature-x` or short SHA | 0 |
| `submodules/<name>` (redirect) | pinned commit date | short SHA | 0 |
| inside snapshots — files | snapshot commit date | None (blob bytes) | blob bytes |
| inside snapshots — subdirs | snapshot commit date | None (recursive bytes) | recursive blob bytes |

Cross-category Size sort is meaningless (ahead-count vs files-changed vs item count); that's an honest tradeoff — each cell is self-explaining via `display_size_tooltip` (also used as the aria-label).

The frontend reads `display_size` / `display_size_tooltip` from `FileEntry`; the Full mode renderer (`FullList.svelte`) calls `pickSizeDisplay` from `full-list-utils.ts`, and `measure-column-widths.ts` already widens the Size column to fit the override string.

**Decision (M4 follow-up)**: Eager-load ahead/behind for branches; eager-load files-changed for commits
**Why**: Bench (release build, M-series): 100 branches with ahead/behind takes p50=33 ms / p95=36 ms — well under the 300 ms p95 budget the spec sets for the listing pipeline. Files-changed for 200 commits: p50=37 ms / p95=40 ms (200 µs / commit), so the typical Cmdr-sized repo (~3000 commits) lands ~600 ms and the 5000-commit cap lands ~1 s. We accept the worst-case 1 s on the cap because (1) Cmdr's own repo never hits the cap, (2) the listing pipeline runs the hook in `spawn_blocking` so the UI stays responsive, and (3) the alternative — lazy-load via a streamed IPC — would mean another round-trip per row and a placeholder `…` in the cell while it resolves. Document worth re-checking if a user reports the 5000-commit cap feeling slow; the M3 bench harness in `bench.rs` already covers 1000 commits and the new `bench_list_commits_files_changed` covers 200.

## Decisions

**Decision (M4 follow-up)**: Cache `list_status` results keyed by `.git/index` mtime
**Why**: Status used to walk the worktree on every `listing-complete` (every nav,
every diff). On a 50k-file repo that's ~75 ms per nav. We now run one full-repo
walk per index change, store the result in a process-global
`RwLock<HashMap<RepoRoot, CachedStatus>>`, and slice by `dir_in_worktree` on
each call. Cached calls land sub-millisecond on the same fixture (warm p95 in
the bench is bounded by an arbitrary 5 ms ceiling so a busy CI doesn't flake).
The watcher (`watcher.rs::recompute_and_emit`) drops the cache entry on every
`.git/*` mutation it observes, so the next call repopulates. The
`unsubscribe`-on-last-pane path also drops the entry so an unwatched repo
doesn't pin a full-repo-sized snapshot.

**Decision (M4 follow-up)**: Always run with `--untracked-files=normal`, no
"skip untracked outside the worktree root" trick
**Why**: An earlier sketch had us pass `--untracked-files=no` when the caller
scoped to a sub-path inside the worktree, on the theory that listing a deep
subdir doesn't need the full untracked walk. With the cache above, the
untracked walk runs once per index change anyway and the cost is amortized
across every subsequent listing — the extra complexity (two code paths,
mismatched cache keys for the same repo) buys nothing measurable. We always
walk the full worktree with `--untracked-files=normal` and let the cache do
the work.

**Decision (M4)**: Live-toggleable portal via a process-global `AtomicBool`
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

**Decision (M4)**: Carry git-friendly payloads through `VolumeError::IoError`
**Why**: `volume_hooks` return `Result<_, VolumeError>` (the contract is
fixed), but the streaming pipeline calls `friendly_error_from_volume_error`
to compute the `ErrorPane` payload, and that function previously knew
nothing about git. Adding a `Friendly(FriendlyError)` variant to
`VolumeError` would ripple through ~12 call sites. Instead, we serialize
`FriendlyGitError` into the `IoError::message` field with a sentinel
prefix and NUL-separated fields
(`__GIT_FRIENDLY__\0<token>\0<path>\0<title>\0<explanation>`), and have
`friendly_error_from_volume_error` recognize and decode it up-front.
Round-trip tested in `friendly::tests`. The sentinel stays grep-friendly
(`grep "__GIT_FRIENDLY__"` finds every git failure that bubbled to the
user); NUL is the field separator because paths can contain `:` (Windows
drive letters, macOS resource forks, `stash@{0}` specs) and an earlier
`split_once(':')` chain mangled them.

**Decision (M3)**: Shell out to `git stash list` rather than driving gix
**Why**: gix 0.81 doesn't expose a public stash-list API. We could parse
the `refs/stash` reflog by hand, but `git stash list -z --format=%H%x09%gd%x09%s%x09%ct`
gives us git's canonical ordering, the exact `stash@{n}` indices users
see in the terminal, and the commit-time / subject in one shot. The
`git` CLI is already a system requirement (M1's status walk shells out
too). Resolution of `stash@{n}` to a commit ID also goes through
`git rev-parse stash@{n}` for the same reason – gix can't expand the
`stash@{n}` syntax.

**Decision (M3)**: Browse the **W (working-tree) commit** for stash entries
**Why**: `git stash` records the dirty worktree as a merge commit (the
"W" commit in git docs); its first parent ("B") is HEAD at stash time
which is the *clean* tree, not the stashed changes. Browsing W matches
what `git stash show <n>` shows. Verified against fixture: the file
listing under `.git/stash/0/` matches `git stash show 0 --name-only`.

**Decision (M3)**: gix `Repository::worktrees()` for the linked-worktree list
**Why**: gix exposes a `worktrees() -> Vec<worktree::Proxy>` that reads
`<common-dir>/worktrees/*/gitdir` and gives us the working-tree base
path via `proxy.base()`. No shell-out needed. We skip proxies whose
`base()` is missing – orphaned linked worktrees stay invisible rather
than break the listing.

**Decision (M3)**: gix `Repository::submodules()` for submodule listing
**Why**: gix reads `.gitmodules` and yields one `Submodule` per entry
with name + path. We resolve the submodule's working dir as
`<repo_root>/<rel-path>` and set it on `redirect_to_path` so the
frontend opens the working dir directly. The submodule itself is a
git repo so the portal experience cascades for free.

**Decision (M3)**: Streaming log capped at 5000 entries, silent cap
**Why**: Per the plan, hard cap at 5000 keeps even pathological monorepos
inside the listing pipeline's responsive window. Cmdr's own ~3000-commit
history walks in ~7 ms, so the cap is a safety net, not a UX entry point.
When the cap is hit the walk stops silently — no "Load more" affordance
in v1 because tapping it would do nothing useful (pagination IPC isn't
wired). When the first user reports hitting the cap, add the IPC + a
real Load-more entry together so the affordance actually works.

**Decision (M3)**: Volume hook stays single-shot; cancellation via task abort + polled flag
**Why**: The plan called for `ListingEventSink` streaming. M2 already
chose to keep the hook single-shot – the existing `Volume::list_directory`
contract is "compute Vec, return". We honour that here too. Cancellation
works two ways: (1) the listing pipeline's `spawn_blocking` task can be
aborted on cancel, dropping the iterator; (2) we poll a per-process
`AtomicBool` (`log::cancel_flag()`) inside the rev-walk callback every
commit so a *cooperative* cancel takes effect within one commit decode
(microseconds). The flag is opt-in for tests and unused by production
listings (which rely on task abort). Streaming through `ListingEventSink`
is M4 territory if the hook contract is changed at all.

**Decision (M3)**: Per-worktree HEAD watch registration on enumeration
**Why**: notify-debouncer-full doesn't natively glob, so
`<common-dir>/worktrees/*/HEAD` can't be expressed as a single watch. We
enumerate worktree gitdirs via `std::fs::read_dir(<common>/worktrees)`
at subscribe time and register one watch per existing `HEAD`. Worktrees
added later are picked up indirectly: `git worktree add` always touches
the main repo's `HEAD` too, which fires our existing main-HEAD watch
and re-emits `git-state-changed`. The cost is a few extra watcher
entries (typical worktree counts are 1-5) – negligible.

**Decision (M3)**: `Cat::browses_commit_tree()` replaces M2's `is_ref_listing_in_m2`
**Why**: The semantics shift now that commits/ and stash/ also browse a
commit tree (just resolved differently). Branches/tags peel through
refs, commits resolve a SHA prefix, stash expands `stash@{n}`, but the
*tree-walking* code path is identical. The new method name describes
the contract. The dispatch lives in `mod.rs::resolve_commit_for_cat`.

**Decision**: Shell out to `git status --porcelain=v2 -z` for `list_status`
rather than driving `gix::Repository::status()` directly
**Why**: gix's status iterator missed staged additions in our fixture-driven
tests against a single-commit repo (we kept getting "Modified" and
"Untracked" but no "Added" – the tree-vs-index thread didn't fire). The
shell-out matches `git status` semantics 1:1, fits inside the 100 ms budget
even for 50k-file repos, and avoids carrying a partial reimplementation of
porcelain v2 for the few remaining shapes. The `git` binary is part of the
project's system requirements anyway. If gix fixes the iter coverage in a
future release, swap back behind a feature flag.

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
back chip lookups skip the open. We keep eviction simple – the M1 plan
mentioned an idle TTL but it adds a timer thread for nearly no gain.

**Decision**: Watcher uses `notify-debouncer-full` rather than a custom
poll loop
**Why**: The rest of the codebase already depends on `notify` and
`notify-debouncer-full` for filesystem watching (see `file_system/listing/`
and `volume::smb_watcher`). Reusing it gives us 200 ms debounce, OS-level
event coalescing, and a battle-tested teardown path.

**Decision**: `redirectToPath` on `FileEntry` ships in M1, inert
**Why**: Adding it later would ripple a schema change through every
consumer (frontend list views, MCP `cmdr://state`, drag-drop, copy preview,
Brief/Full renderers). Cheap field, lives quietly until M3 sets it on
`worktrees/*` and `submodules/*` entries.

**Decision**: Four `git:*` icon IDs are reserved in M1 but the actual icon
fetching ships with M2's virtual listing
**Why**: M1 doesn't emit `FileEntry`s with `iconId: "git:branch"` yet –
that happens when the virtual portal lists `branches/`. Reserving the
namespace means the frontend's icon-cache code can be written against
known IDs from the start without a churn in M2.

## Gotchas

**Gotcha**: gix's `ThreadSafeRepository::work_dir()` is deprecated but the
new name (`workdir`) only exists on `Repository`, not `ThreadSafeRepository`
**Why**: We hit this when bumping gix to 0.81. We hold an
`Arc<ThreadSafeRepository>` for the cache (it's `Send + Sync`) and call
`work_dir()` on it once. The deprecation is suppressed inline with a
`#[allow]` carrying that exact reason.

**Gotcha**: The status shell-out parses `-z` (NUL-separated) output, not
`\n`-separated lines
**Why**: Filenames can contain newlines. NUL is the only safe separator.
The parser splits on `\0` and consumes a follow-up record for rename/copy
entries (porcelain v2's `2 …` lines have a NUL-separated `<orig>` field).

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

**Gotcha**: Listings on virtual portal paths must skip `start_watching`
**Why**: `listing/streaming.rs` starts a `notify` watcher on the listing's
directory. For virtual paths (`.git/branches/...` etc.) the on-disk path
doesn't exist, so `notify` errors with "No path was found" and the warn
log spams every navigation. The fix: skip the watcher start when
`git::is_virtual(path)`. Cache invalidation for virtual listings flows
through `git::watcher::invalidate_virtual_listings` (via the per-repo
`.git/HEAD`, `refs/`, `packed-refs` watchers), so no notify watch is
needed on the virtual side.
