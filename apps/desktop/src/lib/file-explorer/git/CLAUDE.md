# File explorer › git

Frontend module for the git browser. Renders the per-pane breadcrumb chip (`RepoChip.svelte`), the optional Git status
column (`status-column.ts`), the live `RepoInfo` store (`git-store.svelte.ts`), the virtual `.git` portal path detection
(`path-detection.ts`), and the `redirectToPath` plumbing so worktree / submodule entries open their working dir
directly. **Settings > General > Git** has three live toggles (`GitSection.svelte`); `showVirtualGitPortal` round-trips
through a Rust atomic that disables the backend portal hook in real time. Git failures land in `ErrorPane` with warm
copy via the FriendlyError pipeline.

Backend counterpart:
[`apps/desktop/src-tauri/src/file_system/git/CLAUDE.md`](../../../../src-tauri/src/file_system/git/CLAUDE.md) for repo
discovery, the virtual `.git` portal, the per-repo watcher, and the FriendlyError content for git failures.

## File map

- **`RepoChip.svelte`**: Single-line pill rendering branch + ahead/behind/dirty in the header
- **`RepoChip.test.ts`**: Snapshot-style tests for the six visual states
- **`git-store.svelte.ts`**: Per-repo reactive `RepoInfo` map. `subscribeToRepo(repoRoot)` is the live channel;
  `lookupRepoInfo(path)` is one-shot
- **`status-column.ts`**: Pure helpers: `glyphFor`, `labelFor`, `fetchStatusMap`. No reactivity
- **`status-column.test.ts`**: Tests for `glyphFor`, `labelFor`, and `fetchStatusMap` (mocks the IPC envelope)
- **`git-store.test.ts`**: Tests for refcounted subscribe/unsubscribe and `lookupRepoInfo` envelope unwrapping
- **`path-detection.ts`**: `isVirtualGitPath(path)`: shared regex matching the seven backend `Cat` segments. Used to
  skip filesystem-bound polls (and future similar checks) on virtual portal paths
- **`path-detection.test.ts`**: Tests for `isVirtualGitPath` (each category, raw passthrough, real `.git` internals,
  lookalikes)

## Lifecycle

`FilePane.svelte` drives the chip:

1. On every `currentPath` change, call `syncGitState(path)`.
2. `syncGitState` runs `lookupRepoInfo(path)` (fast, one-shot).
3. If a new repo, subscribe via `subscribeToRepo(repoRoot)`. The store keeps a refcount, so two panes on the same repo
   share one watcher.
4. On unmount or path-off-repo, call `unsubscribeFromRepo(repoRoot)`.

Live updates flow through the `git-state-changed` Tauri event, which the store translates into reactive `$state`
mutations. The chip never polls.

`FullList.svelte` drives the optional status column independently:

1. When `showGitColumn && gitRepoRoot`, it calls `fetchStatusMap(repoRoot, currentPath)` once on mount and on every
   `currentPath` / `cacheGeneration` change.
2. It also subscribes to `git-state-changed` for the active repo, refetching the map on every emission.
3. The column is omitted from `grid-template-columns` entirely when off, so the name column keeps every spare pixel for
   non-git folders.

## Settings

Three keys, all under `fileExplorer.git.*`:

- `fileExplorer.git.showRepoChip` (default `true`): gates the chip render.
- `fileExplorer.git.showStatusColumn` (default `false`): gates the optional status column in Full mode.
- `fileExplorer.git.showVirtualGitPortal` (default `true`): controls whether `cd .git` shows the virtual portal.
  `settings-applier.ts` calls `setShowVirtualGitPortal(value)` (Tauri command `set_show_virtual_git_portal`), which
  flips a Rust `AtomicBool` consulted on every volume-hook entry. Toggling off makes the portal stop hijacking `.git`
  listings immediately. **Settings > General > Git > GitSection.svelte** wires the UI; `setShowVirtualGitPortal` lives
  in `tauri-commands/settings.ts`.

## Decisions

**Decision**: Reactive store backed by a `Map<string, RepoEntry>` instead of a per-pane Svelte store **Why**: Two panes
on the same repo would otherwise pay for two watcher subscriptions and two IPC round-trips. Refcounting in the store
makes the backend tear-down deterministic without per-pane dance.

**Decision**: `RepoChip.svelte` is a passive state indicator, not an action surface **Why**: The chip shows branch +
ahead/behind/dirty status. Action affordances (navigate to `.git/...`, branch operations) live in the navigation flow
and Settings rather than crammed into a header pill.

**Decision**: `lookupRepoInfo` and `subscribeToRepo` are separate calls **Why**: Lookup is cheap and runs on every path
change; subscribe is a real commitment (opens a watcher). Splitting them means rapid path changes across non-repo paths
don't churn watcher state, and the chip can react to the lookup before the watcher is up.

**Decision**: Place the Git status column right after Name, not after Modified **Why**: The glyph reads as a per-row tag
of the file, so it sits naturally next to the name. Putting it last would make the row scan name → metadata →
meta-meta-tag, which is one indirection too many.

**Decision**: The Git column is omitted from the grid when `gitRepoRoot` is null, even if the user enabled the setting
**Why**: Outside a worktree the column would just show blank cells, which costs ~28 px from the name column for no
information gain. We treat the user setting as "show when meaningful" rather than "always reserve space."

## Gotchas

**Gotcha**: `FilePane`'s "directory still exists" poll evicts users back to `.git/` on virtual paths if not skipped.
**Why**: `pathExists(currentPath)` returns false for `.git/branches/main/...` because those paths only exist in the
portal, not on disk. After two consecutive false readings the poll calls `navigateToFallback`. The poll body now early-
returns via `isVirtualGitPath(currentPath)` so virtual paths stay put. Cache freshness for virtual listings already
flows through `git-state-changed` and the backend's `invalidate_virtual_listings`.

## Redirect navigation

`FileEntry.redirectToPath` is honoured in `FilePane.svelte::handleNavigate`. When set, opening the entry navigates to
that path directly instead of treating it as a virtual subtree. Used today by:

- `.git/worktrees/<name>` → linked worktree's working dir.
- `.git/submodules/<name>` → submodule's working dir.

`FullList.svelte` shows a tooltip "Opens &lt;path&gt;" for these entries so users know they're about to navigate
elsewhere.

**Gotcha**: Status column data uses _relative_ paths (relative to the repo root). The `FullList.svelte` cell renderer
needs to compute the relative path for each entry before lookup; don't compare against the absolute path.
