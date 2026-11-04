# `cmdr-git` details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the
must-knows; this is the depth. What the APP does with this crate — the route, the `.git/` listing overlay, the toggle,
and the `git-state-changed` event — is `apps/desktop/src-tauri/src/file_system/git/DETAILS.md`, and this document
doesn't restate it.

## Where the boundary runs, and why

A git browser has two faces, and they split along what a REPOSITORY can answer versus what an APP decides.

**What a repository can answer** is everything here: which branch HEAD is on, how far it is from upstream, what changed
in the worktree, what a commit's tree holds, and what bytes a blob is. None of it needs a window, a setting, or a pane,
so all of it verifies with `cargo check -p cmdr-git --all-targets` and no app in the graph.

**What an app decides** stays out:

- **Whether the portal is on at all** — a user setting, read by the route and by the overlay. This crate has no opinion.
- **Which paths route here** (`file_system/volume/manager/git_routing.rs`) — the app owns its `VolumeManager`, so it
  asks [`portal_route`](src/path.rs) the lexical question and does the routing itself.
- **The `.git/` landing listing** (`file_system/git/overlay.rs`) — the six category rows reach a PANE and nothing else,
  which is a property of the app's listing pipeline. This crate only builds the rows, through
  `GitPortal::category_rows`.
- **Every word and every event** (`file_system/git/wiring.rs`) — the `tauri_specta` payload the chip subscribes to, and
  the listing re-reads a repo change drives. The watcher here reports a typed snapshot through `GitStateSink` and knows
  nothing about either.

The one thing that could have gone either way is `volume_holds_real_repos`, the "can `gix` open this volume's paths?"
predicate. It's app-side, because it reads a `Volume` capability (`local_path().is_some()`), both of its callers are the
app's two seams, and nothing in this crate ever asks it.

## The public surface is capped

`index-crate-isolation` holds this crate to **11 root promises, 0 public modules, and 0 public items inside them**, set
on 2026-09-05 to exactly what it exposes — no headroom, so the first addition has to be argued for.

**Every module is private**, which is the tightest shape a backend crate here has taken: a host can name no path into
this crate, so all 11 names arrive as root re-exports and both other buckets are zero. That also means `GitPortal`'s own
methods are reachable but unmeasured, so what holds them is the list below rather than a number.

**A backend's API is the `Volume` trait it implements**, which is `cmdr-fs`'s promise rather than this crate's, so
`GitPortalVolume`'s trait methods aren't counted. What IS counted serves one of four callers, and a new item should name
which:

- **Browsing** (`file_system/volume/manager/git_routing.rs`, `roots.rs`, `file_system/git/overlay.rs`): `GitPortal` (the
  value the app parks, and everything a browse goes through), `GitPortalVolume` (a type the registry downcasts to, to
  tell a routed volume from a mount), and `portal_route` (the lexical question `resolve` asks).
- **The breadcrumb chip** (`commands/file_system/git.rs`): `RepoInfo`, which crosses IPC, `repo_info`, and `RepoHandle`,
  which is what `GitPortal::discover` answers with.
- **The status column** (the same commands file): `EntryStatus` and `EntryStatusCode`, both crossing IPC, and
  `list_status`.
- **Reporting a change** (`file_system/git/wiring.rs`): the `GitStateSink` trait the app implements, and
  `no_git_state_sink` for a session with no window.
- **Filling in commit counts** (`apps/desktop/src-tauri/src/file_system/git/commit_counts.rs`):
  `GitPortal::commit_rows_with_counts` and `GitPortal::COUNT_ALL_LIMIT`, a method and an associated const on the
  browsing type rather than new root exports (approved 2026-10-14), so the count stays 11 while the list names them.
- **Letting a drive eject** (`apps/desktop/src-tauri/src/volumes/unmount_approver/mod.rs`): `GitPortal::release_volume`,
  one method on the browsing type for the same reason (added 2026-10-19 for GitHub #488, pending David's say-so; §
  "Letting go of a volume that's about to unmount").

**A `pub` inside a private module isn't measured either**, which is exactly why one may not sit there waiting for a
caller: nothing would ever notice. So a helper here earns its place from a caller that exists today, ❌ never from a
plausible one — `to_path`, `looks_like_sha_prefix`, and `dir_path_from_subpath` were each written for a future IPC
consumer, reached only their own unit tests, and were deleted rather than carried. The one test-only helper that
survived (`snapshot_dates::clear_cache`, which `bench.rs` needs to measure a cold walk) is `cfg(test)` and `pub(crate)`,
so no configuration compiles it without its caller. The count went 12 → 11 the same way: `virtual_category_prefixes`
lost its only caller when the post-change refresh started matching a listing by its canonical worktree root instead of
by string prefix, so it left with the caller rather than waiting for another.

**A `testing`-gated method on `GitPortal` spends nothing here**, which is why the scripted watcher arrived without a
conversation: `with_scripted_watcher` and `fire_watcher` are methods on a type in a private module (unmeasured, like
every other `GitPortal` method), and the `GitWatcherBackend` trait plus both backends are `pub(crate)`. The count stands
at 11 / 0 / 0 with them in place. ❗ That is not a loophole to route a real API through: an item a HOST calls in
production is a root promise whatever it's attached to.

Two gated items sit outside those numbers: `RecordingGitStateSink` and the whole `test_fixtures` module, both behind
`testing`. The app's routing, overlay, and toggle suites build their repositories with those fixtures, so there is one
set rather than two that drift; ❌ keep it a fixture surface by reading § "Which side a test lives on", not by watching
a number.

## Which side a test lives on

**A cell goes where its ASSERTION lives, never where its fixture does.** What a repository answers is here: the
categories, the column metadata, the snapshot dates, the classifier, the `Volume` contract, and the status cache. What
the APP does with those answers stays app-side beside the code it exercises: the route, the listing overlay, the toggle,
the `git-state-changed` payload, and the walker-exposure regressions.

Which means, file by file: a cell that needs no repository is an inline `mod tests` in the module it asserts on
(`path.rs`, `log.rs`, `stash.rs`, `read_blob.rs`, `state_sink.rs`, `status.rs`'s two), and one that needs a real
repository is a sibling `<subject>_tests.rs` — `repo_tests` (discovery and `RepoInfo`), `status_tests` (the worktree
walk), `category_tests` (all six categories, plus the row set the landing page is built from), `tree_tests` (a
snapshot's tree and its blobs), `column_meta_tests`, `snapshot_dates_tests`, `watcher_tests` (the registry's
bookkeeping: one watch per repo, the refcount, and what a change reports), and `volume_tests` (everything asserting
`GitPortalVolume`, conformance included). The one exception is `path.rs`'s
`classify_names_every_shape_against_a_real_repo`: greedy ref matching reads the repo's known refs, so it needs a fixture
yet belongs beside the parser it asserts on.

The instrument for the app half is the `testing` feature: `test_fixtures` builds the repository, `RecordingGitStateSink`
makes a watcher report observable without a window, and `GitPortal::with_scripted_watcher` plus `fire_watcher` make one
observable without FSEvents. **That recorder is what a subscription cell asserts through.** The DEBOUNCE cell lives
app-side and takes the real backend: it drives the parked portal's `subscribe_state`, writes five commits and a branch
switch back to back, waits until the LAST report carries the post-burst branch and the reports go quiet, and then does
it all a second time to prove the watch outlived git's rename over `HEAD` (§ "Watcher path set"). ❌ It never asserts a
count: how many batches a real burst arrives in depends on how long its writes take against the window, and asserting
"exactly one" made it the most-retried Rust cell (78 runs, 2026-09-06 to 2026-09-11). Delivery is this crate's contract,
but the path that proves it starts where the portal is parked, so the cell belongs at that end — and it is the only one
anywhere that arms a real watcher (§ "The watcher splits into bookkeeping and a backend"). The coalescing that makes a
split burst one report is asserted crate-side on the scripted backend (`watcher_tests`), where a "second batch" is a
second `fire_watcher` and costs no FSEvents.

## One burst is one report

`NotifyWatcherBackend` calls `on_change` once per batch `notify_debouncer_full` EMITS, and it emits on a tick cadence
(with no tick rate given it takes a quarter of the timeout, so 50 ms here). One burst whose per-path deadlines straddle
a tick boundary therefore arrives as two batches, both recomputing the same post-burst snapshot. Measured on an M1 Max
(2026-09-06, 20 runs of the DEBOUNCE cell): 13 ms of writes gave one batch about two-thirds of the time and two, 58–66
ms apart, the rest. Each report costs a `git-state-changed` event and a `FullRefresh` of every open portal and `.git/`
pane, so the second is pure waste.

**`recompute_and_report` drops it.** Each live watch carries the `(Instant, RepoInfo)` of its last report, created empty
by `GitWatcherRegistry::arm` and dropped with the last unsubscribe, so the first report after arming is unconditional. A
recompute whose snapshot equals that one AND lands less than `DEBOUNCE` after it goes nowhere.

**Why the window is not a heuristic.** An emission is never sooner than `DEBOUNCE` after the write behind it:
`DebounceDataInner::debounced_events` expires an event only once `now - event.time >= timeout` (read in
notify-debouncer-full 0.7.0, 2026-09-06), and notify's own delivery latency only adds to that. So a report landing less
than `DEBOUNCE` after the previous one can only be about writes that preceded the previous report's own recompute, which
means the snapshot it carried already reflects them. An identical snapshot inside the window therefore has provably no
news in it, and one outside it may well have.

❌ **Never widen this to "skip any repeat, whatever the gap".** `RepoInfo` is what the breadcrumb chip shows, not a
fingerprint of the repository: a second `git branch`, a commit in a worktree that stays dirty, and a `git add` in an
already-dirty repo each leave every field byte-identical. Those still have to reach the `.git/branches/` and
`.git/commits/` panes and the status column, which re-read on the report rather than on its payload. Swallowing one is
the same class of bug as the symlink mismatch that left a `branches/` pane stale (2026-09-05), and
`watcher_tests::the_same_state_after_the_window_is_news_again` is the cell that holds the line.

## Linked worktrees

`git worktree add` writes `.git` as a FILE holding `gitdir: <common>/worktrees/<name>`.

**The portal lives where `.git` is a DIRECTORY.** The overlay contributes to a directory listing, and listing a gitlink
fails `ENOTDIR`, so a linked worktree has no `.git/` landing page. What it does have is everything below:
`path::classify_in` splits on the path SEGMENT and never stats, `portal_route` is the same pure string check, and `gix`
discovery follows the gitlink, so `<linked>/.git/branches` and deeper answer exactly as in the main worktree (verified
2026-09-05, `a_linked_worktree_serves_the_categories_but_has_no_dot_git_landing`).

**Why that's the right trade.** The landing listing used to rewrite the real gitdir's entries under `<linked>/.git/…`,
where they were visible but not openable (`<linked>/.git/HEAD` is a path THROUGH a file, so reading it is `ENOTDIR`).
Losing rows nobody could open costs less than a `stat` on the route, which runs on every path-bearing call.

## The watcher splits into bookkeeping and a backend

`watcher.rs` is two halves, and the split is what keeps a subscription cell affordable.

- **`GitWatcherRegistry` does the bookkeeping**: one watch per canonical repository root however many subscribers it
  has, refcounted, and the last unsubscribe tears the watch down, evicts the repo handle, and drops the status cache.
- **`GitWatcherBackend` is what talks to the operating system.** `NotifyWatcherBackend` builds the 200 ms debouncer and
  arms the one watch below. `ScriptedWatcherBackend` (behind `testing`) arms nothing: it remembers which repositories
  have a watch and runs the change callback when a test calls `GitPortal::fire_watcher`.

**Why a seam rather than a real watcher everywhere.** Arming a real FSEvents stream on a repository's gitdir is nearly
the whole cost of `subscribe_state`, and no cell that asserts bookkeeping cares about it. On an idle M-series machine
the two app-side subscription cells ran 0.35 s and 0.58 s; on the scripted backend the bookkeeping half runs in 0.05 s,
and under a saturated `cargo nextest run --workspace` the difference is what decided whether they met the suite's 8 s
cap at all (measured 2026-09-05).

**Two cells in the repo take the real backend**, and each one is about something only an operating system does.

- `file_system::git::wiring_tests::a_burst_reports_its_end_state_and_the_watch_survives_for_the_next_one`: the debounce
  is `notify`'s own, and so is the watch surviving git's rename over `HEAD`, so a fake standing in for either would
  assert the fake's arithmetic. Each burst is REDONE on a fresh branch until it's delivered, because a saturated
  `fseventsd` drops events silently and the in-process burst takes milliseconds, so a lost one is lost whole; a new act
  there goes through the same `burst_until_settled`.
- `watcher_tests::a_deleted_repository_stops_reporting_and_still_gives_its_hold_back`: a scripted backend has no watches
  to LOSE, so only a real one can say what a repository's removal does to them.

❌ Don't add a third for a property either of those already arms a watcher for: a new burst behavior belongs as another
act inside the first, which is where its second burst came from, and a new teardown behavior inside the second.

**Neither door costs public surface.** `GitPortal::with_scripted_watcher` and `GitPortal::fire_watcher` are methods on a
type in a private module, so `index-crate-isolation` doesn't measure them, and both are `testing`-gated so a shipped
build carries neither. The trait and both backends are `pub(crate)`: nothing outside this crate names them.

**Two ways in, because two callers want different things.** `GitPortal::subscribe_state` arms the watcher AND reads the
`RepoInfo` the breadcrumb chip's handshake needs. `GitPortal::watch_repo` arms it and answers only the canonical root to
release by: that's what a host arming on behalf of an open listing asks for, and it skips the `is_dirty` walk over the
worktree, which is the expensive half. ❗ A failed handshake still releases what it armed, or a caller handed an error
would never unsubscribe.

## A repository that is deleted under its own watch

Deleting a repo folder is an ordinary thing to do in a file manager, and the pane that was just looking at it is what
armed the watch. On Linux the removal arrives as `Remove` events on every watched directory plus an `IN_IGNORED` per
dying watch; on macOS as FSEvents for the same paths. Either way it is a WRITE by any filter, so the recompute runs.

What happens then is the whole behavior: `recompute_and_report` opens the repository, `RepoCache::discover` fails
because there is nothing there, and it returns without touching the sink. So a repository that is gone raises no
`git-state-changed` event and drives no `FullRefresh` of a listing that is equally gone.

❗ The registry hold SURVIVES the deletion, on purpose. The subscriber has not left, and only its own
`unsubscribe_state` may free the slot; freeing it here would drop a hold somebody still owes back and unbalance the
refcount for the next repository at that path. The `notify` debouncer is dropped with the subscription as always, and
dropping a watch whose inode is gone is fine.
`watcher_tests::a_deleted_repository_stops_reporting_and_still_gives_its_hold_back` pins all of it.

## Watcher path set

ONE watch: the gitdir, recursively, through a `NoCache` debouncer (the listing watcher's shape).

❗ **One path, ❌ never one per state directory.** On macOS `notify`'s `FsEventWatcher::watch` stops the FSEvents
stream, joins its run-loop thread, and starts a new one for EVERY added path (read in notify 8.2.0, `fsevent.rs`
`watch_inner`, 2026-10-05), and `notify_debouncer_full::Debouncer` exposes no batched `paths_mut`. The four directory
watches this replaced cost four restarts before the chip's handshake could run, which is what made the chip late on
first entry and made the real-watcher cell the most-retried in the Rust lane. FSEvents watches a path's whole subtree in
the kernel either way (`notify` emulates `NonRecursive` by filtering), so on macOS the recursive gitdir delivers nothing
the old set didn't.

❗ **`NoCache`, ❌ never the default `FileIdMap`.** The map pairs a rename's two halves by file id, and pays for it by
walking and `stat`ing the whole watched tree at arm time, which for a recursive gitdir is every loose object.
`is_repo_state_path` reads paths only, so the pairing buys nothing.

❗ **A linked worktree watches the COMMON gitdir.** Its `.git` is a FILE (gitlink) pointing at its own gitdir
(`<common>/worktrees/<name>/`), which holds only `HEAD`, `index`, `logs/HEAD`, and the merge/fetch heads; every branch,
tag, and remote-tracking ref lives in the common dir (`commondir` file, verified with `git rev-parse --git-path` on git
2.x, 2026-10-06). A watch on the own gitdir alone missed `git fetch` / `git push` (stale ahead/behind) and branch
create/delete. `watcher::WatchScope` puts the one recursive watch on the common dir (which contains the own one) and
filters to the own gitdir's state files plus the shared `refs/`, `packed-refs`, and `config`; the common dir's top-level
`HEAD` / `index` (the main worktree's) and `worktrees/<sibling>/` are dropped. Two linked worktrees of one repo hold two
watches on that dir rather than sharing one: each reports its own root through its own filter, and sharing would buy a
second refcount layer and a per-worktree fan-out to save a kernel subscription. Pinned by
`watcher_tests::a_linked_worktree_hears_the_shared_refs_and_not_its_siblings`.

❗ **A directory, ❌ never the state files themselves.** git never writes `HEAD` or `index` in place: it writes
`HEAD.lock` and renames it over the top. inotify watches an INODE, so a watch on the file dies at the first rename and
every later write in the same burst is lost with no error anywhere. macOS FSEvents is path-based and tolerated file
watches, so this only ever failed on Linux, where the real-watcher cell timed out having received nothing after the
first commit (CI, 2026-09-06). A directory's inode is what the rename modifies, so it survives.

**What the recursive watch costs, and what pays it back.** It also delivers `objects/`, `hooks/`, `COMMIT_EDITMSG`,
`MERGE_MSG`, `*.lock`, and the per-ref reflogs, none of which moves a pane. `watcher::is_repo_state_path` is the
allowlist that drops them before anything opens the repository: a path counts when it is one of the seven state files
(`HEAD`, `ORIG_HEAD`, `MERGE_HEAD`, `FETCH_HEAD`, `packed-refs`, `index`, and `config`, which names each branch's
upstream) or `logs/HEAD`, or sits under `refs/` or `worktrees/`, and never when it ends in `.lock`. Dropping the lock
half of git's write dance costs no report, because the rename's TARGET (`HEAD`) rides in the same event and answers
`true`. `watcher_tests::only_the_paths_a_snapshot_reads_are_worth_a_recompute` pins the whole table. On Linux the
recursive watch is one inotify watch per gitdir DIRECTORY (`objects/` fans out to at most 256, plus any `modules/` or
`lfs/` trees), the price of one code path for both platforms.

❗ **A READ is dropped by KIND, and that is what keeps the watcher off its own tail.** Linux inotify asks for `IN_OPEN`
(`notify` 8.2 sets it in `add_single_watch`), so every file a recompute OPENS — `HEAD`, `index`, `packed-refs`, a
`refs/` directory — comes back as `Access(Open)` on a directory we watch, and the path allowlist accepts every one of
them. Each report then became the trigger for the next: the debounce cell reported the same snapshot 48 times in 10 s
and timed out, at one report per debounce window, with the test making no git calls at all after the first burst (CI run
34001957865, `ubuntu-latest`, 2026-09-06). macOS stayed green throughout because FSEvents reports no reads.
`watcher::is_repo_state_change` gates on kind BEFORE path and drops `Access(Open)`, `Access(Read)`, and
`Access(Close(Read))`; a real write always arrives as `Modify`, `Create`, `Remove`, a rename, or `Access(Close(Write))`,
so nothing is lost with them. `watcher_tests::the_watchers_own_reads_are_not_changes` pins both halves.

❌ **Don't reach for the report coalescing to close a loop like that.** A feedback loop runs at one cycle per debounce
window plus the recompute, so its reports land just OUTSIDE the window the coalescing covers and it never sees them.
Widening the window to catch one would break the guarantee the § above rests on.

The other half of the invariant is that the reader really is one: `repo_info` and the status walk open the repository
through `gix`, which writes nothing back, index stat-cache refresh included (verified on gix 0.87 by
`watcher_tests::reading_a_repository_writes_nothing_into_the_gitdir`, 2026-09-06). A reader that DID write would arm the
same loop, and no event filter could close that one, because a write really is a change.

**A real-watcher cell that fails prints what the OS delivered.** `watcher::trace_delivery` writes one stderr line per
debounced batch (kinds, paths, and whether a recompute followed) in `test` and `testing` builds, and nothing at all in a
shipped one. `nextest` captures it and prints it only for a failing cell, which is what turns "48 identical reports"
into an answer: a read loop, a dead watch, and an error storm all look the same from the sink.

**A debouncer error recomputes and says so.** `NotifyWatcherBackend` used to swallow `DebounceEventResult::Err`, so a
degraded backend went quiet with no diagnostic. It now logs at `warn` with the repo root and the errors, and calls
`on_change` anyway: the usual cause is a dropped-event queue, so what was missed is unknown, and the report coalescing
drops the recompute again if nothing actually moved. That one call site is the whole reason this crate depends on `log`
at all; `Cargo.toml` says why it stops there.

## Performance

Bench result on the 50k-file synth repo, release build (`cargo test --release -p cmdr-git -- --ignored`), measured on an
M1 Max, gix 0.87, 2026-09-05, median of three runs:

| Metric                     | Budget          | Measured |
| -------------------------- | --------------- | -------- |
| `discover + repo_info` p50 | 50 ms target    | ~54 ms   |
| `discover + repo_info` p95 | 100 ms hard cap | ~58 ms   |
| `list_status` cold p50     | 100 ms          | ~60 ms   |
| `list_status` cold p95     | 100 ms          | ~68 ms   |
| `list_status` warm p50     | –               | ~16 µs   |

Everything is inside budget, and a warm `list_status` is a cache hit in microseconds. Subsequent repo discovery calls
hit the portal's repo-handle cache and run in microseconds too.

**Take every reading with the bench holding `measure_alone()`, or it measures the wrong thing.** Each bench here walks a
big tree, `cargo test` runs them on several threads by default, and two walks over the same fixture inflate each other:
`discover + repo_info` reads p50 92 ms / p95 114 ms beside `list_status` and p50 54 ms / p95 58 ms alone, which is the
difference between failing the hard cap and passing it with 40% to spare. `list_status` cold p95 climbs to 97 ms under
the same contention, a hair off its own budget. The lock is in `bench.rs`; every bench takes it first.

`repo_info`'s `is_dirty()` — a full worktree walk — is nearly all of the `discover + repo_info` number; discovery itself
is a cache hit after the first call. The 50 ms target is aspirational rather than reachable:
`git status --untracked-files=no --porcelain` walks the same fixture in 50 ms on this machine (five runs, 2026-09-05),
so gix has no headroom to give against the tool it replaces, and the 100 ms hard cap is the bound that means something.
If the chip ever does need to render faster than a worktree walk, the lever is to split the fast question (repo + head +
ahead/behind) from the slow one (dirty) and deliver the dirty flag as a second update over `git-state-changed`. Not
needed at these numbers.

The bench is `#[ignore]`d — it builds a 50k-file fixture — so no check lane runs it; take a reading with the command
above after any change to the chip pipeline.

## The portal is a routed volume

`GitPortalVolume` (`volume.rs`) is a read-only `Volume` over one repo's virtual trees, the same shape `ArchiveVolume`
has. The host's `VolumeManager::resolve` routes any path with a `.git/<category>/` segment to it
(`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Resolving a path: the three routes") and hands it the
input path verbatim; the volume maps that path to `(repo, category, ref, tree path)` with `path::classify_in` and
answers from the same `virtual_listing` / `log` / `tree` code `GitPortal::category_rows` calls for the host's `.git/`
listing.

- **Its namespace is the six categories and what's under them, nothing else.** Listing its root (`<worktree>/.git`)
  answers the six category rows ALONE, via `virtual_listing::list_categories`. Real `.git/*` entries are the parent
  volume's, which is what keeps `.git/config` editable and lets a repo-folder delete walk `.git/` as an ordinary
  directory. A pane sees the two halves together because the host's listing pipeline folds the category rows into the
  local volume's read.
- **A `.git` that isn't a repository is `NotFound`, decided here.** Routing is lexical and does no I/O, so "is there
  actually a repo at this path?" is answered on first use, through the portal's `RepoCache`. ❌ Don't add a `stat` to
  the route to pre-empt it.
- **What the trait answers**: `is_writable` false and every mutation on the trait default (`create_directory_all` is
  overridden, since its default would claim success for a directory that exists); `supports_export` and
  `supports_streaming` true, with `open_read_stream` handing back a `GitBlobReadStream`; `can_watch_listings` false and
  `listing_watch_coverage` `None`, because the paths aren't on disk; `supports_local_fs_access` false and `local_path`
  `None` for the same reason; `routes_over_a_parent` TRUE, which is what keeps the host from mistaking `<worktree>/.git`
  for a mount and stealing every path under it; `lane_key`, `get_space_info`, and `get_space_info_at` delegate to the
  PARENT volume, since the objects live on its disk (the last is asked at `<worktree>/.git`, so a repo on a phone's SD
  card answers for the card). `scan_for_copy` and the batch scan come from `cmdr_fs::volume::scan_walk` through a
  two-method `ScanSource`, which is what lets a whole branch tree be copied out to another volume.
- **The host reads a snapshot through the trait and nothing else.** A copy out of `.git/branches/<name>/` walks with
  `scan_for_copy` and streams with `open_read_stream`; the viewer and the agent's `inspect_file` stream one blob to a
  bounded temp and open that. All three ask the host's routing whether a path is served here, ❌ never this crate
  directly, so an archive and a snapshot travel the same seam. The app-side halves are
  `crates/cmdr-operations/DETAILS.md` § "Routing a transfer" and `apps/desktop/src-tauri/src/file_viewer/DETAILS.md` §
  "Preview of a routed file".
- **A MOVE out of a snapshot is refused by the host, not here.** Every mutation stays at the trait default, so this
  volume simply has no delete; the app refuses the move before it copies anything, rather than copying and then failing
  the half it can never do.
- **Every `gix` call runs on `VolumeHost::runtime().spawn_blocking`**, ❌ never on the caller's async worker: a listing
  of a big repo is a blocking walk.

### `GitPortal`: the value that owns the mutable state

`GitPortal` (`portal.rs`) holds the `RepoCache`, the per-repo watcher registry, the `GitStateSink` that registry reports
through, and the `VolumeHost`, and it mints one `GitPortalVolume` per repo. Every one of those is a VALUE the portal
owns, ❌ never a static of its own. `wiring::portal()` is only where the APP parks its instance, so an IPC command, the
listing overlay, and a watcher subscription all share one set of open repositories rather than opening each repo three
times; a volume always holds the `Arc<GitPortal>` that minted it, and a test builds its own with a detached sink.

**Decision: what may stay a static here, and what may not.** Two memos do: `snapshot_dates`'s per-directory date cache
is keyed by `(ObjectId, dir path)`, and `status`'s snapshot is keyed by `(canonical root, .git/index mtime)`. Both are
content- or mtime-addressed, so a second portal sharing one can only get an answer that was correct for that key, and
`list_status` stays callable with a repo handle alone — no portal, no host. The `RepoCache` is the opposite: it owns a
RESOURCE with a lifecycle (an open `gix` repository, evicted when the last subscriber leaves), so a static one would
mean a test's evictions reaching the app's handles and back. Two more statics are no memo of an answer at all:
`repo::is_on_the_startup_disk`'s device list, read once, is a fact about the machine (§ "A disk that goes away
mid-read"), and `releasing.rs`'s registry of reads in flight, where a handle-only walk and a leaving volume meet (§
"Letting go of a volume that's about to unmount").

## A repo's own commands never run

A repository's config can name commands (`filter.<driver>.clean` / `process`), and gix trusts a repo the user owns in
full, which covers one they downloaded or unzipped. Status hashes a file whose stat changed through the filter pipeline,
so browsing such a repo would run its commands. Cmdr only reads, so a filter buys nothing:

- `RepoCache::discover` opens every repo through `repo::without_filter_drivers`, which drops each `filter` section from
  the in-memory config. Nothing is written to disk.
- gix opens a submodule with its OWN, unstripped config, so gix's status looks only at a submodule's checked-out commit
  (`repo::submodule_status`). `repo::dirty_submodule_paths` opens each submodule, strips it the same way, and walks its
  worktree itself (nested submodules included, `ignore = dirty|all` respected); status marks those paths `Modified`, and
  the chip's dirty check counts them.
- ❌ Never call gix's `Repository::is_dirty` (it asks submodules "as configured") or open a repo around the cache.
  `status_never_runs_a_repos_filter_driver` pins both paths.

## A disk that goes away mid-read

`gix` memory-maps pack files, their indexes, and the commit-graph, and has no switch to read them instead (GitHub #488).
When the disk under an open repo goes away (a pulled USB cable, a forced eject, a network volume dropping), the next
read of a non-resident page raises SIGBUS. The app's handler maps zeros over that page instead of crashing
(`crates/cmdr-fs/DETAILS.md` § "Mapped-file faults: a dead page reads as zeros"), and `gix` carries on decoding zeros:
without more, the user would read "This repo looks damaged" (`CorruptRepo`) about a healthy repo on a drive that left.

**`fault_guard.rs` refuses the answer.** A `FaultWatch` reads the recovery's generation before an operation and again
after; one that moved means some page read in between may have been zeros, so the operation answers
`FriendlyGitErrorKind::VolumeGone` (the host words it as "the drive with this repo went away") and drops the repo's
cached status. The watches sit at every entry point a host reaches, inside a `GuardedRead` (which also carries the stop
flag of § "Letting go of a volume that's about to unmount"): `repo_info`, `list_status`'s walk, every `GitPortalVolume`
call (its `blocking` wrapper), and `GitPortal::category_rows`; `commit_rows_with_counts` takes a bare watch (its rows
then stay pending). The watcher's recompute and the chip's handshake go through `repo_info`.

- **No memo stores what a watch refused.** The status cache and the snapshot-dates memo are content- or mtime-keyed and
  would otherwise serve zeros for good, so both skip the insert when the walk under them tripped.
- **`RepoCache` forgets every handle once the generation moved** (`forget_all_after_a_fault`, at the next `discover`): a
  handle with a dead pack mapping would fault on every later read, and nothing says which repo's disk went. Reopening a
  healthy repo costs ~10 ms, once per vanished disk.
- **Address-free on purpose, with one cost.** The fault lands on whatever thread read the page, often one `gix` spawned
  (the status walk's), so nothing thread-local sees it, and nothing maps an address back to a repo. So an operation on a
  healthy repo that ran while ANOTHER repo's disk went away is refused once too, and its next call answers normally.
- **Evidence**: `fault_guard_real_image.rs` builds a repo with an 8 MiB packed blob on an HFS+ image, maps the pack,
  `detach -force`s the image, and reads the blob: the process lives, the watch tripped, and the read answers
  `VolumeGone`. Without the recovery handler it dies of SIGBUS; without the watch it answers `CorruptRepo` (Darwin
  27.0.0 arm64, 2026-10-19, `pnpm check disk-images`).

**Two things `RepoCache::discover` sets on every open, so fewer pages can fault at all:**

- **Packed-refs off the startup disk is read, ❌ never mapped.** gix maps a `packed-refs` over 32 KiB, and its one knob
  (`set_packed_buffer_mmap_threshold(u64::MAX)`) turns that into a read. `is_on_the_startup_disk` compares the common
  dir's device with `/`'s and the home directory's; anything else counts as a disk that can go away, a second internal
  volume included, which costs only memory. The startup devices are read once into a static: a fact about the machine,
  not state.
- **One allocation from a repo's object data is capped** at `OBJECT_ALLOC_LIMIT` (twice `tree::MAX_BLOB_BYTES`), via
  `gitoxide.objects.allocLimit` on the FULL-trust open options only: a size field decoded from zeros mustn't size an
  allocation, and a reduced-trust repo already gets gix's own 16 MiB cap, which an explicit limit would raise.

## Letting go of a volume that's about to unmount

A plain unmount (Finder's eject, `diskutil unmount`) FAILS while any file on the volume is mapped, and every open repo
keeps its packs mapped for as long as its handle lives, so an open repo used to block the user's eject (verified with a
RAM disk on Darwin 27.0.0 arm64, 2026-10-19). The app's DiskArbitration unmount approver asks
`GitPortal::release_volume(mount_root, deadline)` first (`apps/desktop/src-tauri/src/volumes/DETAILS.md` § "The unmount
approver"), which:

1. **Refuses to open or cache a repo under the root** (`RepoCache::let_go_under`), as `VolumeGone`, until `REOPEN_HOLD`
   (5 s) past the deadline: long enough to cover the unmount that follows an approval, short enough that a refused one
   hands git back soon. Time-bounded on purpose, so a refusal needs no second call to undo, and an open that was already
   in flight finishes but isn't cached.
2. **Drops the cached handles** under the root and their cached status, keeping only a `Weak` of each one's object
   store, which is what owns the pack mappings.
3. **Stops the long reads**: every `GuardedRead` (status walk, dirty walk, every portal-volume call) holds a `ReadLease`
   in `releasing.rs`, whose flag `gix`'s status walk polls (`should_interrupt_owned`) and the commit walk's
   `WalkControl` polls. A stopped read answers `VolumeGone`.
4. **Waits until every store is dropped and no lease under the root is left**, polling every 5 ms up to the deadline
   (gix has no drop hook to subscribe to), and answers whether that happened. The approver dissents when it didn't.

- **In memory only**: no `stat`, no `canonicalize`, which the approver's ask path forbids. Matching is by path segment
  against canonical roots, which a DiskArbitration mount root already is.
- **Why `releasing.rs` is a static** while everything else mutable is a `GitPortal` field: the status walk is reachable
  from `list_status` and `repo_info` with a handle alone, so that registry is the one place a walk and `release_volume`
  can meet. Each entry lives exactly as long as its read.
- **Residual**: a `discover` already past the leaving check when the release starts opens its repo and uses it once
  (uncached, and its handle isn't in the waited set), so an unmount racing it by milliseconds can still be refused.
- **Evidence**: `fault_guard_real_image::an_open_repo_lets_its_volume_unmount_once_released` shows the plain unmount
  failing with the pack mapped and going through after `release_volume`; the approver's own
  `real_image::a_repo_open_on_the_drive_lets_go_so_a_plain_unmount_goes_through` drives it through real DiskArbitration.

## A miss is not a damaged repo

Every portal lookup that can legitimately find nothing answers `Lookup<T>` (`Result<Option<T>, FriendlyGitError>`, in
`lib.rs`): `Ok(None)` means "that path isn't in this snapshot", `Err` means the repo couldn't answer at all.
`found_or_not_found` folds the first into `VolumeError::NotFound` carrying the path the caller asked for, which is what
the transfer layer renders as the user's own file name, and the second into `VolumeError::FriendlyGit` with the
git-specific repair copy.

Four things resolve to `Ok(None)`: a name `gix`'s tree walk doesn't find (`tree::resolve_tree_at`, `get_tree_entry`,
`lookup_blob`), a blob asked for as a directory or a directory asked for as a blob, a branch or tag whose ref isn't in
the repo (gix classifies it `Class::NotFound`, read as the error's `dominant_class()` so a corruption outranks it; ❌
never a string match), and any name under `worktrees/` or `submodules/` deeper than the leaf, since neither browses a
commit tree.

`log::resolve_commit_id` and `stash::resolve_stash_commit` still answer `Err` for a revspec they can't parse: an
unresolvable SHA is not the same shape of miss, and the friendly kinds (`ShallowBoundary`, `MissingObject`) carry repair
copy a bare `NotFound` would drop.

## Honest blob streaming

gix 0.81 returns whole-blob `Vec<u8>` for `Object::data` – there's no chunked loose-object reader exposed at the public
surface yet. So `GitBlobReadStream` owns the full `Vec<u8>` and yields 256 KB chunks for the consumer API shape.
**Memory cost equals blob size; chunked yield is for the consumer API, not memory streaming.** We refuse blobs over
`tree::MAX_BLOB_BYTES` (256 MB) up-front via `FriendlyGitErrorKind::BlobTooLarge` rather than OOM. Revisit when gix
exposes a chunked loose-object reader.

The stream reports the tree entry's mode (`VolumeReadStream::metadata`, from `tree::lookup_blob`'s `BlobEntry`: `0o644`,
`0o755` for an executable, `0o777` for a link), so a script copied out of a snapshot onto local disk lands executable
with no second lookup. No date: one blob sits in many commits.

## Ref-name flat rendering

Branches like `feature/foo` show as a single entry called `feature/foo`, not nested `feature/` then `foo`. The
classifier (`path::classify`) greedy-matches ref names against the repo's known refs (longest-first) before treating any
remainder as a tree sub-path. This is the only place where classification needs the repo open, and the only reason
`classify_names_every_shape_against_a_real_repo` carries a fixture.

## Modified + Size columns for virtual entries

Every virtual entry carries a real `modified_at`, and most carry a typed `git_meta` (`GitEntryMeta` in
`crates/cmdr-fs/src/git_meta.rs`) stating what the Size cell should say. The backend ships the FACT; the frontend words
it (`src/lib/file-explorer/views/full-list-utils.ts::wordGitMeta`), so every row reads in the user's language with that
language's plural rules. ❌ Never put a sentence in a variant.

| Path                           | `modified_at`                                                                        | `git_meta`                                              | `size` (sort key)               |
| ------------------------------ | ------------------------------------------------------------------------------------ | ------------------------------------------------------- | ------------------------------- |
| `.git/branches/`               | newest branch tip date                                                               | `Count { Branches }`                                    | branch count                    |
| `.git/tags/`                   | newest tag/commit date                                                               | `Count { Tags }`                                        | tag count                       |
| `.git/commits/`                | HEAD committer date                                                                  | `Count { Commits }`                                     | commit count (capped at 15,000) |
| `.git/stash/`                  | newest stash creation date                                                           | `Count { StashEntries }`                                | stash count                     |
| `.git/worktrees/`              | newest linked worktree HEAD                                                          | `Count { LinkedWorktrees }`                             | worktree count                  |
| `.git/submodules/`             | newest pinned commit                                                                 | `Count { Submodules }`                                  | submodule count                 |
| `branches/<name>/`             | branch tip committer date                                                            | `AheadBehind` vs upstream (or fallback `main`/`master`) | ahead-count                     |
| `tags/<name>/`                 | annotated tag date or commit date                                                    | `TaggedCommit`                                          | 0                               |
| `commits/<sha>/`               | commit committer date                                                                | `Count { FilesChanged }`                                | files-changed count             |
| `stash/<n>/`                   | stash creation date                                                                  | `StashedOnBranch` (parsed from the stash subject)       | 0                               |
| `worktrees/<name>` (redirect)  | worktree HEAD date                                                                   | `WorktreeOnBranch` or `WorktreeDetachedAt`              | 0                               |
| `submodules/<name>` (redirect) | pinned commit date                                                                   | `PinnedCommit`                                          | 0                               |
| inside snapshots: files        | most recent commit that touched the file (fallback: snapshot commit date)            | None (blob bytes)                                       | blob bytes                      |
| inside snapshots: subdirs      | most recent commit that touched any file underneath (fallback: snapshot commit date) | None (recursive bytes)                                  | recursive blob bytes            |

Cross-category Size sort is meaningless (ahead-count vs files-changed vs item count); that's an honest tradeoff. Each
cell is self-explaining via its tooltip, which is also the aria-label.

`modified_at` is the ONLY date a row carries: ❌ never copy it (or a commit's author time) into `created_at` /
`added_at`. The portal claims Modified only (`supported_dates` keeps the trait default), so a Created or Added sort here
falls back to Modified with the pane's hint; a copied date would silently turn that sort into a Modified sort with no
hint. Why each date is left out: `crates/cmdr-fs/DETAILS.md` § "Which dates each volume lists".

**Gotcha**: a commit id crosses IPC in FULL, and the cell shortens it to seven characters on the frontend. **Why**: the
tooltip names the whole id, so shipping only the short form would mean shipping both. The seven is a display choice
(`SHORT_ID_LENGTH` in `full-list-utils.ts`), which is where a display choice belongs.

**Decision**: `AheadBehind` carries the comparison branch's name **Why**: the tooltip reads "3 commits ahead, 1 commit
behind `origin/main`", and which branch that is depends on whether the branch has a configured upstream or fell back to
`main` / `master`. Only the backend knows; carrying `vs` is what lets the frontend word the sentence without asking
again.

**Decision**: Eager-load ahead/behind for branches; count commits' files-changed AFTER the walk, in full only up to
5,000 commits **Why**: Bench (release build, M-series): 100 branches with ahead/behind takes p50=33 ms / p95=36 ms, well
under the 300 ms p95 budget the spec sets for the listing pipeline. A commit row costs ~230 µs WITH its files-changed
diff and ~11 µs without (Cmdr's own 10,646 commits: ~2.4 s of CPU time vs 116 ms; CPU time because the machine was under
heavy load, release build, `list_commits` on the Cmdr repo, 2026-10-14). So the walk counts nothing: every row is
`GitEntryMeta::CountPending` (shown as `…`), and the list with its total lands at once. Then the pane asks for a
whole-listing pass (`apps/desktop/src-tauri/src/file_system/git/commit_counts.rs`): at most `GitPortal::COUNT_ALL_LIMIT`
(5,000) commits are counted in full, in 200-row batches, so the cells fill and Size sort works. A longer listing answers
`TooMany`: only the rows a pane shows get counted, and Size sort is refused with a toast, since most rows would have
nothing to sort by. Counted rows are patched into the listing as `DirectoryChange::Modified`, so they re-sort and stay,
through the listing engine alone, ❌ never `listing_host::directory_changed`, which would also feed virtual rows to the
file index.

## Decisions

**Decision**: Per-file Modified dates inside snapshot listings via walk-once batching **Why**: The snapshot date ("when
this commit landed") is the same value for every file inside a `branches/main/`, `commits/<sha>/`, etc. listing:
semantically correct as a "frozen point in time", but useless as a "when did I last work on this?" hint. We now run a
single rev-walk per `(commit_id, dir_path)` listing: from the snapshot commit backwards by commit time, first-parent
only, diffing each commit against its first parent (gix's `Tree::changes()::for_each_to_obtain_tree`). Each
`Change.location` is matched against the directory's top-level entries; the first-seen commit's committer time wins. The
walk stops early when every entry is dated, after `MAX_COMMITS_PER_WALK` (1000), or when the rev-walk exits. Initial
commits short-circuit. Cache is process-global, FIFO-bounded at 50 keys, content-addressable so it never invalidates.
Bench: 100 entries × 5000 commits cold p95=21 ms (budget 200 ms), warm p95=2 µs. 50k-commit fixture sits inside the 500
ms budget too. Entries that don't surface within the cap fall back to the snapshot date so the cell never reads as
blank.

**Decision**: Cache `list_status` results keyed by `.git/index` mtime **Why**: A naive implementation would walk the
worktree on every `listing-complete` (every nav, every diff). On a 50k-file repo that's ~75 ms per nav. We run one
full-repo walk per index change, store the result in a process-global `RwLock<HashMap<RepoRoot, CachedStatus>>`, and
slice by `dir_in_worktree` on each call. Cached calls land sub-millisecond on the same fixture (warm p95 in the bench is
bounded by an arbitrary 5 ms ceiling so a busy CI doesn't flake). The watcher (`watcher.rs::recompute_and_report`) drops
the cache entry on every `.git/*` mutation it observes, BEFORE the report goes out, so a subscriber that reacts by
asking for status can't be answered with the walk it just invalidated. The `unsubscribe`-on-last-pane path also drops
the entry so an unwatched repo doesn't pin a full-repo-sized snapshot.

**Decision**: Always run with `--untracked-files=normal`, no "skip untracked outside the worktree root" trick **Why**:
Passing `--untracked-files=no` for sub-path listings would avoid the full untracked walk per call, but with the
index-mtime cache above, the untracked walk runs once per index change anyway and the cost is amortized across every
subsequent listing. The extra complexity (two code paths, mismatched cache keys for the same repo) buys nothing
measurable. We always walk the full worktree with `--untracked-files=normal` and let the cache do the work.

**Decision**: Typed `VolumeError::FriendlyGit(FriendlyGitError)` variant **Why**: The portal volume's methods return
`Result<_, VolumeError>` and the streaming pipeline calls `listing_error_from_volume_error` to compute the `ErrorPane`
payload. We carry the structured payload through a typed enum variant so the path from "git layer detected something" to
"frontend renders the git error pane" is type-checked end-to-end. Don't revert to stuffing a sentinel-tagged string into
`VolumeError::IoError::message` and parsing it in the friendly mapper – that's string-shaped data inside a typed enum, a
maintenance hazard, and violates the no-error-string-match rule.

**Decision**: Shell out to `git stash list` rather than driving gix **Why**: gix 0.81 doesn't expose a public stash-list
API. We could parse the `refs/stash` reflog by hand, but `git stash list -z --format=%H%x09%gd%x09%s%x09%ct` gives us
git's canonical ordering, the exact `stash@{n}` indices users see in the terminal, and the commit-time / subject in one
shot. The `git` CLI is already a system requirement. Resolution of `stash@{n}` to a commit ID also goes through
`git rev-parse stash@{n}` for the same reason – gix can't expand the `stash@{n}` syntax.

**Decision**: Browse the **W (working-tree) commit** for stash entries **Why**: `git stash` records the dirty worktree
as a merge commit (the "W" commit in git docs); its first parent ("B") is HEAD at stash time which is the _clean_ tree,
not the stashed changes. Browsing W matches what `git stash show <n>` shows. Verified against fixture: the file listing
under `.git/stash/0/` matches `git stash show 0 --name-only`.

**Decision**: gix `Repository::worktrees()` for the linked-worktree list **Why**: gix exposes a
`worktrees() -> Vec<worktree::Proxy>` that reads `<common-dir>/worktrees/*/gitdir` and gives us the working-tree base
path via `proxy.base()`. No shell-out needed. We skip proxies whose `base()` is missing – orphaned linked worktrees stay
invisible rather than break the listing.

**Decision**: gix `Repository::submodules()` for submodule listing **Why**: gix reads `.gitmodules` and yields one
`Submodule` per entry with name + path. We resolve the submodule's working dir as `<repo_root>/<rel-path>` and set it on
`redirect_to_path` so the frontend opens the working dir directly. The submodule itself is a git repo so the portal
experience cascades for free.

**Decision**: Commit log capped at 100,000 entries, silent cap **Why**: A hard cap keeps pathological monorepos from
listing unbounded. With lazy files-changed (above) a row costs ~11 µs and ~576 bytes of listing cache, so the cap is ~1
s plus the eager share and ~58 MB; 500,000 would be ~290 MB for one open pane. When the cap is hit the walk stops
silently (no "Load more" affordance, because tapping it would do nothing useful: pagination IPC isn't wired). When the
first user reports hitting the cap, add the IPC + a real Load-more entry together so the affordance actually works.

**Decision**: A portal listing stays single-shot, with progress and cancellation through a per-listing `WalkControl`
**Why**: The `Volume::list_directory` contract is "compute Vec, return", and the portal honours it. The commit walk
publishes how many rows it has into the listing's own `WalkControl`, which `list_directory` samples every 200 ms through
`cmdr_fs::volume::listing_progress::report_while` (the local reader's helper too) and hands to the pane as progress. A
`spawn_blocking` task can't be aborted, so dropping the listing future (the pane left) sets the control's "abandoned"
flag through a drop guard, and the walk stops at the next commit instead of burning seconds for nobody. Per listing, ❌
never a static: two listings must not cancel each other.

**Decision**: One recursive watch on the whole gitdir, ❌ not a watch per state directory **Why**: each `watch` call
restarts the macOS FSEvents stream, so one path is one start (§ "Watcher path set"). It also needs no glob and no
enumeration at subscribe time, and it covers linked worktrees and state directories that appear AFTER the subscribe.
`is_repo_state_path` keeps the extra subtree from turning into extra reports.

**Decision**: `Cat::browses_commit_tree()` covers branches/tags/commits/stash **Why**: All four categories browse a
commit tree, just resolved differently. Branches/tags peel through refs, commits resolve a SHA prefix, stash expands
`stash@{n}`, but the _tree-walking_ code path is identical. The method name describes the contract. The dispatch lives
in `lib.rs::resolve_commit_for_cat`.

**Decision**: Use `gix::Repository::status()` for `list_status` (not a `git status --porcelain=v2 -z` shell-out)
**Why**: In gix 0.81, `Repository::status().into_iter()` runs both a `TreeIndex` leg (HEAD vs index, for staged changes)
and an `IndexWorktree` leg (index vs worktree, for unstaged changes) in parallel. The `TreeIndex` leg surfaces staged
additions correctly even in single-commit repos. The gix approach is fully typed (no string parsing of stderr), which
keeps us inside the no-error-string-match rule. Error mapping is entirely through typed enum variants. Don't reintroduce
a shell-out: parsing `stderr.contains(...)` for error classification was the previous failure mode and it's banned.

**Decision**: repo discovery rejects bare repos via `BareRepo` **Why**: The whole UX (chip, status column, future
portal) is anchored on a working tree. Showing a chip for a bare repo is meaningless. The
`FriendlyGitErrorKind::BareRepo` variant tells the user clearly what's up without claiming a problem.

**Decision**: `RepoCache::lookup_for_path` returns the _longest_ matching root **Why**: HashMap iteration is unordered.
With nested submodules both the parent and the child match `canonical.starts_with(root)`. Picking the shortest (parent)
would surface the wrong repo for paths inside the child; picking the first match (HashMap order) is non-deterministic.
We pick the longest matching root – that's always the deepest enclosing worktree, which is the right answer for both
submodules and linked worktrees.

**Decision**: `RepoCache` holds a handle until the last unsubscribe, with no idle TTL **Why**: Re-opening a
`gix::Repository` is cheap (~10 ms on warm caches) but not free; the cache pins one handle per active subscriber so
back-to-back chip lookups skip the open. Eviction stays simple: an idle TTL would need a timer thread for nearly no
gain. The cache itself is a VALUE the `GitPortal` owns, ❌ not a static of its own (§ "`GitPortal`: the value that owns
the cache").

**Decision**: Watcher uses `notify-debouncer-full` rather than a custom poll loop **Why**: The rest of the codebase
already depends on `notify` and `notify-debouncer-full` for filesystem watching (see `crates/cmdr-listing/` and the SMB
share watcher). Reusing it gives us 200 ms debounce, OS-level event coalescing, and a battle-tested teardown path.

**Decision**: `gix` builds with `default-features = false` plus `sha1`, `status`, `revision`, `max-performance-safe`,
and `auto-chain-error` (each one's reason sits beside it in `Cargo.toml`) **Why**: the defaults' `extras` bundle is
write, network, and signal machinery (`worktree-mutation` with `dua-core` + `crossbeam`, `credentials`, `merge`,
`blame`, `worktree-archive`, `interrupt`) this read-only crate never calls. Trimming it took cmdr-git's normal deps from
170 to 150 and a clean `cargo build -p cmdr-git` from ~45 s to ~24 s (M1 Max, gix 0.89, 2026-10-12). `parallel` is
load-bearing, not tuning: without it `ThreadSafeRepository` holds `Rc`s and `GitPortal` stops being `Send + Sync`
(verified by `cargo check`). A new gix API that won't resolve probably needs its feature added here, not a default.

## Gotchas

**Gotcha**: gix's `ThreadSafeRepository::work_dir()` is deprecated but the new name (`workdir`) only exists on
`Repository`, not `ThreadSafeRepository` **Why**: We hold an `Arc<ThreadSafeRepository>` for the cache (it's
`Send + Sync`) and call `work_dir()` on it once. The deprecation is suppressed inline with a `#[allow]` carrying that
exact reason.

**Gotcha**: The bench tests share one fixture dir (`target/test-fixtures/ git/synth-50k/`). Without a `BUILD_LOCK`
mutex, they raced each other into half-built `.git` dirs when run in parallel **Why**: `cargo test` defaults to
threads-per-core. The fixture builder checks `dir.join(".git").exists()` to skip rebuild, but the check raced with the
actual `git init`. The mutex serializes the build; concurrent runs of the test bodies themselves are fine because they
only read.

**Gotcha**: `is_dirty()` runs a worktree walk, so `repo_info` is the expensive call in the chip pipeline **Why**: On 50k
files it dominates the ~60 ms total. Don't add more work on the chip refresh path without re-benchmarking.

**Gotcha**: `repo_info` reads `.git/config` from the cached repo, but refs from disk, so config changes don't reflect
live while ref changes do **Why**: The process-global `RepoCache` hands out a long-lived `Arc<ThreadSafeRepository>`
that loaded `.git/config` once at open. Config-derived fields (the upstream, hence `ahead`/`behind`) keep using that
snapshot until the cache is dropped (app restart) — a live `git branch --unset-upstream` won't change the chip. But
`repo_info` resolves the upstream _ref_ via `find_reference` on every call, so moving `refs/remotes/origin/main` (for
example `git update-ref refs/remotes/origin/main main` to zero the ahead count) IS picked up on the next chip refresh.
This is the lever the screenshot guide uses to force a clean `main` chip; see `docs/guides/screenshots.md`.
