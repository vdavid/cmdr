/**
 * Reactive per-repo git state store.
 *
 * Each `FilePane` calls `subscribeToRepo(repoRoot)` once, gets an entry that
 * updates reactively as `git-state-changed` events arrive, and calls
 * `unsubscribe(repoRoot)` on unmount or path-off-repo.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from '$lib/tauri-commands/ipc-types'

export interface RepoInfo {
  repoRoot: string
  branch: string | null
  detachedSha: string | null
  unborn: boolean
  upstream: string | null
  ahead: number | null
  behind: number | null
  isDirty: boolean
}

interface RepoEntry {
  refcount: number
  info: RepoInfo
}

interface GitStateChangedPayload {
  repoRoot: string
  info: RepoInfo
}

const repos = $state<Map<string, RepoEntry>>(new Map())
let unlisten: UnlistenFn | null = null

async function ensureListener(): Promise<void> {
  if (unlisten) return
  unlisten = await listen<GitStateChangedPayload>('git-state-changed', (event) => {
    const { repoRoot, info } = event.payload
    const entry = repos.get(repoRoot)
    if (entry) {
      entry.info = info
      // Trigger reactivity by replacing the entry.
      repos.set(repoRoot, { ...entry })
    }
  })
}

/**
 * Subscribes to live updates for the repo at `repoRoot`. Returns the current
 * `RepoInfo` synchronously so the chip never sees a flash of empty state.
 */
export async function subscribeToRepo(repoRoot: string): Promise<RepoInfo> {
  await ensureListener()

  const existing = repos.get(repoRoot)
  if (existing) {
    existing.refcount += 1
    return existing.info
  }

  const res = await commands.subscribeGitState(repoRoot)
  if (res.status === 'error') throwIpcError(res.error)
  const info = res.data
  repos.set(repoRoot, { refcount: 1, info })
  return info
}

/**
 * Drops one subscriber for the repo. The watcher tears down on the last
 * unsubscribe.
 */
export async function unsubscribeFromRepo(repoRoot: string): Promise<void> {
  const entry = repos.get(repoRoot)
  if (!entry) return
  entry.refcount -= 1
  if (entry.refcount <= 0) {
    repos.delete(repoRoot)
    await commands.unsubscribeGitState(repoRoot)
  }
}

/**
 * Reads the current `RepoInfo` for the repo. Used by `RepoChip.svelte` and
 * `status-column.ts` to render reactively.
 */
export function getRepoInfo(repoRoot: string): RepoInfo | null {
  return repos.get(repoRoot)?.info ?? null
}

/**
 * One-shot, no-subscription lookup. Returns `null` if the path isn't inside
 * a git repo or the lookup timed out.
 */
export async function lookupRepoInfo(path: string): Promise<RepoInfo | null> {
  const result = await commands.getGitRepoInfo(path)
  return result.data
}
