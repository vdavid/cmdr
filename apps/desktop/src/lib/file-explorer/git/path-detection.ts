/**
 * Shared helper for spotting paths inside the virtual `.git` portal.
 *
 * The backend exposes seven categories under `.git/`: `branches`, `tags`,
 * `commits`, `stash`, `worktrees`, `submodules`, and `raw`. Subpaths that
 * route through any of these are virtual (they don't exist on disk in the
 * shape Cmdr presents them). The `.git` directory itself (and other real
 * `.git/` internals like `HEAD` or `refs/heads/main`) stay real.
 *
 * The frontend uses this to skip filesystem-bound bookkeeping (the
 * "directory still exists" poll, future similar checks) on virtual paths.
 *
 * The seven category names mirror `Cat` in
 * `apps/desktop/src-tauri/src/file_system/git/path.rs`. Keep them in sync.
 */

const VIRTUAL_GIT_CATEGORIES = ['branches', 'tags', 'commits', 'stash', 'worktrees', 'submodules', 'raw'] as const

const VIRTUAL_GIT_PATH_REGEX = new RegExp(`/\\.git/(?:${VIRTUAL_GIT_CATEGORIES.join('|')})(?:/|$)`)

/**
 * Returns `true` when `path` lives under one of the seven virtual git
 * categories. Returns `false` for the `.git` root itself, for raw git
 * internals like `.git/HEAD` or `.git/refs/heads/main`, and for normal
 * non-git paths.
 */
export function isVirtualGitPath(path: string): boolean {
  return VIRTUAL_GIT_PATH_REGEX.test(path)
}
