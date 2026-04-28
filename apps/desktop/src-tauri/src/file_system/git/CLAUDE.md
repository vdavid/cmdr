# File system › git (M1 foundation)

Backend module for the git browser. M1 ships repo discovery, repo info,
status, the watcher, and friendly errors. M2 will add the virtual `.git`
portal; M3 fills in commits, stash, worktrees, submodules.

## File map

| File | Role |
|---|---|
| `mod.rs` | Public API re-exports (`discover_repo`, `repo_info`, `list_status`, watcher registry, friendly errors) |
| `repo.rs` | `discover_repo(path)` walking up via `gix::discover` (follows gitlinks). `repo_info(handle, root)` collects branch, detached SHA, unborn flag, upstream, ahead/behind, and `is_dirty`. Process-global `RepoCache` (`Arc<RwLock<HashMap>>`) keyed by canonical worktree root |
| `status.rs` | `list_status(repo, dir)` shells out to `git status --porcelain=v2 -z`. Parses the output into a `Vec<EntryStatus>` |
| `watcher.rs` | `GitWatcherRegistry` — per-repo notify-rs debouncer. `subscribe(app, root)` returns the current `RepoInfo` synchronously and emits `git-state-changed` on relevant `.git/*` mutations. 200 ms debounce |
| `friendly.rs` | `FriendlyGitError`, `FriendlyGitErrorKind` — six variants (`NotARepo`, `OrphanedWorktree`, `CorruptRepo`, `IndexLocked`, `PermissionDenied`, `BareRepo`). Active-voice copy, no "error" / "failed" |
| `tests.rs` | Unit + integration tests for discover, repo_info, status, friendly errors |
| `bench.rs` | `#[ignore]` benchmark over a 50k-file synth fixture. Run with `cargo test --release -- --ignored --test-threads=1 bench_50k` |

## Tauri commands

Wired from `commands/file_system/git.rs`:

- `get_git_repo_info(path) -> TimedOut<Option<RepoInfo>>` — one-shot lookup, 2 s timeout
- `subscribe_git_state(repo_root) -> RepoInfo` — registers a subscriber, returns current `RepoInfo` synchronously, then emits `git-state-changed` events
- `unsubscribe_git_state(repo_root) -> ()` — drops one subscriber; tears down the watcher when refcount hits zero
- `get_git_status_for_paths(repo_root, dir) -> TimedOut<Vec<EntryStatus>>` — porcelain v2 walk, 5 s timeout

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

## Decisions

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
