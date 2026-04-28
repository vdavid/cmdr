# File system › git (complete: M1 + M2 + M3 + M4)

Backend module for the git browser. M1 shipped repo discovery, repo
info, status, the watcher, and the friendly-error skeleton. M2 added
the virtual `.git` portal — `branches/`, `tags/`, `raw/` browsable as
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
| `log.rs` | `list_commits` — gix `rev_walk` over HEAD-reachable commits; cap 5000, batch 200, polled `AtomicBool` cancel flag. `resolve_commit_id` resolves a SHA prefix even for unreachable commits |
| `stash.rs` | `list_stashes`, `resolve_stash_commit` — shells out to `git stash list -z` (gix has no public stash API) |
| `worktrees.rs` | `list_worktrees` — gix `Repository::worktrees()`. Each entry sets `redirect_to_path` to the worktree's working dir |
| `submodules.rs` | `list_submodules` — gix `Repository::submodules()`. Each entry sets `redirect_to_path` to `<repo_root>/<rel-path>` |
| `tree.rs` | `list_tree`, `get_tree_entry`, `lookup_blob_id`, `read_blob` — gix tree walks. Permissions reflect `EntryKind::BlobExecutable` so cross-volume copy preserves the executable bit |
| `read_blob.rs` | `GitBlobReadStream` — owns the full `Vec<u8>` and yields 256 KB chunks. See *Honest blob streaming* below |
| `status.rs` | `list_status(repo, dir)` shells out to `git status --porcelain=v2 -z`. Parses the output into a `Vec<EntryStatus>` |
| `watcher.rs` | `GitWatcherRegistry` — per-repo notify-rs debouncer. `subscribe(app, root)` returns the current `RepoInfo` synchronously and emits `git-state-changed` on relevant `.git/*` mutations. 200 ms debounce. M2: also calls `notify_directory_changed(.., FullRefresh)` for any cached `.git/{branches,tags}/` listings on the local volume |
| `friendly.rs` | `FriendlyGitError`, `FriendlyGitErrorKind` — ten variants (M1's six, `BlobTooLarge` from M2, plus M4's `ShallowBoundary`, `MissingObject`, `GitDirPermissionDenied`). Active-voice copy, no "error" / "failed". `to_friendly_error()` builds a `volume::FriendlyError` for `ErrorPane`; `encode_for_volume_error()` + `try_decode_git_friendly()` carry the structured payload through `VolumeError::IoError` so the streaming pipeline rebuilds it on the way out |
| `tests.rs` | M1 tests: discover, repo_info, status, friendly errors |
| `m2_tests.rs` | M2 tests: classify, list_branches/tags/root, list_tree, blob-read parity with `git show`, cross-volume copy round-trip |
| `m3_tests.rs` | M3 tests: list_commits + sha browsing + cancellation + 1000-commit walk (`#[ignore]`), list_stashes, list_worktrees + redirect, list_submodules + redirect, watcher invalidation for `commits/` |
| `bench.rs` | `#[ignore]` benchmark over a 50k-file synth fixture. Run with `cargo test --release -- --ignored --test-threads=1 bench_50k` |

## Tauri commands

Wired from `commands/file_system/git.rs`:

- `get_git_repo_info(path) -> TimedOut<Option<RepoInfo>>` — one-shot lookup, 2 s timeout
- `subscribe_git_state(repo_root) -> RepoInfo` — registers a subscriber, returns current `RepoInfo` synchronously, then emits `git-state-changed` events
- `unsubscribe_git_state(repo_root) -> ()` — drops one subscriber; tears down the watcher when refcount hits zero
- `get_git_status_for_paths(repo_root, dir) -> TimedOut<Vec<EntryStatus>>` — porcelain v2 walk, 5 s timeout
- `set_show_virtual_git_portal(enabled)` (in `commands::settings`) — flips the live portal toggle. Pushed by `settings-applier.ts` whenever `fileExplorer.git.showVirtualGitPortal` changes

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
over the aspirational 50 ms target — `is_dirty` does a full worktree walk,
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

gix in 0.81 returns whole-blob `Vec<u8>` for `Object::data` — there's no chunked loose-object reader exposed at the public surface yet. So `GitBlobReadStream` owns the full `Vec<u8>` and yields 256 KB chunks for the consumer API shape. **Memory cost equals blob size; chunked yield is for the consumer API, not memory streaming.** We refuse blobs over `tree::MAX_BLOB_BYTES` (256 MB) up-front via `FriendlyGitErrorKind::BlobTooLarge` rather than OOM. Future work: revisit when gix exposes a chunked loose-object reader (track upstream).

## Ref-name flat rendering

Branches like `feature/foo` show as a single entry called `feature/foo`, not nested `feature/` then `foo`. The classifier (`path::classify`) greedy-matches ref names against the repo's known refs (longest-first) before treating any remainder as a tree sub-path. The inverse (`to_path`) splits ref names on `/` so OS-native separators are used in the on-disk representation. This is the only place where the URL → path round-trip needs the repo open.

## Decisions

**Decision (M4)**: Live-toggleable portal via a process-global `AtomicBool`
**Why**: `try_route_listing` / `try_route_metadata` / `try_open_blob_stream`
each early-return `None` when the toggle is off, falling through to the
real-FS path. This keeps the toggle a no-op cost (one atomic load per
hook call) and makes "show me the raw `.git`" instant — no listing
cache invalidation, no IPC dance. The setter is wired live from the
frontend (`set_show_virtual_git_portal`) and seeded at startup from
`Settings::show_virtual_git_portal`. Mutation guards (`is_virtual` in
`local_posix`) intentionally don't consult the toggle: even with the
portal off we don't want Cmdr to write to `.git/HEAD` from a copy
dialog. Power users who really want to mutate `.git` use a terminal.

**Decision (M4)**: Carry git-friendly payloads through `VolumeError::IoError`
**Why**: `volume_hooks` return `Result<_, VolumeError>` (the contract is
fixed), but the streaming pipeline calls `friendly_error_from_volume_error`
to compute the `ErrorPane` payload — and that function previously knew
nothing about git. Adding a `Friendly(FriendlyError)` variant to
`VolumeError` would ripple through ~12 call sites. Instead, we serialize
`FriendlyGitError` into the `IoError::message` field with a sentinel
prefix (`__GIT_FRIENDLY__:<token>:<path>:<title>: <explanation>`) and
have `friendly_error_from_volume_error` recognize and decode it
up-front. Round-trip tested in `friendly::tests`. The encoded form
also reads naturally in logs (`grep "__GIT_FRIENDLY__"` finds every
git failure that bubbled to the user).

**Decision (M3)**: Shell out to `git stash list` rather than driving gix
**Why**: gix 0.81 doesn't expose a public stash-list API. We could parse
the `refs/stash` reflog by hand, but `git stash list -z --format=%H%x09%gd%x09%s%x09%ct`
gives us git's canonical ordering, the exact `stash@{n}` indices users
see in the terminal, and the commit-time / subject in one shot. The
`git` CLI is already a system requirement (M1's status walk shells out
too). Resolution of `stash@{n}` to a commit ID also goes through
`git rev-parse stash@{n}` for the same reason — gix can't expand the
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
`base()` is missing — orphaned linked worktrees stay invisible rather
than break the listing.

**Decision (M3)**: gix `Repository::submodules()` for submodule listing
**Why**: gix reads `.gitmodules` and yields one `Submodule` per entry
with name + path. We resolve the submodule's working dir as
`<repo_root>/<rel-path>` and set it on `redirect_to_path` so the
frontend opens the working dir directly. The submodule itself is a
git repo so the portal experience cascades for free.

**Decision (M3)**: Streaming log capped at 5000 entries with a "Load more" sentinel
**Why**: Per the plan, hard cap at 5000 keeps even pathological monorepos
inside the listing pipeline's responsive window. Cmdr's own ~3000-commit
history walks in ~7 ms, so the cap is a safety net, not a UX entry
point. When the cap is hit, we append a synthetic entry whose
`redirect_to_path` is `cmdr-git://load-more/<after-sha>`. The frontend
intercepts the magic prefix and treats Enter as "fetch the next page"
rather than "navigate to that path". M3 ships the marker; the
pagination IPC isn't wired yet because Cmdr's own and almost every
typical repo never hit the cap. Wiring is a one-method follow-up
when the first user reports the cap.

**Decision (M3)**: Volume hook stays single-shot; cancellation via task abort + polled flag
**Why**: The plan called for `ListingEventSink` streaming. M2 already
chose to keep the hook single-shot — the existing `Volume::list_directory`
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
entries (typical worktree counts are 1-5) — negligible.

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
"Untracked" but no "Added" — the tree-vs-index thread didn't fire). The
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

**Decision**: `RepoCache` is process-global, evicted only on the last
unsubscribe (no idle TTL)
**Why**: Re-opening a `gix::Repository` is cheap (~10 ms on warm caches)
but not free; the cache pins one handle per active subscriber so back-to-
back chip lookups skip the open. We keep eviction simple — the M1 plan
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
**Why**: M1 doesn't emit `FileEntry`s with `iconId: "git:branch"` yet —
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
