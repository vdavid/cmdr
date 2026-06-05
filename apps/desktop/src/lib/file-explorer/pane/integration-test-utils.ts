/**
 * Shared helpers for FilePane / VolumeBreadcrumb / selection integration tests.
 *
 * NOTE: vi.mock() calls must remain in each test file. Vitest hoists them and
 * they don't work when imported from a shared module. Only non-mock helpers
 * (waitForUpdates, useMountTarget, the listen capture-and-replay helper) are
 * shared here.
 */
import { vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'

// Helper to wait for async updates
export async function waitForUpdates(ms = 50): Promise<void> {
  await tick()
  await new Promise((r) => setTimeout(r, ms))
  await tick()
}

// Mock scrollIntoView which isn't available in jsdom
Element.prototype.scrollIntoView = vi.fn()

/** Standard beforeEach/afterEach for mounting tests with a target div. */
export function useMountTarget(): { getTarget: () => HTMLDivElement } {
  let target: HTMLDivElement

  beforeEach(() => {
    vi.clearAllMocks()
    target = document.createElement('div')
    document.body.appendChild(target)
  })

  afterEach(() => {
    target.remove()
  })

  return {
    getTarget: () => target,
  }
}

/**
 * Capture-and-replay `listen` mock — the navigation-transaction seam (a).
 *
 * The default `listen` mock in the pane integration tests is a no-op
 * (`vi.fn(() => Promise.resolve(() => {}))`): a mounted `DualPaneExplorer` /
 * `FilePane` registers its listing-event listeners but they can never be fired,
 * so a synthetic `listing-complete` / `listing-error` can't drive the coordinator
 * navigation braid (`handlePathChange` / `handleVolumeChange`, reached via
 * `FilePane.onPathChange` / `onVolumeChange`). This recorder replaces that no-op:
 * it records every registered callback keyed by event name and exposes
 * `fireListingEvent(eventName, payload)` to invoke them with the Tauri
 * `{ payload }` event shape the listeners read.
 *
 * Staleness is the system-under-test's job, not the helper's: `FilePane` gates
 * each listener on `event.payload.listingId === thisListingId && thisGeneration
 * === loadGeneration` and tears down the previous load's listeners via the
 * `unlisten` it returns. The recorder honors that teardown — invoking the
 * returned `unlisten` drops the callback — so firing an event reaches only the
 * live listeners, exactly as the real `@tauri-apps/api/event` would. A test that
 * wants to fire a *stale* completion captures the old listing's id (from the
 * `listDirectoryStart` mock call), navigates the pane elsewhere, then fires with
 * that old id; the live listener's id-gate drops it, which is the behavior being
 * pinned.
 *
 * Usage in a test file (the `vi.mock` must stay in the test file — vitest hoists
 * it and it can't live in this shared module):
 *
 *   const events = createListenRecorder()
 *   vi.mock('$lib/tauri-commands', async (orig) => ({
 *     ...(await orig()),
 *     listen: events.listen,
 *   }))
 *   // … mount, capture the minted listingId from the listDirectoryStart mock …
 *   events.fireListingEvent('listing-complete', { listingId, totalCount: 3, volumeRoot: '/' })
 */
export interface ListenRecorder {
  /** Drop-in for the `$lib/tauri-commands` `listen` export. */
  listen: (eventName: string, callback: (event: { payload: unknown }) => void) => Promise<() => void>
  /** Invoke every live listener registered for `eventName` with `{ payload }`. */
  fireListingEvent: (eventName: string, payload: unknown) => void
  /** Number of live listeners currently registered for `eventName` (for the helper's own smoke test). */
  listenerCount: (eventName: string) => number
  /** Forget all recorded listeners (call in `beforeEach`). */
  reset: () => void
}

export function createListenRecorder(): ListenRecorder {
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>()

  function listen(eventName: string, callback: (event: { payload: unknown }) => void): Promise<() => void> {
    let set = listeners.get(eventName)
    if (!set) {
      set = new Set()
      listeners.set(eventName, set)
    }
    set.add(callback)
    return Promise.resolve(() => {
      listeners.get(eventName)?.delete(callback)
    })
  }

  function fireListingEvent(eventName: string, payload: unknown): void {
    const set = listeners.get(eventName)
    if (!set) return
    // Copy before iterating: a listener may unlisten (mutating the set) while firing.
    for (const cb of [...set]) {
      cb({ payload })
    }
  }

  function listenerCount(eventName: string): number {
    return listeners.get(eventName)?.size ?? 0
  }

  function reset(): void {
    listeners.clear()
  }

  return { listen: vi.fn(listen), fireListingEvent, listenerCount, reset }
}
