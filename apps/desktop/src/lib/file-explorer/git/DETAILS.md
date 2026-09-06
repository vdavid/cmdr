# File explorer › git — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the
must-knows; this is the depth.

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
- **A portal pane's capability row is derived LEXICALLY on this side, not published by the backend.** The routed
  `GitPortalVolume` id never enters FE state (the tab keeps the parent drive's id, as for archives), so there's no
  `VolumeInfo` to read `backendCanWrite` off. `capabilitiesForPane` therefore reruns the backend's own routing
  predicate: `isVirtualGitPath` plus the live `showVirtualGitPortal` toggle. Keeping the toggle in the predicate is what
  makes the two agree in both directions — with the portal off nothing is routed, so a real `.git/branches/` directory
  on disk must stay writable.
- **The toggle reaches the predicate through `reactive-settings.svelte`, ❌ never `getSetting`.** The pane's `caps` is a
  `$derived`, and `getSetting` reads a plain Map, so a direct read would leave a portal pane stuck on its old row until
  the next navigation. `getShowVirtualGitPortal()` is a `$state` slot fed by the same `onSettingChange` subscription
  every other live setting uses.
- **The portal borrows the archive kind's shape rather than inventing a third pattern.** Read-only where the archive row
  is writable, identical everywhere else (real listing, `..` row, MCP sync, no own tint, no breadcrumb special case).
  The per-cell rationale and the guard sites live with the archive ones in `../pane/DETAILS.md` § "The virtual `.git`
  portal pane", so the two routed kinds stay legible side by side.

## Gotcha detail

The virtual-path poll-skip exists because `pathExists(currentPath)` returns false for `.git/branches/main/...` (portal
paths exist only in the portal, not on disk); after two consecutive false readings the poll calls `navigateToFallback`.
Cache freshness for virtual listings flows through `git-state-changed` and the backend's `invalidate_virtual_listings`.
