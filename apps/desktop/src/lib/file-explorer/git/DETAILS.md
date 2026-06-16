# File explorer › git — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the must-knows; this is the depth.

## Chip lifecycle

`FilePane.svelte` drives the chip:

1. On every `currentPath` change, call `syncGitState(path)`.
2. `syncGitState` runs `lookupRepoInfo(path)` (fast, one-shot).
3. If a new repo, subscribe via `subscribeToRepo(repoRoot)`. The store's refcount means two panes on the same repo share
   one watcher.
4. On unmount or path-off-repo, call `unsubscribeFromRepo(repoRoot)`.

Live updates flow through the `git-state-changed` Tauri event, which the store translates into reactive `$state`
mutations.

## Status-column lifecycle

`FullList.svelte` drives the optional column independently:

1. When `showGitColumn && gitRepoRoot`, it calls `fetchStatusMap(repoRoot, currentPath)` once on mount and on every
   `currentPath` / `cacheGeneration` change.
2. It also subscribes to `git-state-changed` for the active repo, refetching the map on every emission.
3. The column is omitted from `grid-template-columns` entirely when off, so the name column keeps every spare pixel.

## Decisions

- **Reactive store backed by a `Map<string, RepoEntry>`, not per-pane Svelte stores.** Two panes on the same repo would
  otherwise pay for two watcher subscriptions and two IPC round-trips. Refcounting makes backend tear-down deterministic
  without a per-pane dance.
- **`RepoChip` is a passive state indicator, not an action surface.** It shows branch + ahead/behind/dirty; action
  affordances live in the navigation flow and Settings, not crammed into a header pill.
- **`lookupRepoInfo` and `subscribeToRepo` are separate.** Lookup is cheap and runs on every path change; subscribe
  opens a watcher (a real commitment). Splitting them means rapid path changes across non-repo paths don't churn watcher
  state, and the chip can react to the lookup before the watcher is up.
- **Git status column sits right after Name, not after Modified.** The glyph reads as a per-row tag of the file, so it
  belongs next to the name. Putting it last would make the row scan name → metadata → meta-meta-tag.
- **The column is omitted from the grid when `gitRepoRoot` is null, even if enabled.** Outside a worktree it would show
  blank cells, costing ~28 px from the name column for no information gain. The setting means "show when meaningful."

## Gotcha detail

The virtual-path poll-skip exists because `pathExists(currentPath)` returns false for `.git/branches/main/...` (portal
paths exist only in the portal, not on disk); after two consecutive false readings the poll calls `navigateToFallback`.
Cache freshness for virtual listings flows through `git-state-changed` and the backend's `invalidate_virtual_listings`.
