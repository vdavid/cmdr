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

**A repo's `.git/` listing mixes real entries (HEAD, config, hooks/, objects/, refs/, …) with the six virtual
categories so the user sees everything in one place.** The real half is the LOCAL volume's ordinary `read_dir`; the six
join it in the listing pipeline, through the overlay (§ "Two seams, no hooks").

## File map

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` § Module
map. What each piece DOES is in the sections below: the route and the overlay in § "Two seams, no hooks", the toggle in § Decisions
("Live-toggleable portal"), `classify`'s greedy ref matching in § "Ref-name flat rendering", the status cache, the
snapshot-date walk, the 5000-commit cap, the stash shell-out, the `worktrees()` / `submodules()` choices, `RepoCache`'s
longest-root lookup, and the typed `VolumeError::FriendlyGit` variant all in § Decisions, blob memory in § "Honest blob
streaming", and the column semantics in § "Modified + Size columns". Only the layout facts that none of those carry live
here:

- **`tree.rs` reflects `EntryKind::BlobExecutable` into the entry's permissions**, which is what makes a cross-volume
  copy out of the portal preserve the executable bit.
- **`friendly.rs` is classification only, deliberately word-free.** `kind.category()` maps a variant to an
  `ErrorCategory` and `raw_detail()` builds the technical-details string (kind token + path/raw); ALL user-facing copy
  lives on the frontend in `src/lib/error-messages/git-error-messages.ts`, and so do the writing-rules tests
  (`friendly-error-style.test.ts`, every kind × rendered output). Adding a variant means touching both sides.
- **`watcher.rs` names no window.** It notices a relevant `.git/*` mutation, recomputes `RepoInfo`, drops the status
  cache, and reports through `GitStateSink` (`state_sink.rs`). `wiring.rs` is the app's answer: it emits
  `git-state-changed` and calls `notify_directory_changed(.., FullRefresh)` for every cached listing under any of the
  repo's six category prefixes, so an open portal pane refreshes rather than showing stale children. Every volume, not
  only the boot one: a repo on an external disk gets its own volume id, and its portal panes went stale while that
  filter was there.
- **`overlay.rs` is the `.git/` half**: the `ListingOverlay` contributor that puts the six category rows into a repo's
  `.git/` listing, for a PANE and nothing else (`src/listing_overlays.rs`).
- **No English leaves this module for the Size column.** `column_meta.rs` and the per-category listers hand back numbers and ids on a typed `git_meta`; the words are the frontend's, from the message catalog. See § "Modified + Size columns for virtual entries".
- **`log.rs::resolve_commit_id` resolves a SHA prefix even for an UNREACHABLE commit**, so browsing
  `commits/<sha>` works for something the rev-walk would never list.
- **`walker_exposure_tests.rs` pins which non-pane walkers can reach a virtual entry**, because that set is what the
  portal's blast radius IS. See § "What each walker sees" below.
- **`bench.rs` never runs in a normal suite**: every bench in it is `#[ignore]`d because each builds its own synth
  fixture (a 50k-file repo, a 100-branch repo, a 5k-commit history), cached once under `target/test-fixtures/git/`. The
  run command lives in the module's own header, so it can't drift from the test names.

## What each walker sees

A virtual entry is a name with no inode. A pane renders one; anything that stats, copies, or removes one meets a path
that isn't there. **No walker can reach one**, and that follows from the shape rather than from a guard (verified on
macOS 26.6, `cargo test --lib file_system::git::walker_exposure_tests`, 2026-09-05):

- **A walker lists through `Volume::list_directory`, and no `Volume` holds a category row.** The six reach a pane from
  the listing pipeline's overlay step; `LocalPosixVolume` answers `.git/` with what `read_dir` found. So the volume
  delete walker, the trait scanner, and every other `Volume` consumer see a plain directory.
- **The routed portal volume can't be walked into by accident either.** It serves only `.git/<category>/…`, refuses
  every mutation by trait default, and `mount_id_for_path` skips it by TYPE so nothing indexes through one.
- **The copy scan never asks a volume at all.** `local_posix/scan.rs` walks with `walkdir` against the resolved path.
- **The LOCAL delete walker asks the fresh-listing oracle first**, and the oracle declines any listing an overlay
  decorated (`CachedListing::has_overlay_rows`). That is the one place where the pane's extra rows and a walker's read
  could have met: `.git/` is a real directory, so a pane on one arms a real watch and the listing would otherwise pass
  the freshness test. ❌ Never let a decorated listing answer that oracle.
- **The drive index CAN'T.** Local volumes are walked by `cmdr-index`'s guarded walker with raw syscalls, and that
  crate can't name the app's git module.

Real files under `.git` are ordinary local files throughout: readable, writable, renamable, and deletable, with the
portal on as much as off, which is what lets a repo-folder delete walk `.git/` to the end.

## Linked worktrees

`git worktree add` writes `.git` as a FILE holding `gitdir: <common>/worktrees/<name>`.

**The portal lives where `.git` is a DIRECTORY.** The overlay contributes to a directory listing, and listing a gitlink
fails `ENOTDIR`, so a linked worktree has no `.git/` landing page. What it does have is everything below:
`path::classify` splits on the path SEGMENT and never stats, `portal_route` is the same pure string check, and `gix`
discovery follows the gitlink, so `<linked>/.git/branches` and deeper answer exactly as in the main worktree (verified
2026-09-05, `a_linked_worktree_serves_the_categories_but_has_no_dot_git_landing`).

**Why that's the right trade.** The landing listing used to rewrite the real gitdir's entries under `<linked>/.git/…`,
where they were visible but not openable (`<linked>/.git/HEAD` is a path THROUGH a file, so reading it is `ENOTDIR`).
Losing rows nobody could open costs less than a `stat` on the route, which runs on every path-bearing call.

## Tauri commands

Wired from `commands/file_system/git.rs`:

- `get_git_repo_info(path) -> TimedOut<Option<RepoInfo>>` – one-shot lookup, 2 s timeout
- `subscribe_git_state(repo_root) -> Result<RepoInfo, GitSubscribeError>` – registers a subscriber, returns current `RepoInfo` synchronously, then emits `git-state-changed` events. 2 s timeout (the synchronous handshake discovers the repo and computes `repo_info` so a hung repo would otherwise freeze IPC). `GitSubscribeError` (in `commands/file_system/git.rs`) is `Git { error: FriendlyGitError }` / `TimedOut` / `Unexpected { detail }`, so git's own typed kind reaches the frontend intact and `src/lib/error-messages/git-error-messages.ts` words it per locale
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

Bench result on the 50k-file synth repo, release build (`cargo test --release --lib file_system::git::bench --
--ignored`), measured on an M1 Max, gix 0.87, 2026-09-05, median of three runs:

| Metric | Budget | Measured |
|---|---|---|
| `discover + repo_info` p50 | 50 ms target | ~87 ms |
| `discover + repo_info` p95 | 100 ms hard cap | ~108 ms |
| `list_status` cold p50 | 100 ms | ~67 ms |
| `list_status` cold p95 | 100 ms | ~86 ms |
| `list_status` warm p50 | – | ~96 µs |

`list_status` lands inside budget cold, and a warm call is a cache hit in microseconds. Subsequent repo discovery
calls hit the portal's repo-handle cache and run in microseconds too.

**`discover + repo_info` is over its hard cap on this hardware**, so
`bench_50k_files_discover_and_repo_info_under_budget` fails when run. All of it is `repo_info`'s `is_dirty()`, a full
worktree walk; discovery itself is a cache hit after the first call. It's a real number to act on rather than a
measurement artifact: `git status --untracked-files=no --porcelain` walks the SAME fixture in 50 ms on this machine
(five runs, 2026-09-05), where the earlier table recorded 75 ms for it, so this machine is the faster one and `is_dirty`
still costs ~26 ms more than the number that table carried. The likeliest cause is the gix 0.81 → 0.87 bump the earlier
numbers predate. ❗ Nothing in the chip pipeline was made slower by the routing work: `repo_info` and `is_dirty` are
untouched by it. Worth a look before the next release, since this is the call the breadcrumb chip waits on.

The bench is `#[ignore]`d, so no check lane runs it; take a reading with the command above after any change to the chip
pipeline.

## The portal is a routed volume

`GitPortalVolume` (`volume.rs`) is a read-only `Volume` over one repo's virtual trees, the same shape `ArchiveVolume`
has. `VolumeManager::resolve` routes any path with a `.git/<category>/` segment to it (`volume/DETAILS.md` §
"Resolving a path: the two routes") and hands it the input path verbatim; the volume maps that path to
`(repo, category, ref, tree path)` with `path::classify_in` and answers from the same `virtual_listing` / `log` /
`tree` code the overlay's row builder calls.

- **Its namespace is the six categories and what's under them, nothing else.** Listing its root
  (`<worktree>/.git`) answers the six category rows ALONE, via `virtual_listing::list_categories`. Real `.git/*`
  entries are the parent volume's, which is what keeps `.git/config` editable and lets a repo-folder delete walk
  `.git/` as an ordinary directory. A pane sees the two halves together because the listing pipeline
  folds the overlay's rows into the local volume's read.
- **A `.git` that isn't a repository is `NotFound`, decided here.** Routing is lexical and does no I/O, so "is there
  actually a repo at this path?" is answered on first use, through the portal's `RepoCache`. ❌ Don't add a `stat` to
  the route to pre-empt it.
- **What the trait answers**: `is_writable` false and every mutation on the trait default (`create_directory_all` is
  overridden, since its default would claim success for a directory that exists); `supports_export` and
  `supports_streaming` true, with `open_read_stream` handing back a `GitBlobReadStream`; `can_watch_listings` false and
  `listing_watch_coverage` `None`, because the paths aren't on disk; `supports_local_fs_access` false and `local_path`
  `None` for the same reason; `lane_key` and `get_space_info` delegate to the PARENT volume, since the objects live on
  its disk. `scan_for_copy` and the batch scan come from `cmdr_fs::volume::scan_walk` through a two-method
  `ScanSource`, which is what lets a whole branch tree be copied out to another volume.
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
mean a test's evictions reaching the app's handles and back.

## Two seams, no hooks

`LocalPosixVolume` names git nowhere. `rg 'git' backends/local_posix.rs` is empty, and the ten `if` sites it used to
carry (three route hooks, five mutation guards, the `notify_mutation` early return, plus the `is_virtual` watch skip in
`listing/streaming.rs`) are gone. Two seams replace them, each with a rule a type enforces:

1. **The route.** `VolumeManager::resolve` sends any `.git/<category>/…` path to the read-only `GitPortalVolume`. Read
   only by trait default, unwatchable by `can_watch_listings() == false`, invisible to `mount_id_for_path` by type.
   `volume/DETAILS.md` § "Resolving a path: the two routes".
2. **The overlay.** `git::overlay::GitPortalOverlay` contributes the six category rows to a repo's `.git/` listing, run
   by `listing/streaming.rs::read_directory_with_progress` (and `listing/operations.rs`, and again on every
   watcher-driven `FullRefresh`) after the volume's entries arrive and before the cache insert. Pane-only by
   construction: `src/listing_overlays.rs`.

Both consult ONE app-side switch, `git::wiring::is_virtual_portal_enabled()`, so the toggle is "no route" plus "no
contribution". Both also ask `wiring::volume_holds_real_repos`, which is `local_path().is_some()`: `gix` can't open a
path only a protocol can reach, so a `.git/branches` on a direct-SMB share stays an ordinary folder.

**The overlay's predicate**: the listed directory's last segment is `.git`, the volume answers `local_path().is_some()`
(so `gix` can open its paths), and the portal is on. ❗ Cheap and zero-I/O, because it runs on every listing in the app;
"is there a repository here?" is answered when the rows are built, not when the predicate is asked. The route asks the
volume half of the same question (`git::overlay::volume_holds_real_repos`), so the portal appears in exactly one set of
places.

**A `.git` inside a repo's working tree that isn't that repo's gitdir gets nothing.** `gix` discovery walks UP, so a
placeholder directory named `.git` in a test corpus would otherwise be handed the enclosing repo's branches; the
contributor compares the discovered root against the listed directory's parent.

## A miss is not a damaged repo

Every portal lookup that can legitimately find nothing answers `Lookup<T>` (`Result<Option<T>, FriendlyGitError>`, in `mod.rs`): `Ok(None)` means "that path isn't in this snapshot", `Err` means the repo couldn't answer at all. `found_or_not_found` folds the first into `VolumeError::NotFound` carrying the path the caller asked for, which is what the transfer layer renders as the user's own file name, and the second into `VolumeError::FriendlyGit` with the git-specific repair copy.

Four things resolve to `Ok(None)`: a name `gix`'s tree walk doesn't find (`tree::resolve_tree_at`, `get_tree_entry`, `lookup_blob_id`), a blob asked for as a directory or a directory asked for as a blob, a branch or tag whose ref isn't in the repo (typed `gix::reference::find::existing::Error::NotFound`, ❌ never a string match), and any name under `worktrees/` or `submodules/` deeper than the leaf, since neither browses a commit tree.

`log::resolve_commit_id` and `stash::resolve_stash_commit` still answer `Err` for a revspec they can't parse: an unresolvable SHA is not the same shape of miss, and the friendly kinds (`ShallowBoundary`, `MissingObject`) carry repair copy a bare `NotFound` would drop.

## Honest blob streaming

gix 0.81 returns whole-blob `Vec<u8>` for `Object::data` – there's no chunked loose-object reader exposed at the public surface yet. So `GitBlobReadStream` owns the full `Vec<u8>` and yields 256 KB chunks for the consumer API shape. **Memory cost equals blob size; chunked yield is for the consumer API, not memory streaming.** We refuse blobs over `tree::MAX_BLOB_BYTES` (256 MB) up-front via `FriendlyGitErrorKind::BlobTooLarge` rather than OOM. Revisit when gix exposes a chunked loose-object reader.

## Ref-name flat rendering

Branches like `feature/foo` show as a single entry called `feature/foo`, not nested `feature/` then `foo`. The classifier (`path::classify`) greedy-matches ref names against the repo's known refs (longest-first) before treating any remainder as a tree sub-path. The inverse (`to_path`) splits ref names on `/` so OS-native separators are used in the on-disk representation. This is the only place where the URL → path round-trip needs the repo open.

## Modified + Size columns for virtual entries

Every virtual entry carries a real `modified_at`, and most carry a typed `git_meta` (`GitEntryMeta` in
`crates/cmdr-fs/src/git_meta.rs`) stating what the Size cell should say. The backend ships the FACT; the frontend words
it (`src/lib/file-explorer/views/full-list-utils.ts::wordGitMeta`), so every row reads in the user's language with that
language's plural rules. ❌ Never put a sentence in a variant.

| Path | `modified_at` | `git_meta` | `size` (sort key) |
|---|---|---|---|
| `.git/branches/` | newest branch tip date | `Count { Branches }` | branch count |
| `.git/tags/` | newest tag/commit date | `Count { Tags }` | tag count |
| `.git/commits/` | HEAD committer date | `Count { Commits }` | commit count (capped at 5000) |
| `.git/stash/` | newest stash creation date | `Count { StashEntries }` | stash count |
| `.git/worktrees/` | newest linked worktree HEAD | `Count { LinkedWorktrees }` | worktree count |
| `.git/submodules/` | newest pinned commit | `Count { Submodules }` | submodule count |
| `branches/<name>/` | branch tip committer date | `AheadBehind` vs upstream (or fallback `main`/`master`) | ahead-count |
| `tags/<name>/` | annotated tag date or commit date | `TaggedCommit` | 0 |
| `commits/<sha>/` | commit committer date | `Count { FilesChanged }` | files-changed count |
| `stash/<n>/` | stash creation date | `StashedOnBranch` (parsed from the stash subject) | 0 |
| `worktrees/<name>` (redirect) | worktree HEAD date | `WorktreeOnBranch` or `WorktreeDetachedAt` | 0 |
| `submodules/<name>` (redirect) | pinned commit date | `PinnedCommit` | 0 |
| inside snapshots: files | most recent commit that touched the file (fallback: snapshot commit date) | None (blob bytes) | blob bytes |
| inside snapshots: subdirs | most recent commit that touched any file underneath (fallback: snapshot commit date) | None (recursive bytes) | recursive blob bytes |

Cross-category Size sort is meaningless (ahead-count vs files-changed vs item count); that's an honest tradeoff. Each
cell is self-explaining via its tooltip, which is also the aria-label.

**Gotcha**: a commit id crosses IPC in FULL, and the cell shortens it to seven characters on the frontend.
**Why**: the tooltip names the whole id, so shipping only the short form would mean shipping both. The seven is a
display choice (`SHORT_ID_LENGTH` in `full-list-utils.ts`), which is where a display choice belongs.

**Decision**: `AheadBehind` carries the comparison branch's name
**Why**: the tooltip reads "3 commits ahead, 1 commit behind `origin/main`", and which branch that is depends on
whether the branch has a configured upstream or fell back to `main` / `master`. Only the backend knows; carrying `vs`
is what lets the frontend word the sentence without asking again.

**Decision**: Eager-load ahead/behind for branches; eager-load files-changed for commits
**Why**: Bench (release build, M-series): 100 branches with ahead/behind takes p50=33 ms / p95=36 ms, well under the 300 ms p95 budget the spec sets for the listing pipeline. Files-changed for 200 commits: p50=37 ms / p95=40 ms (200 µs / commit), so the typical Cmdr-sized repo (~3000 commits) lands ~600 ms and the 5000-commit cap lands ~1 s. We accept the worst-case 1 s on the cap because (1) Cmdr's own repo never hits the cap, (2) the listing pipeline runs the row build in `spawn_blocking` so the UI stays responsive, and (3) the alternative (lazy-load via a streamed IPC) would mean another round-trip per row and a placeholder `…` in the cell while it resolves. Worth re-checking if a user reports the 5000-commit cap feeling slow; the bench harness in `bench.rs` covers 1000 commits and `bench_list_commits_files_changed` covers 200.

## Decisions

**Decision**: `.git/` shows real entries and the six virtual ones together; no `raw/` escape hatch
**Why**: Hiding real `.git/*` contents behind a separate `raw/` category meant two extra clicks for anyone wanting to peek at `HEAD`, `config`, `hooks/`, or `objects/`. The virtual rows already cover the friendly view; showing the real entries in the same listing gives one-click access with no indirection, and costs nothing on the read side because the real half IS the local volume's ordinary listing. A real entry whose name collides with a virtual category gives way to it (the overlay's shadowing rule): the deprecated `.git/branches/` directory git stopped writing years ago, and `.git/worktrees/` in a linked-worktree setup, whose internals belong to git rather than to the user. The pane's own sort orders the merged rows; the six carry no sort privilege of their own.

**Decision**: Per-file Modified dates inside snapshot listings via walk-once batching
**Why**: The snapshot date ("when this commit landed") is the same value for every file inside a `branches/main/`, `commits/<sha>/`, etc. listing: semantically correct as a "frozen point in time", but useless as a "when did I last work on this?" hint. We now run a single rev-walk per `(commit_id, dir_path)` listing: from the snapshot commit backwards by commit time, first-parent only, diffing each commit against its first parent (gix's `Tree::changes()::for_each_to_obtain_tree`). Each `Change.location` is matched against the directory's top-level entries; the first-seen commit's committer time wins. The walk stops early when every entry is dated, after `MAX_COMMITS_PER_WALK` (1000), or when the rev-walk exits. Initial commits short-circuit. Cache is process-global, FIFO-bounded at 50 keys, content-addressable so it never invalidates. Bench: 100 entries × 5000 commits cold p95=21 ms (budget 200 ms), warm p95=2 µs. 50k-commit fixture sits inside the 500 ms budget too. Entries that don't surface within the cap fall back to the snapshot date so the cell never reads as blank.

**Decision**: Cache `list_status` results keyed by `.git/index` mtime
**Why**: A naive implementation would walk the worktree on every `listing-complete` (every nav,
every diff). On a 50k-file repo that's ~75 ms per nav. We run one full-repo
walk per index change, store the result in a process-global
`RwLock<HashMap<RepoRoot, CachedStatus>>`, and slice by `dir_in_worktree` on
each call. Cached calls land sub-millisecond on the same fixture (warm p95 in
the bench is bounded by an arbitrary 5 ms ceiling so a busy CI doesn't flake).
The watcher (`watcher.rs::recompute_and_report`) drops the cache entry on every
`.git/*` mutation it observes, BEFORE the report goes out, so a subscriber that reacts by asking for status can't be
answered with the walk it just invalidated. The
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
**Why**: One atomic load, read by the route before it routes and by the overlay before it contributes, so the toggle
costs nothing on a path that isn't a repo's. The setter is wired live from the frontend
(`set_show_virtual_git_portal`) and seeded at startup from `Settings::show_virtual_git_portal`. Writes to real files
under `.git` are unaffected either way: they're the local volume's, and always were editable from a terminal.

**Toggle invalidates open listings.** Flipping the atomic alone isn't enough: panes already showing a `.git/...`
listing keep their cached children until the next navigation. So `set_show_virtual_git_portal` also calls
`wiring::refresh_all_virtual_listings_after_toggle`, which emits a `FullRefresh` for every cached listing that IS a
`.git` directory or sits inside one, on any volume.

**Gotcha**: that set comes from the LISTING CACHE, ❌ never from the watcher registry.
**Why**: a pane standing in `.git/` doesn't imply a `subscribe_git_state` for that repo, so the registry-derived
version left the pane the user was looking at showing six rows the portal no longer served, until they navigated away
and back (caught by `git-portal.spec.ts`, 2026-09-05). Over-selecting here costs a re-read and nothing else, which is
what makes a path-shape check the right instrument: it decides what to RE-READ, ❗ not what a mutation may touch. The
watcher-driven `wiring::refresh_virtual_listings` still goes through `refresh_local_listings_under` with real repo
roots, because there it HAS one.

A pane standing on a virtual path when the portal turns off gets `NotFound` from the parent volume (the directory isn't
on disk) and the pane's own recovery runs.

**Decision**: Typed `VolumeError::FriendlyGit(FriendlyGitError)` variant
**Why**: The portal volume's methods return `Result<_, VolumeError>` and the
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

**Decision**: A portal listing stays single-shot; cancellation via task abort + polled flag
**Why**: The `Volume::list_directory` contract is "compute Vec, return", and the portal honours it – no
`ListingEventSink` streaming here. Cancellation
works two ways: (1) the listing pipeline's `spawn_blocking` task can be
aborted on cancel, dropping the iterator; (2) we poll a per-process
`AtomicBool` (`log::cancel_flag()`) inside the rev-walk callback every
commit so a *cooperative* cancel takes effect within one commit decode
(microseconds). The flag is opt-in for tests and unused by production
listings (which rely on task abort). Changing to streaming would require
revisiting the trait contract everywhere.

**Decision**: Per-worktree HEAD watch registration on enumeration
**Why**: notify-debouncer-full doesn't natively glob, so
`<common-dir>/worktrees/*/HEAD` can't be expressed as a single watch. We
enumerate worktree gitdirs via `std::fs::read_dir(<common>/worktrees)`
at subscribe time and register one watch per existing `HEAD`. Worktrees
added later are picked up indirectly: `git worktree add` always touches
the main repo's `HEAD` too, which fires our existing main-HEAD watch
and drives another report. The cost is a few extra watcher
entries (typical worktree counts are 1-5) – negligible.

**Decision**: `Cat::browses_commit_tree()` covers branches/tags/commits/stash
**Why**: All four categories browse a commit tree, just resolved differently. Branches/tags peel through
refs, commits resolve a SHA prefix, stash expands `stash@{n}`, but the
*tree-walking* code path is identical. The method name describes
the contract. The dispatch lives in `mod.rs::resolve_commit_for_cat`.

**Decision**: Use `gix::Repository::status()` for `list_status` (not a `git status --porcelain=v2 -z` shell-out)
**Why**: In gix 0.81, `Repository::status().into_iter()` runs both a `TreeIndex` leg (HEAD vs index, for staged changes) and an `IndexWorktree` leg (index vs worktree, for unstaged changes) in parallel. The `TreeIndex` leg surfaces staged additions correctly even in single-commit repos. The gix approach is fully typed (no string parsing of stderr), which keeps us inside the no-error-string-match rule. Error mapping is entirely through typed enum variants. Don't reintroduce a shell-out: parsing `stderr.contains(...)` for error classification was the previous failure mode and it's banned.

**Decision**: repo discovery rejects bare repos via `BareRepo`
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

**Decision**: `RepoCache` holds a handle until the last unsubscribe, with no idle TTL
**Why**: Re-opening a `gix::Repository` is cheap (~10 ms on warm caches) but not free; the cache pins one handle per
active subscriber so back-to-back chip lookups skip the open. Eviction stays simple: an idle TTL would need a timer
thread for nearly no gain. The cache itself is a VALUE the `GitPortal` owns, ❌ not a static of its own (§ "`GitPortal`:
the value that owns the cache").

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

**Decision**: a virtual listing is unwatchable by TYPE, and `.git/` itself IS watched
**Why**: `listing/streaming.rs` arms a `notify` watch on any listing whose volume says it can carry one. A virtual path
has nothing on disk, so `notify` answers "No path was found" and spams the warn log; the portal volume returns
`can_watch_listings() == false`, which keeps every one of them out with no path check anywhere. Invalidation arrives
from the per-repo `.git/HEAD`, `refs/`, `packed-refs` watchers instead. `.git/` itself is a real directory on the local
volume, so it now IS watched, which is what makes an open `.git/` pane notice a new `MERGE_HEAD`. Two things ride on
that and are tested: a `FullRefresh` re-runs the overlays (else the six rows vanish from the pane), and the
fresh-listing oracle declines any overlay-decorated listing (else a delete walker gets the six rows).
