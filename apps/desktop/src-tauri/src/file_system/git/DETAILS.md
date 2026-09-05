# The app's git wiring: details

Pull-tier docs for `file_system/git/`: the two seams that reach `crates/cmdr-git`, and the decisions only the app can
make. Must-know invariants live in `CLAUDE.md`.

Everything that talks to a repository — discovery, `repo_info`, status, the portal volume, the `.git/*` watcher — is
`crates/cmdr-git/DETAILS.md`, and this document doesn't restate it. Frontend counterpart:
`apps/desktop/src/lib/file-explorer/git/CLAUDE.md` for the breadcrumb chip, status column, and the live `RepoInfo`
store.

**A repo's `.git/` listing mixes real entries (HEAD, config, hooks/, objects/, refs/, …) with the six virtual
categories so the user sees everything in one place.** The real half is the LOCAL volume's ordinary `read_dir`; the six
join it in the listing pipeline, through the overlay (§ "Two seams, no hooks").

## File map

Three files, and each is one of the app's answers:

- **`overlay.rs`** — the `ListingOverlay` contributor that puts the six category rows into a repo's `.git/` listing, for
  a PANE and nothing else (`src/listing_overlays.rs`). It asks the crate for the rows and does nothing else.
- **`wiring.rs`** — the parked `GitPortal` (`install_git_portal` at startup, `portal()` with a detached-sink fallback so
  a test binary that never runs `setup()` still browses), the toggle both seams consult, `volume_holds_real_repos`, the
  `GitStateChangedPayload` event and the sink that emits it, and the listing re-reads a repo change or a toggle drives.
- **`arming.rs`** — the `ListingLifecycle` observer (`crate::listing_lifecycle`) that keeps a repo's `.git/*` watcher
  armed while a pane is showing one of its virtual listings. § "Who arms the repo watcher".
- **`mod.rs`** — the module declarations and the `cmdr_git` re-exports the IPC commands import.

The route itself lives with the registry that owns it: `file_system/volume/manager/git_routing.rs`.

## A virtual listing is unwatchable by type, and `.git/` itself is watched

`listing/streaming.rs` arms a `notify` watch on any listing whose volume says it can carry one. A virtual path
has nothing on disk, so `notify` answers "No path was found" and spams the warn log; the portal volume returns
`can_watch_listings() == false`, which keeps every one of them out with no path check anywhere. Invalidation arrives
from the per-repo `.git/HEAD`, `refs/`, `packed-refs` watchers instead. `.git/` itself is a real directory on the local
volume, so it IS watched, which is what makes an open `.git/` pane notice a new `MERGE_HEAD`. Two things ride on
that and are tested: a `FullRefresh` re-runs the overlays (else the six rows vanish from the pane), and the
fresh-listing oracle declines any overlay-decorated listing (else a delete walker gets the six rows).

That FSEvents watch on `.git/` is non-recursive, so it sees a new direct child and nothing deeper. What keeps a `.git/`
pane's category-row COUNTS honest after a `git branch` is the per-repo watcher instead, which is why that pane arms one
of its own (§ "Who arms the repo watcher").

## Who arms the repo watcher

**The OPEN LISTINGS do, ❌ not the frontend's subscription.** `arming.rs` registers a `ListingLifecycle` observer; the
listing pipeline calls it when a listing enters the cache and when it leaves, and the observer takes and gives back one
subscriber on `wiring::portal()`. Two listing shapes arm: a path inside one of the six virtual trees, and the repo's own
`.git/` (its category rows carry live counts). The refcount is the watcher registry's own, so a working-tree pane and a
`.git/` pane on one repo cost one watcher between them.

**Why the backend owns it.** `src/lib/file-explorer/pane/git-browser-sync.svelte.ts` calls `subscribeGitState` only
while the breadcrumb chip or the status column is switched on, and only for the repo it looked up from the pane's path.
So three cases had nothing arming the watcher and sat on the refs as they were when the pane opened: a lone `branches/`
pane, a pane the MCP server drove, and any window with both git features off. Backend arming holds for all three and for
whatever asks next.

**The arm is detached, and the reconcile is not optional.** `arm_detached` hands the subscribe to
`tauri::async_runtime::spawn_blocking`, because arming is a repository open plus one FSEvents stream per watched
`.git/*` path and a listing open must not sit on a runtime worker for it. `list_directory_end` then runs the release
against a map the arm may not have written yet, so the arm re-checks listing-cache membership afterwards and gives the
subscriber straight back if the listing ended meanwhile. Same shape, same reason, as `watcher::start_watching_detached`.
❗ `list_directory_end` calls the observer AFTER removing the cache entry, which is what makes that membership check
answer honestly.

**What a change then re-reads**: `wiring::listings_a_repo_change_re_reads`. The six virtual trees by prefix, and the
repo's `.git/` itself by EXACT match. `.git/` is in the set because its category rows carry counts the overlay reads off
the repository, and its own FSEvents watch is non-recursive so a new `refs/heads/…` never touches it. It is not a prefix
because that would re-read `objects/` and `hooks/` panes on every commit.

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

## Two seams, no hooks

`LocalPosixVolume` names git nowhere: `rg 'git' backends/local_posix.rs` is empty, and ❌ it must stay that way. A
guard there once refused the REAL `.git/config` alongside the virtual folders, which stopped a repo-folder delete with
`.git/` still on disk. Two seams reach a repository instead, each with a rule a TYPE enforces rather than an `if`:

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

## Tauri commands

Wired from `commands/file_system/git.rs`:

- `get_git_repo_info(path) -> TimedOut<Option<RepoInfo>>` – one-shot lookup, 2 s timeout
- `subscribe_git_state(repo_root) -> Result<RepoInfo, GitSubscribeError>` – registers a subscriber, returns current `RepoInfo` synchronously, then emits `git-state-changed` events. 2 s timeout (the synchronous handshake discovers the repo and computes `repo_info` so a hung repo would otherwise freeze IPC). `GitSubscribeError` (in `commands/file_system/git.rs`) is `Git { error: FriendlyGitError }` / `TimedOut` / `Unexpected { detail }`, so git's own typed kind reaches the frontend intact and `src/lib/error-messages/git-error-messages.ts` words it per locale
- `unsubscribe_git_state(repo_root) -> ()` – drops one subscriber; tears down the watcher when refcount hits zero
- `get_git_status_for_paths(repo_root, dir) -> TimedOut<Vec<EntryStatus>>` – gix status walk, 5 s timeout
- `set_show_virtual_git_portal(enabled)` (in `commands::settings`) – flips the live portal toggle. Pushed by `settings-applier.ts` whenever `fileExplorer.git.showVirtualGitPortal` changes

## Decisions

**Decision**: `.git/` shows real entries and the six virtual ones together; no `raw/` escape hatch
**Why**: Hiding real `.git/*` contents behind a separate `raw/` category meant two extra clicks for anyone wanting to peek at `HEAD`, `config`, `hooks/`, or `objects/`. The virtual rows already cover the friendly view; showing the real entries in the same listing gives one-click access with no indirection, and costs nothing on the read side because the real half IS the local volume's ordinary listing. A real entry whose name collides with a virtual category gives way to it (the overlay's shadowing rule): the deprecated `.git/branches/` directory git stopped writing years ago, and `.git/worktrees/` in a linked-worktree setup, whose internals belong to git rather than to the user. The pane's own sort orders the merged rows; the six carry no sort privilege of their own.

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
