/**
 * Per-path tail-mode persistence.
 *
 * Persists "did the user have tail on for this file last time?" in a small
 * LRU map. The key is `sha256(absolute path)` truncated to 16 hex chars: the
 * settings file stays small and absolute paths don't appear in clear in
 * persisted state.
 *
 * Capacity is capped at 100 entries. The LRU is **access-promoted**: reading
 * an entry moves it to the front, so the entry evicted on a fresh insert is
 * the one nobody's touched in the longest time. Write-storm avoidance: the
 * in-memory LRU updates synchronously, but disk writes are debounced 5 s
 * after the last mutation; consumers also explicitly flush on session close
 * to guarantee crash-safety for the most recent value.
 */

import { load, type Store } from '@tauri-apps/plugin-store'

const STORE_NAME = 'viewer-tail.json'
const STORE_KEY = 'pathTailMode'
const MAX_ENTRIES = 100
const SAVE_DEBOUNCE_MS = 5_000

interface Entry {
  hash: string
  enabled: boolean
}

let cache: Entry[] = []
let storeInstance: Store | null = null
let initialized = false
let initPromise: Promise<void> | null = null
let saveTimer: ReturnType<typeof setTimeout> | null = null

async function getStore(): Promise<Store> {
  if (storeInstance) return storeInstance
  storeInstance = await load(STORE_NAME, { defaults: { [STORE_KEY]: [] }, autoSave: false })
  return storeInstance
}

async function initialize(): Promise<void> {
  if (initialized) return
  if (initPromise) return initPromise
  initPromise = (async () => {
    try {
      const store = await getStore()
      const raw = await store.get<unknown>(STORE_KEY)
      if (Array.isArray(raw)) {
        cache = raw
          .filter(
            (e): e is Entry =>
              typeof e === 'object' &&
              e !== null &&
              typeof (e as Entry).hash === 'string' &&
              typeof (e as Entry).enabled === 'boolean',
          )
          .slice(-MAX_ENTRIES)
      }
      initialized = true
    } catch {
      // Best-effort: persistence failures degrade to "tail mode resets each session."
      initialized = true
    }
  })()
  return initPromise
}

function scheduleSave(): void {
  if (saveTimer !== null) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    void flush()
  }, SAVE_DEBOUNCE_MS)
}

/** Flush the in-memory map to disk immediately. Safe to call from session close. */
export async function flush(): Promise<void> {
  if (saveTimer !== null) {
    clearTimeout(saveTimer)
    saveTimer = null
  }
  if (!initialized) return
  try {
    const store = await getStore()
    await store.set(STORE_KEY, cache)
    await store.save()
  } catch {
    // Same degradation policy as init.
  }
}

/** SHA-256 the absolute path and truncate to the first 16 hex chars (64 bits). */
export async function hashPath(path: string): Promise<string> {
  const buf = new TextEncoder().encode(path)
  const digest = await crypto.subtle.digest('SHA-256', buf)
  const bytes = new Uint8Array(digest)
  let hex = ''
  for (let i = 0; i < bytes.length; i++) {
    hex += bytes[i].toString(16).padStart(2, '0')
  }
  return hex.slice(0, 16)
}

function findIndex(hash: string): number {
  for (let i = 0; i < cache.length; i++) {
    if (cache[i].hash === hash) return i
  }
  return -1
}

/**
 * Returns the persisted tail-mode flag for `path`, or `null` when no entry
 * exists. Reading promotes the entry to the most-recently-used slot.
 */
export async function getLastTailMode(path: string): Promise<boolean | null> {
  await initialize()
  const hash = await hashPath(path)
  const idx = findIndex(hash)
  if (idx === -1) return null
  const [entry] = cache.splice(idx, 1)
  cache.push(entry)
  scheduleSave()
  return entry.enabled
}

/**
 * Persist the tail-mode flag for `path`. Evicts the oldest unread entry if
 * the cache is at capacity. Disk write is debounced 5 s.
 */
export async function setLastTailMode(path: string, enabled: boolean): Promise<void> {
  await initialize()
  const hash = await hashPath(path)
  const idx = findIndex(hash)
  if (idx !== -1) {
    cache.splice(idx, 1)
  } else if (cache.length >= MAX_ENTRIES) {
    cache.shift()
  }
  cache.push({ hash, enabled })
  scheduleSave()
}

/** Test-only: wipe in-memory state and pending timers. */
export function _testOnlyReset(): void {
  if (saveTimer !== null) {
    clearTimeout(saveTimer)
    saveTimer = null
  }
  cache = []
  storeInstance = null
  initialized = false
  initPromise = null
}

/** Test-only: peek at the in-memory cache. */
export function _testOnlyGetCache(): Entry[] {
  return cache.slice()
}

/** Test-only: set the cache directly (skip async hashing). */
export function _testOnlyPushEntry(hash: string, enabled: boolean): void {
  cache.push({ hash, enabled })
  initialized = true
}
