# Git history & branches browser: plan

A read-only, file-manager-native way to browse git history and branches for any cloned repo. No new top-level volume, no
new mental model: the existing `.git` directory becomes a portal into refs, commits, stash, worktrees, and submodules.
Cross-volume copy already works, so dragging a file out of a commit tree into the working tree restores it for free.

This doc captures the **why** behind each decision so the implementing agent can adapt as reality pushes back. Don't
follow the steps blindly. When something doesn't fit, re-read the intent and pick the better path.

## Goal & user value

Five concrete moments a user gets:

1. They open a project and immediately see the branch + dirty state in the breadcrumb chip. Ambient context, zero
   actions.
2. They look at a directory inside a worktree and see at a glance which files are modified, untracked, ignored. A new
   column in Full mode.
3. They `cd` into `.git` (already a thing they can do today) and instead of seeing libgit internals, they see
   `branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`, `submodules/`, and a `raw/` escape hatch. Git-themed icons,
   distinct color in the breadcrumb so they know they're in "history-land."
4. They navigate `.git/branches/feature-x/src/foo.rs` exactly like a normal file. Preview, sort, ⌘C all work.
5. They put working tree on the right pane, `.git/branches/feature-x/` on the left, and copy a file across. They've just
   plucked a single file from another branch into the working tree, no `git checkout`, no danger.

That's v1. No diffs, no writes, no global "all repos" view, no blame. Read-only and refresh.

## Why this shape

- **Reuse `.git` as the portal.** The user already knows `.git` exists at the repo root. Reusing the path keeps the
  whole feature discoverable without adding a new "thing" to learn. The first time they wander in, they get virtual refs
  instead of `HEAD` + `objects/`. A `raw/` entry preserves access to the real innards for power users.
- **No new volume in the volume switcher.** The switcher is for storage destinations (drives, network shares, devices).
  Git history isn't a destination; it's a lens on the worktree they're already in.
- **Cross-volume copy as the file-pluck mechanism.** The copy engine already streams bytes between any two volumes.
  Modeling git blobs as a read-source means "drag from history pane to working pane" gives file-level pluck-from-ref for
  free. No new code paths in the copy engine. This is the whole punchline.
- **`gix` for reads, shell out to `git` only when gix lacks something cleanly.** Pure-Rust pulls keep the build clean
  and align with project direction. We accept a runtime dependency on the user's `git` binary: every developer with
  cloned repos has one. Each shell-out site is documented with the gix gap that justified it.
- **Subscribe, don't poll. Don't cache mutable state without watcher invalidation.** Static blob bytes (immutable by
  SHA) can be re-read on every request without a perf concern (gix is fast). Mutable state like ahead/behind/dirty is
  computed once per repo at portal entry and _re-emitted from the backend_ whenever the watcher detects a change to the
  relevant `.git/*` paths. The frontend never polls. There's no manual cache to keep coherent: data is read fresh on the
  read path, and re-emitted reactively on the watch path.
- **Streaming everywhere.** `git log`, status walks, tree listings: all run as async tasks emitting `listing-progress`
  events through the same pipeline as normal directory listings. Cancellation via the existing `AtomicBool` checked
  inside the iterator callback. Cancellation matters here because revwalks on a monorepo can run for seconds.
- **Lazy repo detection only.** Use `gix::discover` to walk up from the current path. No background scan, no global
  index. Cost of a walk-up is microseconds; cost of a wrong global index is user trust.

## Architecture

A new `src-tauri/src/file_system/git/` module owns all gix calls, all virtual-path parsing, and all friendly-error
mapping for git-specific failures.

```
file_system/git/
├── CLAUDE.md                module map, decisions, gotchas
├── mod.rs                   public API
├── repo.rs                  gix::discover, gitlink follow, ahead/behind/dirty, RepoHandle cache (shared, watcher-keyed)
├── path.rs                  parse `.git/{branches|tags|commits|stash|worktrees|submodules|raw}/...`
├── virtual_listing.rs       list virtual children at each level (root, ref-list, ref-tree)
├── tree.rs                  walk a commit's tree at a path
├── status.rs                per-entry status for a working-tree directory
├── log.rs                   streamed commit log (M3)
├── stash.rs                 stash entries (M3)
├── worktrees.rs             linked worktrees (M3)
├── submodules.rs            submodule entries (M3)
├── read_blob.rs             `VolumeReadStream` for git blobs
├── icons.rs                 register four git-themed icon IDs
├── watcher.rs               subscribe to .git mutable-state changes, emit `git-state-changed`
└── friendly.rs              git-specific FriendlyError mapping
```

### Volume hook contract

`LocalPosixVolume` gets thin hooks. The hook order is fixed and load-bearing:

1. `resolve(path)` runs first (existing). It normalizes absolute vs. relative paths against the volume root.
2. After `resolve`, before any disk call, each method calls `git::try_route_*(resolved_path)`. If it returns `Some`,
   that result is the volume method's return. Otherwise the existing real-FS path runs.

Three hook points:

- `list_directory`: `git::try_route_listing(resolved_path) -> Option<ListingResult>`.
- `get_metadata`: `git::try_route_metadata(resolved_path) -> Option<FileEntry>`.
- `open_read_stream`: `git::try_open_blob_stream(resolved_path) -> Option<Box<dyn VolumeReadStream>>`.

All mutation methods (`create_file`, `create_directory`, `delete`, `rename`, `write_from_stream`) detect a virtual git
path via `git::path::is_virtual(path)` and return `VolumeError::NotSupported` immediately. This is explicit, not
implicit: if a future write feature is added, it must update these methods, not bypass them.

`notify_mutation` for virtual paths is a no-op (early return). Mutations of git state happen out-of-band (the user runs
`git` in a terminal); they're surfaced through the `.git`-watcher pipeline below, not through `notify_mutation`.

### Watcher and live invalidation

`git::watcher` registers per-repo file watchers via the existing notify-rs infra:

- `<repo>/.git/HEAD`
- `<repo>/.git/ORIG_HEAD`
- `<repo>/.git/MERGE_HEAD`
- `<repo>/.git/FETCH_HEAD`
- `<repo>/.git/refs/` (recursive)
- `<repo>/.git/packed-refs`
- `<repo>/.git/index`
- `<repo>/.git/logs/HEAD`
- `<repo>/.git/worktrees/*/HEAD` (for linked worktrees)

On any change, the watcher:

1. Recomputes `RepoInfo` (branch, ahead, behind, dirty) and emits `git-state-changed` event for that repo. The frontend
   chip subscribes once per repo and updates reactively. No polling on `listing-complete`, no per-listing recompute.
2. Calls `notify_directory_changed(local_volume_id, parent_virtual_path, FullRefresh)` for every cached listing whose
   path is inside `<repo>/.git/{branches|tags|commits|stash|worktrees|submodules}/...`. The existing
   `find_listings_for_path_on_volume` helper finds them; we extend the prefix match to include the virtual root.
3. Debounces at 200 ms per repo (matches the existing watcher debounce in `file_system/listing/`).

This means every mutable view stays live. A `git fetch` updates ahead/behind. A `git commit` updates the dirty status,
the `commits/` listing, and any open `branches/<current>/` listing.

### Caching: what we cache, what we don't

- **`RepoHandle` (gix::Repository wrapper)** is cached per-`(repo_root)` in a small `RwLock<HashMap>`, keyed by
  canonical repo root path. Lifetime: opened lazily on first portal entry; pinned by any active subscriber (chip, open
  virtual listing, status query). Evicted only when subscriber count drops to zero AND the last unsubscribe was more
  than 5 minutes ago. This is a handle cache, not a data cache. gix re-reads refs/objects on each call.
- **`RepoInfo` (branch, ahead, behind, dirty)** is computed once per `git-state-changed` event and pushed to
  subscribers. The `subscribe_git_state` command returns the current `RepoInfo` synchronously on subscribe, so there's
  no race between subscription start and the first event.
- **No data caching of refs, trees, blobs, or status.** gix reads are fast; if they prove slow under real load, we
  benchmark and cache surgically with watcher-driven invalidation. Out of scope for v1.

### Path shape

| Category      | Path shape                                         | Tree at                      | Notes                                                                                |
| ------------- | -------------------------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------ |
| `branches/`   | `.git/branches/<branch>/...`                       | branch tip                   | Ref names with `/` rendered flat (the entry's display name shows the full ref name). |
| `tags/`       | `.git/tags/<tag>/...`                              | tagged commit                | Annotated tags resolve through their tag-object to the underlying commit.            |
| `commits/`    | `.git/commits/<short-or-full-sha>/...`             | that commit                  | v1 lists HEAD-reachable commits only.                                                |
| `stash/`      | `.git/stash/<n>/...`                               | stash's working-tree commit  | `n` matches `stash@{n}`. Empty if no stashes.                                        |
| `worktrees/`  | `.git/worktrees/<name>` redirects to worktree path | n/a                          | **Collides with the real `.git/worktrees/` dir in linked-worktree setups.** See raw. |
| `submodules/` | `.git/submodules/<name>` redirects to submodule    | n/a                          | Each submodule's working tree is itself a git portal.                                |
| `raw/`        | `.git/raw/...`                                     | real on-disk `.git` contents | Escape hatch. Includes the real `.git/worktrees/` and any other internals.           |

**`worktrees/` collision**: linked-worktree setups have a real `.git/worktrees/` directory. We override its listing with
the virtual one. Real internals stay accessible via `.git/raw/worktrees/`. Document this clearly in the chip tooltip and
in `git/CLAUDE.md`. It's the only place in the design where we shadow a real directory.

### Visual language

- **Breadcrumb chip** (right of the path bar, only shown when current path is inside a worktree): displays
  `main • +3 / -0 • dirty`. Click to copy current branch name. Tooltip expands to full status: "On branch `main`. 3
  ahead, 0 behind `origin/main`. 5 modified, 2 untracked." Detached HEAD shows `(detached)` on the chip and the short
  SHA in the tooltip.
- **Repo chip styling**: gray pill when clean, accent when ahead/behind, warning when dirty.
- **Status column** (Full mode only, opt-in): single-glyph per row: `M` modified, `A` added, `D` deleted, `?` untracked,
  `!` ignored, blank for clean. Column header reads `Git`. Each cell has an `aria-label` with the long form ("Modified",
  "Untracked", and so on) for screen readers, plus tooltip on hover. Hidden by default; toggled in Settings or via the
  column-config menu.
- **Breadcrumb segments inside `.git/...`**: rendered with a new dedicated `--color-git-portal` token (subtle accent
  variant, distinct from `--color-accent` to avoid the "alarming" feel). Hover/click behavior unchanged.
- **Git-themed icons** (four total, reused with category context):
  - `git:branch`: branches and `branches/` parent
  - `git:tag`: tags and `tags/` parent
  - `git:commit`: commits and `commits/` parent
  - `git:fork`: stash, worktrees, submodules, and the `raw/` parent (catch-all for less-common categories)
  - All sourced from Lucide where available (`git-branch`, `tag`, `git-commit-horizontal`, `git-fork`).

### Settings

All git settings live under the `fileExplorer.git.*` namespace for discoverability in the settings registry:

- `fileExplorer.git.showRepoChip` (default `true`)
- `fileExplorer.git.showStatusColumn` (default `false`; preserves the standard column layout out of the box)
- `fileExplorer.git.showVirtualGitPortal` (default `true`; toggling off makes `.git` browse like a normal directory)

### Cancellation pattern for streaming git ops

`git log` and `status` walks can run for seconds on monorepos. The pattern (matching the existing listing pipeline):

1. Backend spawns the gix iterator on a `tokio::task::spawn_blocking` (gix iterators are mostly sync).
2. The task holds an `Arc<AtomicBool>` cancel flag, polled inside the iterator's callback (every commit, every status
   entry).
3. The task emits `listing-progress` events in batches (every 200 ms or every N entries, whichever first).
4. On cancellation, the task drops the iterator and emits `listing-cancelled`.
5. The frontend already has the cancel hook for normal listings; reuses it.

`spawn_blocking` alone doesn't give cancellation; the polled flag is what does. Document this in `git/CLAUDE.md`.

### Performance budget

- **Repo open + `RepoInfo` first emit** ≤ 50 ms p95 on a 50k-file repo. Cold open includes ref enumeration. If gix is
  slower, bench and decide between gix optimization or shelling out.
- **Status walk for a single directory's entries** ≤ 100 ms p95 on a 50k-file repo. Status column must not feel laggy.
  Bench in M1 with a real fixture; if gix's `Repository::status()` exceeds budget, switch to
  `git status --porcelain=v2 --untracked-files=normal <dir>` shell-out from day one.
- **Tree listing at a ref** ≤ 30 ms p95 for a typical directory. Streaming kicks in for huge trees.

The benchmark step in M1 isn't optional. If gix doesn't hit budget, we change tools before building M2 on top of the
slow path.

## Edge cases & non-goals

### Edge cases we handle

- **`.git` as a file (gitlink)**: submodules and linked worktrees use this. `gix::discover` follows it.
- **Linked worktrees**: their `.git` is a file pointing into `<main-repo>/.git/worktrees/<name>`. Each worktree's portal
  works independently. The watcher list includes `<main-repo>/.git/worktrees/*/HEAD`.
- **Submodules**: their `.git` is a file pointing into `<parent>/.git/modules/<name>`. Portal works.
- **jj-on-top-of-git**: `.jj/` exists alongside `.git/`. We see only the git layer. Documented as expected behavior.
- **Detached HEAD**: chip shows `(detached)`, tooltip shows short SHA.
- **Empty repo (unborn HEAD)**: `gix::Repository::head()` returns `Reference::Symbolic` pointing at a non-existent ref.
  Chip shows `(no commits yet)`. `branches/`, `tags/`, `commits/` list as empty dirs.
- **`.git` exists but isn't a real repo** (someone ran `mkdir .git`): `gix::discover` errors. We surface a friendly
  message via `friendly.rs` rather than crashing. If `showVirtualGitPortal` is enabled, the portal shows a single "Not a
  git repo" entry with explanation.
- **Orphaned linked worktree** (main repo deleted): `gix::discover` errors trying to follow the gitlink. Same friendly
  error path.
- **Shallow clone**: log walk stops at the shallow boundary; surface a friendly note in `commits/<sha>/...` if the SHA
  isn't reachable.
- **Corrupt or missing objects**: `gix` errors map to `FriendlyError` with category `Serious`.
- **Non-UTF-8 paths and ref names**: gix returns `BString`. Lossy-convert with `to_string_lossy()` for display;
  round-trip via byte paths internally where possible.
- **Symlinks in trees**: gix exposes `EntryKind::Symlink`. Surface as a symlink `FileEntry` so existing UI handles it.
- **Executable bit on tree entries**: gix exposes the file mode. Map to `FileEntry.permissions` so cross-volume copy
  preserves it.

### Non-goals (v1)

- Bare repos (no working tree to anchor the experience).
- Native-jj or Sapling-only repos.
- Diff visualization.
- Any write operations, even "safe" ones like `mkdir branches/x = git branch x`. Validate the read-only model first.
- Global "all repos on this laptop" view.
- Blame, bisect, log search.
- LFS pointer dereferencing. v1 surfaces the pointer file as-is; future milestone can fetch.

### Anti-patterns

- **Pretending commits are mutable.** Commit trees are read-only. No write surface. Period.
- **Eagerly loading the full log.** Always stream. Always paginate. Always cancellable.
- **Caching mutable state without watcher invalidation.** RepoInfo is the watcher's job, not a cache TTL's job.
- **Coupling `LocalPosixVolume` to gix.** All git logic lives in `git/`. The volume calls a small interface.
- **Polling on `listing-complete`.** The chip subscribes once per repo via `git-state-changed`; it doesn't refresh on
  listing events.
- **The phrase "cherry-pick" in user-facing copy.** It means commit-level cherry-picking in git. What we do is "pluck a
  file from a ref." Use that phrase or similar in tooltips and docs.

## Schema additions (M1 commits these)

These small schema changes land in M1 because later milestones depend on them. They're cheap and inert if unused.

1. `FileEntry.iconId` already supports arbitrary string IDs. M1 adds the four `git:*` IDs to the icon registry.
2. `FileEntry` gets an optional `redirectToPath: Option<String>` field. When set on a virtual entry, the frontend
   navigates to `redirectToPath` instead of treating the entry as a normal directory. Default `None`. M3 uses this for
   worktrees and submodules; M1 ships the field so M3 doesn't have to ripple a schema change through every consumer
   (frontend list views, MCP `cmdr://state`, drag-drop, copy preview, Brief/Full renderers).
3. Tauri event `git-state-changed` with payload `{ repoRoot: String, info: RepoInfo }`.
4. New CSS token `--color-git-portal` in `apps/desktop/src/app.css` (light + dark variants). Distinct from
   `--color-accent`.

## Milestones

Each milestone is independently shippable. Each ends with: tests passing, full `./scripts/check.sh` clean, relevant
`CLAUDE.md` files updated, `CHANGELOG.md` entry added, one commit, no push.

The agent should run sequentially. There's no rush, and parallel work risks merge conflicts in shared files (Volume
trait, listing pipeline, FilePane).

### Milestone 1: Foundation: detection, chip, status column, schema

**Goal**: make every worktree-aware folder feel "git-aware" and ship the schema additions M2/M3 will rely on. Zero
virtual paths yet.

**Why first**: highest value-per-line, smallest blast radius. Validates `gix` performance budget, the
`blocking_with_timeout` pattern for git ops, and the watcher subscription model before we build on them.

**Deliverables**:

1. `gix` dep added to `apps/desktop/src-tauri/Cargo.toml`. Pin to a stable version ≥ 1 month old per project rules.
   Verify license compatibility via `cargo deny check`.
2. New module `src-tauri/src/file_system/git/` with:
   - `mod.rs` (public API)
   - `repo.rs`: `discover_repo(path)` (uses `gix::discover`, follows gitlinks), `repo_info(handle) -> RepoInfo` (branch
     or detached SHA, ahead/behind vs upstream, is_dirty, derived from gix). Handle cache as described above.
   - `status.rs`: `list_status(repo, dir_in_worktree) -> Vec<EntryStatus>`. Uses `gix::Repository::status()`. Benchmark
     step (see below). If gix exceeds budget, swap to porcelain v2 shell-out before merging.
   - `watcher.rs`: subscribe to the `.git` paths listed in the Architecture section. Debounce 200 ms. Recompute
     `RepoInfo`. Emit `git-state-changed`. Invalidate any open virtual-path listings (placeholder until M2; safe to
     wire).
   - `friendly.rs`: `NotARepo`, `OrphanedWorktree`, `CorruptRepo`, `IndexLocked`, `PermissionDenied`. Pass the
     `error_messages_never_contain_error_or_failed` test.
3. New Tauri commands in `commands/`:
   - `get_git_repo_info(path: String) -> Option<RepoInfo>`. Async, `blocking_with_timeout(2s)`.
   - `subscribe_git_state(repo_root: String) -> RepoInfo`. Registers the watcher + frontend subscription AND returns the
     current `RepoInfo` synchronously, so the chip never sees an empty interim state. Subsequent updates flow via
     `git-state-changed` events.
   - `unsubscribe_git_state(repo_root: String) -> ()`.
   - `get_git_status_for_paths(repo_root: String, dir: String) -> Vec<EntryStatus>`. Async, batch. For large walks it
     streams batches via `git-status-progress` (separate from `listing-progress` because the payload shape is per-path
     status, not `FileEntry`s; documented in `git/CLAUDE.md`).
4. Schema additions: `redirectToPath: Option<String>` on `FileEntry`, `--color-git-portal` CSS token, four `git:*` icon
   IDs registered. Inert until M2/M3.
5. Frontend:
   - New module `apps/desktop/src/lib/file-explorer/git/` with `RepoChip.svelte`, `git-store.svelte.ts` (per-repo
     reactive `RepoInfo`), `status-column.ts` (utilities).
   - `FilePane.svelte`: on volume/path change, derive repo root via the new command and subscribe via
     `subscribe_git_state`. Unsubscribe on unmount or path change off the repo. Repo info populates the chip.
   - Status column: optional column in `FullList.svelte`, gated by `fileExplorer.git.showStatusColumn` setting.
     Single-glyph cell with `aria-label` and tooltip.
   - Settings entry registered for the three settings keys.
6. Benchmark (mandatory before M2):
   - Build a 50k-file synthetic repo fixture (or use an existing real-world clone path supplied via env var).
   - Measure `discover_repo + repo_info` p95 and `list_status` p95.
   - If either exceeds budget, shell out from day one. Update `git/CLAUDE.md` decision log with the bench result.
7. Tests:
   - Unit: `discover_repo` against fixtures (real `.git` dir, gitlink, no-repo, bare-repo-rejected, empty-mkdir-only,
     orphaned worktree, unborn HEAD).
   - Unit: `repo_info` against fixtures with known branch + dirty state, detached HEAD, no upstream.
   - Unit: `status::list_status` with one of each status type.
   - Unit: `friendly.rs` covers each variant; `error_messages_never_contain_error_or_failed` passes.
   - Integration: watcher emits `git-state-changed` after a simulated `git commit` (write to `index` and
     `refs/heads/<branch>`).
   - Frontend: `RepoChip.svelte` snapshot tests for the six states (clean, ahead, behind, dirty, detached, unborn).
   - Accessibility: status column cells have `aria-label`s.
8. Docs:
   - New `src-tauri/src/file_system/git/CLAUDE.md`: module map, decisions (budget, gix-vs-shell-out outcome,
     `redirectToPath`, watcher path set), gotchas.
   - New `apps/desktop/src/lib/file-explorer/git/CLAUDE.md`.
   - Update `docs/architecture.md`: add the `git/` row in both backend and frontend tables.
   - Update `apps/desktop/src/lib/file-explorer/CLAUDE.md`: chip + status column.
   - `CHANGELOG.md` entry under `[Unreleased] / Added`.
9. Checks:
   - `./scripts/check.sh` clean.
   - `cargo deny check` clean for new dep.

**Risks / things to watch**:

- ahead/behind requires an upstream. If no upstream is set, omit those numbers from the chip.
- `gix::Repository::status()` needs care around untracked-cache configuration; benchmark before deciding on tool.
- Test fixtures: don't check in a 50k-file tarball (repo bloat). Add a `cargo xtask gen-git-fixture` (or a small Go
  script under `scripts/`) that builds the fixture deterministically into `target/test-fixtures/git/`. Tests that need
  it call the script first if the dir is missing. Document the script in `git/CLAUDE.md`.

### Milestone 2: Virtual `.git` portal: refs, tags, trees, blobs

**Goal**: `cd .git`, navigate `branches/<name>/...` and `tags/<name>/...`, preview files, copy them out via the existing
copy engine.

**Deliverables**:

1. `git/path.rs`:
   - `VirtualGitPath` enum: `Root`, `Category(Cat)`, `RefList(Cat)`, `Ref(Cat, name)`, `RefTree(Cat, name, sub)`,
     `Raw(sub)`.
   - `classify(path: &Path, repo: &RepoHandle) -> VirtualGitPath`.
   - Inverse: `to_path(VirtualGitPath, repo_root) -> PathBuf`.
   - `is_virtual(path) -> bool` for the volume hook short-circuits.
   - Tests for every shape, including ref names with slashes.
2. `git/virtual_listing.rs`:
   - `list_root(repo)`: returns `[branches, tags, commits, stash, worktrees, submodules, raw]`. M2 ships only
     `branches`, `tags`, and `raw` as navigable; `commits`, `stash`, `worktrees`, `submodules` are _omitted from the
     listing_ until M3 wires them. (No "Coming soon" stub entries.)
   - `list_branches(repo)`, `list_tags(repo)`: return refs as virtual dirs, streamed via the existing `listing-progress`
     event channel. Cancellable.
   - `list_raw(repo, sub)`: returns the real on-disk `.git/<sub>` contents. Implementation: just delegates back to
     `LocalPosixVolume::list_directory` on the resolved real path, but bypasses the git hook to avoid recursion.
3. `git/tree.rs`:
   - `list_tree(repo, commit_id, path) -> Vec<FileEntry>`: walks the commit's tree at the path. Sets correct sizes,
     `is_directory` flags, `permissions` (executable bit from file mode), and symlink flags.
   - `get_tree_entry(repo, commit_id, path) -> FileEntry`.
4. `git/read_blob.rs`:
   - `GitBlobReadStream`: implements `VolumeReadStream`. Implementation honestly: gix's `Object::data` returns `Vec<u8>`
     for the whole blob. The stream owns the `Vec<u8>` and yields slices in 256 KB chunks. Memory cost equals blob size;
     this is the gix constraint, not a streaming win. Documented in `git/CLAUDE.md`. For blobs larger than
     `git.maxBlobBytes` (default 256 MB), the stream returns a `FriendlyError::BlobTooLarge` rather than OOM. Future
     work: gix may expose a chunked read for loose objects; revisit then.
5. `git/icons.rs`: register the four icon IDs.
6. Hook `LocalPosixVolume`:
   - `list_directory`, `get_metadata`, `open_read_stream`: each calls the corresponding `git::try_route_*` after
     `resolve()` and returns early if `Some`.
   - All mutation methods detect virtual paths via `git::path::is_virtual` and return `NotSupported`.
   - `notify_mutation` early-returns for virtual paths.
7. Frontend:
   - Breadcrumb segments inside `.git/...` rendered with `--color-git-portal`. Detection: segment-by-segment scan after
     the repo root.
   - Confirm git icons render correctly in Brief and Full modes.
8. Tests:
   - Unit: every `VirtualGitPath` variant parser/inverse, including round-trip and ref names with `/`.
   - Unit: `list_branches` against a fixture with multiple branches including nested-name (`feature/foo`).
   - Unit: `list_tree` at root, at sub-paths, with binary blobs, with symlinks.
   - Integration: navigate `.git/branches/main/`, list, read a file. Assert content matches `git show main:<path>`.
   - **Cross-volume copy E2E**: copy `.git/branches/main/scripts/run.sh` (executable) to a tmp dir. Assert byte-equal to
     `git show` output AND that the executable bit is preserved on the destination. This is the punchline: file-level
     pluck-from-ref via drag-drop must round-trip metadata.
   - Listing cancellation: long ref enumeration is cancellable mid-stream.
   - Watcher integration: simulate `git branch new-branch HEAD`, assert open `branches/` listing receives a
     `directory-diff` event.
9. Docs:
   - Update `git/CLAUDE.md` with the volume hook contract (post-resolve order), blob streaming honesty section, and
     ref-name flat-rendering decision.
   - Update `file_system/volume/CLAUDE.md` (LocalPosix entry) to note the git delegation hooks.
   - `CHANGELOG.md` entry.
   - `docs/architecture.md` mention.

**Risks / things to watch**:

- Repo with thousands of branches: gix `references().all()` is iterator-based; confirm cancellation polls inside the
  loop.
- Symlinks in trees: gix represents target as blob content. Surface as `FileEntry` with `isSymlink: true`.
- Watcher must invalidate virtual listings cleanly. Test with both panes open on the same repo's `branches/`.

### Milestone 3: Commits, stash, worktrees, submodules

**Goal**: fill in the four remaining categories. Same pattern as M2 but with category-specific quirks.

**Deliverables**:

1. `git/log.rs`: streamed commit log.
   - `list_commits(repo, opts) -> stream of CommitEntry`. v1 listing is HEAD-reachable only. **Direct path entry to any
     commit SHA still works**: `.git/commits/<full-or-short-sha>/...` resolves to that commit's tree even if it isn't in
     the listing (useful for typed-in SHAs and unreachable commits in shallow clones).
   - Each commit becomes a virtual dir named with the short SHA. Display name includes `<short-sha> <subject>`, dates
     populate `addedAt`/`createdAt` so sort-by-date works.
   - Pagination: emit batches of 200 commits, allow cancellation, cap at 5000 entries with a friendly "Load more" entry
     at the bottom (frontend handles that on Enter).
   - `commits/<sha>/` reuses M2's `tree.rs`.
2. `git/stash.rs`:
   - `list_stashes(repo) -> Vec<StashEntry>`. v1: shells out to `git stash list --format=%H %gd %s` if gix doesn't
     expose a clean stash list (decision documented in CLAUDE.md after gix recon).
   - `stash/<n>/...` browses that stash's working-tree commit.
3. `git/worktrees.rs`:
   - `list_worktrees(repo) -> Vec<WorktreeEntry>`. Each entry sets `redirectToPath` to the worktree's working dir.
4. `git/submodules.rs`:
   - `list_submodules(repo) -> Vec<SubmoduleEntry>`. Same `redirectToPath` pattern.
5. Frontend:
   - Handle `redirectToPath`: when present, navigation goes there instead of treating as a directory listing. Tooltip:
     "Opens worktree at <path>" / "Opens submodule…".
6. Tests:
   - Fixture with 1000+ commits to validate streaming and cancellation.
   - Fixture with 3 stashes.
   - Fixture with a linked worktree + a submodule.
   - Redirect navigation E2E.
7. Docs: extend `git/CLAUDE.md` and `CHANGELOG.md`.

**Risks / things to watch**:

- gix's revwalk may not be `Send`-friendly. Confirm; if not, use `spawn_blocking` and a channel.
- gix stash API maturity. Document the gix recon outcome.
- Linked worktree's `.git/worktrees/<name>/HEAD` watcher subscription needs to update when worktrees are added.

### Milestone 4: Polish: settings UI, friendly errors, E2E

**Goal**: ship-ready quality. Settings panel UI, FriendlyError integration end-to-end, full E2E coverage.

**Deliverables**:

1. Settings UI:
   - "Git" section in Settings with the three toggles. Wire to settings registry. Reactive in frontend.
   - Tooltips explaining each setting in plain language.
2. FriendlyError integration:
   - `friendly.rs` covers all known git failure modes (M1's set plus shallow boundary, blob-too-large, missing object,
     gitdir-permission-denied, locked index during status walk).
   - Test the existing `error_messages_never_contain_error_or_failed` rule passes.
3. Tooltips and help:
   - Hover on the repo chip shows full status. Localized strings, sentence case.
   - Hover on a status-column glyph shows the long form.
4. E2E (Playwright):
   - Test fixture: a small bare-bones repo committed to `apps/desktop/test/e2e-playwright/fixtures/git-repo/` (or init
     at runtime via test setup).
   - Tests: nav into `.git`, see virtual entries; nav into branches, see refs; nav into a ref, see tree; copy a file
     from history pane to working tree; verify the copied file matches `git show` AND preserves executable bit.
5. CLAUDE.md updates everywhere git landed: `file_system/git/`, `file-explorer/git/`, `file_system/volume/`,
   `file-explorer/`, `architecture.md`.
6. `CHANGELOG.md` entry summarizing the feature for users.
7. Final full check: `./scripts/check.sh`. Then E2E suite.

## Open questions

Flagged for the implementing agent to surface or decide as context arrives. Not blockers.

1. Where exactly does the breadcrumb chip sit visually? Find the existing breadcrumb component, decide between trailing
   pill or inline-with-path. Whatever fits the existing visual hierarchy.
2. Status column placement: between Name and Size, or after Modified? Intuition: right after Name. Settle via visual
   review during M1.
3. Does watching `.git/worktrees/*/HEAD` need glob support in the existing watcher infra, or do we register one watcher
   per linked worktree on enumeration? Decide during M1's watcher implementation.
4. `git stash` API in gix: clean enough, or shell out? Decide in M3 after recon.

## Future work (post-MVP)

For context. Don't pre-architect for these.

- Log search (`/` inside `commits/`)
- Diff visualization
- Write gestures (mkdir-as-branch-create, rm-as-branch-delete) with carefully-bounded blast radius
- Blame on focused file
- LFS pointer dereferencing
- Bisect helper
- Native-jj / Sapling-only repo support
- Larger-blob streaming when gix exposes a chunked loose-object reader

## Workflow expectations for the implementing agent

- Work milestones in order. Don't skip ahead.
- After each milestone: full `./scripts/check.sh` clean, all relevant `CLAUDE.md` updated, `CHANGELOG.md` entry, one
  commit, no push.
- Commits follow project convention (`.claude/rules/git-conventions.md`): prefix like `Git browser:`, max 50-char title,
  no co-author.
- If a decision changes mid-milestone, update this plan in-place and commit the plan change before continuing.
- Use subagents (Explore for codebase questions, Plan for re-planning, voltagent reviewers for fresh-eyes review) when
  blast radius gets big. Don't blindly delegate trivial work.
- `gix` first; shell out to `git` only when gix lacks something cleanly. Document each shell-out in `git/CLAUDE.md`.
- Every git-touching IPC command: async, `blocking_with_timeout`, cancellable.
- The user is AFK. Do not block on user input. If a decision is genuinely ambiguous, pick the more conservative option,
  document it in the plan, and continue.
- Test the running app via the MCP servers when behavior is best validated end-to-end. See `docs/tooling/mcp.md`.
