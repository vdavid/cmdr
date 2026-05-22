/**
 * Frontend-only in-memory store of search-result snapshots with refcounting.
 *
 * See search-redesign-plan.md §3.7. Each pane history entry whose `path` starts with
 * `search-results://` holds a +1 ref on the snapshot. The dialog's "last attempt" slot
 * (the most-recent dialog search the user ran) also holds a +1 ref. When a snapshot's
 * refcount drops to 0, the entry is deleted from the store. Refcount is the only
 * authority; there's no hard cap on the store itself.
 *
 * M8a verification — TabState ownership of history (plan §3.7 risk register):
 * `TabState.history: NavigationHistory` lives in `tab-types.ts` (a per-tab field on
 * `TabManager.tabs`). The dual-pane explorer's `{#key activeTabId}` block recreates
 * `FilePane` on tab switch, but the parent `DualPaneExplorer` owns the `TabManager`
 * and only reads/writes `tab.history` via the `setPaneHistory` accessor. The pane
 * recreation never touches history — it just re-renders against the surviving
 * `TabState`. Verified by reading `pane/DualPaneExplorer.svelte`'s `getPaneHistory` /
 * `setPaneHistory` helpers (defined around line 311–335) and
 * `tabs/tab-state-manager.svelte.ts::switchTab` (which only touches `cursorFilename`
 * and `activeTabId`, never `history`). Net effect: snapshot refs survive `{#key}`
 * re-creation because history isn't owned by the pane. M8b can rely on this without
 * re-verifying.
 *
 * The store is pure module state (not a `$state` reactive store) because consumers
 * read snapshots imperatively at render time — there's no DOM that should re-render
 * when the map changes. If a future consumer needs reactivity, wrap `getSnapshot`
 * results in a `$derived` at the call site.
 */

import type { SearchResultEntry } from '$lib/ipc/bindings'

/** Maximum entries we keep in a single snapshot. Excess matches are truncated. */
export const SNAPSHOT_ENTRIES_CAP = 10_000

/** The four pattern types the search engine accepts. */
export type SearchSnapshotMode = 'ai' | 'filename' | 'regex'

/** Filter values captured at snapshot creation time. All fields optional. */
export interface SearchSnapshotFilters {
  sizeMin?: number
  sizeMax?: number
  modifiedAfter?: number
  modifiedBefore?: number
}

/** A fully-materialized search snapshot. Immutable from the store's perspective. */
export interface SearchSnapshot {
  /** Monotonic per-session id, like 'sr-1'. */
  id: string
  /** The raw query text the user typed (or the AI's translated pattern in AI mode). */
  query: string
  /** Which engine produced these results. */
  mode: SearchSnapshotMode
  /** Captured filter values. Absent fields mean "no filter on this dimension". */
  filters: SearchSnapshotFilters
  /** Scope expression (comma-separated paths with `!` for exclusions). */
  scope: string
  caseSensitive: boolean
  excludeSystemDirs: boolean
  /** The result entries, capped at SNAPSHOT_ENTRIES_CAP. */
  entries: SearchResultEntry[]
  /** The total match count the backend reported (may exceed `entries.length`). */
  totalCount: number
  /** Snapshot creation time as a Unix epoch in milliseconds. */
  createdAt: number
  /** Friendly label for breadcrumbs and tab titles, derived from the query. */
  label: string
}

/** Internal map entry: snapshot plus the live reference count. */
interface StoreEntry extends SearchSnapshot {
  refCount: number
}

// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; consumers read imperatively at render time. Snapshots are immutable once stored (refCount aside), so there's nothing to subscribe to. See module header.
const store = new Map<string, StoreEntry>()

let nextId = 1
let lastAttemptId: string | null = null

/**
 * Monotonic mutation tick bumped whenever a stored snapshot's `entries` array is
 * mutated in place (currently only `removeEntryFromAllSnapshots`). Components
 * that render a snapshot's entries can subscribe to this via `getMutationTick()`
 * to re-derive after a cross-snapshot delete. We don't push reactivity into the
 * `Map` itself — keeping snapshots non-reactive is part of the design (see
 * module header) — but `mutationTick` IS a `$state` cell because it's the one
 * place where consumers need to subscribe.
 *
 * The tick is bumped at most once per `removeEntryFromAllSnapshots` call,
 * regardless of how many snapshots changed: consumers should re-derive in
 * full, not per-snapshot.
 */
let mutationTick = $state(0)

/** Returns the current mutation-tick value. Subscribe to drive re-renders after a cross-snapshot mutation. */
export function getMutationTick(): number {
  return mutationTick
}

/** Returns a fresh monotonic snapshot id (`sr-1`, `sr-2`, …). Per-session only. */
export function nextSnapshotId(): string {
  return `sr-${String(nextId++)}`
}

/**
 * Stores `snapshot` under `id` with refCount = 0 if it doesn't already exist, then
 * returns the canonical snapshot. If a snapshot with the same id is already stored,
 * the existing one is returned unchanged (never overwritten). Entry caps are
 * enforced here: if `snapshot.entries.length > SNAPSHOT_ENTRIES_CAP`, entries are
 * truncated and the label is annotated with `(first N of M)`.
 */
export function getOrCreate(id: string, snapshot: SearchSnapshot): SearchSnapshot {
  const existing = store.get(id)
  if (existing) {
    return existing
  }
  const total = snapshot.totalCount
  const capped =
    snapshot.entries.length > SNAPSHOT_ENTRIES_CAP ? snapshot.entries.slice(0, SNAPSHOT_ENTRIES_CAP) : snapshot.entries
  const label =
    snapshot.entries.length > SNAPSHOT_ENTRIES_CAP
      ? `${snapshot.label} (first ${String(SNAPSHOT_ENTRIES_CAP)} of ${String(total)})`
      : snapshot.label
  const entry: StoreEntry = {
    ...snapshot,
    id,
    entries: capped,
    label,
    refCount: 0,
  }
  store.set(id, entry)
  return entry
}

/** Returns the snapshot for `id`, or `undefined` if not stored / already evicted. */
export function getSnapshot(id: string): SearchSnapshot | undefined {
  return store.get(id)
}

/** Increments `id`'s refcount. No-op if the snapshot doesn't exist. */
export function incrementRef(id: string): void {
  const entry = store.get(id)
  if (!entry) return
  entry.refCount += 1
}

/**
 * Decrements `id`'s refcount. Deletes the snapshot when the count reaches 0.
 * No-op if the snapshot doesn't exist or the count is already 0 (defensive — a
 * decrement-below-zero would point at a refcount accounting bug elsewhere).
 */
export function decrementRef(id: string): void {
  const entry = store.get(id)
  if (!entry) return
  if (entry.refCount <= 0) return
  entry.refCount -= 1
  if (entry.refCount === 0) {
    store.delete(id)
  }
}

/** Returns the live refcount for `id`, or 0 if the snapshot is not in the store. */
export function getRefCount(id: string): number {
  return store.get(id)?.refCount ?? 0
}

/**
 * Removes the entry with the given `path` from every stored snapshot. Called
 * after a successful delete from a search-results pane so the row disappears
 * from this snapshot AND from any other snapshot that happened to contain the
 * same file. Returns the list of snapshot ids that were mutated (useful for
 * tests and debugging; production callers can ignore it).
 *
 * The `entries` array on each affected snapshot is replaced with a fresh
 * filtered array so reactive consumers (Svelte `$derived` over
 * `getSnapshot(id).entries`) see the change. We do NOT touch `totalCount` —
 * it still reports what the backend originally found; mismatch between
 * `entries.length` and `totalCount` is the existing "truncated-to-cap"
 * signal, so reusing it here is consistent.
 *
 * Per plan §3.7: "delete from search-results pane: confirms with the real
 * path. On success, the row is removed from this snapshot AND from any
 * other snapshot it appears in."
 */
export function removeEntryFromAllSnapshots(path: string): string[] {
  const mutatedIds: string[] = []
  for (const [id, entry] of store.entries()) {
    const filtered = entry.entries.filter((e) => e.path !== path)
    if (filtered.length !== entry.entries.length) {
      entry.entries = filtered
      mutatedIds.push(id)
    }
  }
  if (mutatedIds.length > 0) {
    mutationTick += 1
  }
  return mutatedIds
}

/**
 * Swaps the "last dialog attempt" strong ref: decrements the previously-pinned id
 * (if any) and increments the new one (if any). The dialog calls this whenever it
 * runs a fresh search so the most-recent attempt stays alive even when no pane
 * history references it. `setLastAttemptId(null)` releases the slot.
 */
export function setLastAttemptId(id: string | null): void {
  if (lastAttemptId === id) return
  const previous = lastAttemptId
  // Order: increment new first, decrement old second. The reverse order would
  // briefly drop the snapshot to refCount 0 and delete it when old === new (already
  // guarded above) or when both ids resolve to the same entry via aliasing — which
  // shouldn't happen today but the incrementing-first ordering is cheap insurance.
  if (id !== null) incrementRef(id)
  if (previous !== null) decrementRef(previous)
  lastAttemptId = id
}

/** Returns the id currently held by the "last dialog attempt" slot, or `null`. */
export function getLastAttemptId(): string | null {
  return lastAttemptId
}

/**
 * Dev-only inspection helper. Returns aggregate counts plus a list of every stored
 * id with its current refcount. Useful for spotting leaks during real-app testing
 * (MCP-driven over-fill, then verify counts back off).
 */
export function getDebugStats(): {
  count: number
  totalEntries: number
  maxRefCount: number
  idsWithRefCount: [string, number][]
} {
  let totalEntries = 0
  let maxRefCount = 0
  const idsWithRefCount: [string, number][] = []
  for (const [id, entry] of store.entries()) {
    totalEntries += entry.entries.length
    if (entry.refCount > maxRefCount) maxRefCount = entry.refCount
    idsWithRefCount.push([id, entry.refCount])
  }
  return { count: store.size, totalEntries, maxRefCount, idsWithRefCount }
}

/**
 * Test-only reset. Clears the store and resets the id counter and last-attempt slot.
 * Not exported for production use; tests import via the file path.
 */
export function _resetForTesting(): void {
  store.clear()
  nextId = 1
  lastAttemptId = null
  mutationTick = 0
}
