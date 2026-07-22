# File explorer › git

Frontend git browser: the per-pane breadcrumb chip (`RepoChip.svelte`), the optional Git status column
(`status-column.ts`), the live `RepoInfo` store (`git-store.svelte.ts`), virtual `.git` portal path detection
(`path-detection.ts`), and the `redirectToPath` plumbing so worktree / submodule entries open their working dir
directly. Settings UI lives in `settings/sections/GitSection.svelte`.

Backend counterpart: `apps/desktop/src-tauri/src/file_system/git/CLAUDE.md` for repo discovery, the virtual `.git`
portal, the per-repo watcher, and the FriendlyError content.

## File map

- `RepoChip.svelte`: single-line pill: branch + ahead/behind/dirty in the header (passive indicator, not an action
  surface).
- `git-store.svelte.ts`: per-repo reactive `RepoInfo` map. `subscribeToRepo(repoRoot)` is the live channel (refcounted);
  `lookupRepoInfo(path)` is the cheap one-shot.
- `status-column.ts`: pure helpers `glyphFor`, `labelFor`, `fetchStatusMap` (no reactivity).
- `path-detection.ts`: `isVirtualGitPath(path)`, a shared regex matching the backend's virtual `.git` portal segments.

## Must-knows

- **The store is refcounted: one watcher per repo root, shared across panes.** `subscribeToRepo` / `unsubscribeFromRepo`
  keep the refcount; two panes on the same repo share one watcher and one subscription. Live updates flow through the
  `git-state-changed` Tauri event into reactive `$state`; the chip never polls. Don't replace this with per-pane stores
  (doubles watchers and IPC round-trips).
- **`FilePane`'s "directory still exists" poll evicts users back to `.git/` on virtual portal paths unless skipped.**
  `pathExists()` returns false for portal-only paths like `.git/branches/main/...`, and two false readings trigger
  `navigateToFallback`. The poll body early-returns via `isVirtualGitPath(currentPath)`. Keep that guard, and extend
  `path-detection.ts` if you add a portal segment.
- **Status column data uses paths RELATIVE to the repo root.** The `FullList.svelte` cell renderer must compute the
  relative path per entry before lookup; don't compare against the absolute path.
- **`fileExplorer.git.showVirtualGitPortal` round-trips through a Rust `AtomicBool`.** `settings-applier.ts` calls
  `setShowVirtualGitPortal(value)` (Tauri command `set_show_virtual_git_portal`, in `tauri-commands/settings.ts`), which
  flips an atomic consulted on every volume-hook entry, so toggling off stops the portal hijacking `.git` listings in
  real time.
- **Error/provider wording lives in Rust** (the FriendlyError pipeline), not in the frontend. Git failures land in
  `ErrorPane`; to change wording or add a state/provider, edit the Rust.
- **`FileEntry.redirectToPath`** is honored in `FilePane.svelte::handleNavigate`: when set, opening the entry navigates
  there directly instead of as a virtual subtree (used by `.git/worktrees/<name>` and `.git/submodules/<name>` → working
  dir). `FullList.svelte` shows an "Opens <path>" tooltip.

Settings keys (all `fileExplorer.git.*`): `showRepoChip` (default `true`), `showStatusColumn` (default `false`),
`showVirtualGitPortal` (default `true`).

Full details (chip lifecycle, status-column lifecycle, decision rationale): `DETAILS.md`.
