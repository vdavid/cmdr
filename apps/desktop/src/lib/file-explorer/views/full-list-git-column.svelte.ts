/**
 * The optional Git status column's data: the per-path status map for the directory
 * on screen, kept fresh by the repo watcher.
 *
 * The map is keyed by repo-relative path with forward slashes (what
 * `get_git_status_for_paths` returns), so `statusFor` computes the relative path per
 * row rather than storing absolute keys. That's what lets a directory with the repo
 * root in the MIDDLE of its path still resolve.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { onGitStateChanged } from '$lib/tauri-commands'
import { fetchStatusMap, type EntryStatusCode } from '../git/status-column'
import type { FileEntry } from '../types'

export interface GitStatusColumn {
  /**
   * Loads the status map for `repoRoot` / `dir` and re-loads it whenever the watcher
   * reports a change in that repo. Pass `repoRoot: null` when the column is off or
   * the path isn't in a worktree: the map clears and nothing is listened to.
   *
   * Returns a teardown. Call it from a Svelte `$effect` so a path or repo change
   * cancels the in-flight load before the next one starts.
   */
  watch: (repoRoot: string | null, dir: string) => () => void
  /**
   * The row's status code, or `null` when it's clean, outside the worktree, or the
   * map hasn't loaded yet.
   */
  statusFor: (file: FileEntry) => EntryStatusCode | null
}

export function createGitStatusColumn(): GitStatusColumn {
  /** Reactive map from path-relative-to-repo → status code. `null` while loading. */
  let statusMap = $state<Map<string, EntryStatusCode> | null>(null)
  /**
   * The repo the map is being kept for. Reactive so a repo switch repaints the
   * column right away instead of leaving the previous repo's glyphs on screen
   * until the new map lands.
   */
  let activeRepoRoot = $state<string | null>(null)

  return {
    watch: (repoRoot: string | null, dir: string) => {
      activeRepoRoot = repoRoot
      if (!repoRoot) {
        statusMap = null
        return () => {}
      }

      const repo = repoRoot
      let cancelled = false
      let unlisten: UnlistenFn | undefined

      async function load(): Promise<void> {
        const map = await fetchStatusMap(repo, dir).catch(() => null)
        if (!cancelled) statusMap = map
      }

      void load()
      void onGitStateChanged((payload) => {
        if (payload.repoRoot === repo) void load()
      }).then((fn) => {
        if (cancelled) fn()
        else unlisten = fn
      })

      return () => {
        cancelled = true
        unlisten?.()
      }
    },

    statusFor: (file: FileEntry) => {
      if (!statusMap || !activeRepoRoot) return null
      const root = activeRepoRoot.endsWith('/') ? activeRepoRoot : activeRepoRoot + '/'
      if (!file.path.startsWith(root)) return null
      return statusMap.get(file.path.slice(root.length)) ?? null
    },
  }
}
