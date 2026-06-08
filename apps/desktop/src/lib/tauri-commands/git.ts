// Git browser event listeners

import { type UnlistenFn } from '@tauri-apps/api/event'
import { events, type GitStateChangedPayload } from '$lib/ipc/bindings'

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
