// Git browser commands and event listeners

import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events, type EntryStatus, type GitStateChangedPayload, type RepoInfo } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'
import type { TimedOut } from './ipc-types'

/**
 * Returns the repo info for any path inside a worktree, or `null` if there's
 * no repo above it. The one-shot variant; `subscribeGitState` is the live channel.
 */
export function getGitRepoInfo(path: string): Promise<TimedOut<RepoInfo | null>> {
  return commands.getGitRepoInfo(path)
}

/**
 * Subscribes a frontend pane to live `git-state-changed` events for the repo
 * at `repoRoot`. Returns the current `RepoInfo` synchronously so the chip
 * never sees an empty interim state.
 */
export async function subscribeGitState(repoRoot: string): Promise<RepoInfo> {
  const res = await commands.subscribeGitState(repoRoot)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Drops one subscriber for the repo. The watcher itself stays alive until the last subscriber unsubscribes. */
export function unsubscribeGitState(repoRoot: string): Promise<void> {
  return commands.unsubscribeGitState(repoRoot)
}

/** Returns the per-entry status for a worktree, scoped by `dir`. */
export function getGitStatusForPaths(repoRoot: string, dir: string): Promise<TimedOut<EntryStatus[]>> {
  return commands.getGitStatusForPaths(repoRoot, dir)
}

/**
 * Subscribes to live `git-state-changed` events emitted by the per-repo `.git/*`
 * watcher. The payload carries the repo root and a fresh `RepoInfo` snapshot.
 * The git store and the Full mode status column both listen and filter by repo root.
 */
export function onGitStateChanged(handler: (payload: GitStateChangedPayload) => void): Promise<UnlistenFn> {
  return events.gitStateChanged.listen((event) => {
    handler(event.payload)
  })
}
