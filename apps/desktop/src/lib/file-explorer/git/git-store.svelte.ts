/**
 * Reactive per-repo git state store.
 *
 * Each `FilePane` calls `subscribeToRepo(repoRoot)` once, gets an entry that
 * updates reactively as `git-state-changed` events arrive, and calls
 * `unsubscribe(repoRoot)` on unmount or path-off-repo.
 */
import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, type RepoInfo } from '$lib/ipc/bindings'
import { onGitStateChanged } from '$lib/tauri-commands'
import { throwIpcError } from '$lib/tauri-commands/ipc-types'

// Re-export the generated `RepoInfo` so existing importers (`RepoChip`, `FilePane`,
// tests) keep their `from './git-store.svelte'` import path.
export type { RepoInfo }

interface RepoEntry {
  refcount: number
  info: RepoInfo
}

/**
 * In-flight backend subscribes, keyed by `repoRoot`. A new subscribe registers
 * its promise here synchronously (before any `await`) so concurrent callers for
 * the same repo coalesce onto one `commands.subscribeGitState` round-trip
 * instead of each issuing their own — which would leave the backend refcount
 * one ahead of the FE map and leak the `.git` watcher for the session.
 */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; transient coalescing bookkeeping consumed only inside `subscribeToRepo`, never rendered.
const inflight = new Map<string, Promise<RepoInfo>>()

const repos = $state<Map<string, RepoEntry>>(new Map())
let unlisten: UnlistenFn | null = null

async function ensureListener(): Promise<void> {
  if (unlisten) return
  unlisten = await onGitStateChanged((payload) => {
    const { repoRoot, info } = payload
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
  // Already-subscribed repo: bump the refcount synchronously and we're done.
  const existing = repos.get(repoRoot)
  if (existing) {
    existing.refcount += 1
    return existing.info
  }

  // A concurrent caller already started subscribing this repo: join its
  // in-flight backend round-trip. We bump the refcount NOW (before awaiting),
  // because the racer who started it will write `{ refcount: 1 }` when it
  // resolves; our increment lands on top once that entry exists.
  const pending = inflight.get(repoRoot)
  if (pending) {
    const info = await pending
    const entry = repos.get(repoRoot)
    if (entry) entry.refcount += 1
    return info
  }

  // First subscriber for this repo. Register the in-flight promise
  // synchronously, before any `await`, so concurrent callers coalesce onto it.
  const subscribe = (async (): Promise<RepoInfo> => {
    await ensureListener()
    const res = await commands.subscribeGitState(repoRoot)
    if (res.status === 'error') throwIpcError(res.error)
    const info = res.data
    repos.set(repoRoot, { refcount: 1, info })
    return info
  })()
  inflight.set(repoRoot, subscribe)
  try {
    return await subscribe
  } finally {
    inflight.delete(repoRoot)
  }
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
