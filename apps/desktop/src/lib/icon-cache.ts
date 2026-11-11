// Icon cache for efficient icon loading
// Caches icon data URLs by icon ID to avoid redundant Tauri calls

import { writable } from 'svelte/store'
import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  getIcons,
  getCustomFolderIconIds,
  refreshDirectoryIcons as refreshIconsCommand,
  clearExtensionIconCache as clearExtensionIconCacheCommand,
  clearDirectoryIconCache as clearDirectoryIconCacheCommand,
  getDisplayAccessibility,
  onDisplayAccessibilityChanged,
  type CustomFolderIconId,
} from './tauri-commands'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('icon-cache')

const STORAGE_KEY = 'cmdr-icon-cache'

/**
 * Bump when a BOUNDED icon's pixels can change while its key stays the same.
 *
 * `dir` / `ext:*` / `special:*` survive restarts in localStorage and are only
 * refetched on a cache miss, so a backend change to how one is PRODUCED is
 * otherwise invisible forever on every machine that already ran the old build —
 * the fix ships, the pixels don't move, and it reads as the fix not working.
 *
 * 2: `dir` / `symlink-dir` stopped sampling the home directory, whose baked-in
 *    house badge every plain folder had been wearing.
 * 3: the envelope records the display variant its icons were drawn under.
 */
const CACHE_SCHEMA = 3

/** The persisted envelope. A bare map (no `version`) is a pre-schema build's. */
interface StoredCache {
  version: number
  /** The `IconDisplayVariant` the icons were drawn under. */
  variant: IconDisplayVariant
  icons: Record<string, string>
}

/**
 * The macOS display options that change the pixels macOS draws for an icon, as one value.
 * Mirrors the Rust `icons::IconVariant`, which keys the backend caches the same way.
 */
type IconDisplayVariant = 'standard' | 'contrast' | 'invert' | 'contrast+invert'

/** The two Accessibility > Display options that change icon pixels. */
interface IconDisplayOptions {
  increaseContrast: boolean
  invertColors: boolean
}

function iconDisplayVariantOf({ increaseContrast, invertColors }: IconDisplayOptions): IconDisplayVariant {
  if (increaseContrast && invertColors) return 'contrast+invert'
  if (increaseContrast) return 'contrast'
  if (invertColors) return 'invert'
  return 'standard'
}

/**
 * The variant the cached icons were drawn under: the persisted one at load, then whatever the OS
 * reports. `null` until either is known; nothing persists meanwhile, so an envelope never claims
 * a variant it can't vouch for.
 */
let activeVariant: IconDisplayVariant | null = null

/**
 * Bumped by every flush. A fetch remembers the value it started under, and its result is dropped
 * if a flush happened meanwhile: otherwise a slow fetch issued before a theme, accent, or display
 * change lands after the flush and puts the previous look back for good.
 */
let cacheEpoch = 0

const retryDelayMs = 5000

/**
 * Prefixes marking per-path (per-folder/per-package) icon keys. Unlike `dir` /
 * `ext:*` / `file` (an inherently bounded set), `path:` (custom-icon folders) and
 * `pkg:` (app/bundle packages) keys grow with the number of distinct folders and
 * bundles visited. They're LRU-capped in `memoryCache` and never persisted to
 * localStorage — the bounded `dir` / `ext:` / `special:` keys still persist.
 * Mirrors the Rust `ICON_CACHE` backstop (`is_per_path_key`).
 */
const PATH_KEY_PREFIX = 'path:'
const PKG_KEY_PREFIX = 'pkg:'

/** True for the unbounded per-path keys (`path:*` custom-icon folders + `pkg:*` packages). */
function isPerPathKey(id: string): boolean {
  return id.startsWith(PATH_KEY_PREFIX) || id.startsWith(PKG_KEY_PREFIX)
}

/**
 * Prefix marking special-system-folder icon keys (`special:downloads`, …). The
 * set is finite and stable (Downloads, Applications, the home folder, …), so —
 * unlike `path:` keys — these are uncapped and DO persist to localStorage
 * alongside `dir` / `ext:`. They're only cleared on theme/accent change, since
 * macOS tints the special-folder glyphs by the current appearance. Mirrors the
 * Rust `special_folders::SPECIAL_KEY_PREFIX`.
 */
const SPECIAL_KEY_PREFIX = 'special:'

/**
 * Backstop LRU cap for `path:`-keyed entries in `memoryCache`. A long session
 * browsing thousands of distinct folders would otherwise accumulate one base64 WebP
 * data-URL per folder forever. A few hundred covers any plausible visible/recent
 * working set; the rest evict oldest-first. Kept in sync with the Rust-side
 * `PATH_KEY_CAP`.
 */
const pathKeyCap = 256

/**
 * In-memory cache for current session. A `Map` preserves insertion order, which we
 * lean on for `path:`-key LRU eviction: re-setting a key moves it to the back, so the
 * front is always the oldest. See `setCacheEntry`.
 */
const memoryCache = new Map<string, string>()

/**
 * The `path:` keys whose custom icon follows the system accent (`followsAccent` from
 * `getCustomFolderIconIds`), each mapped to its Customize Folder emoji, or `null` for a symbol.
 * Only meaningful while the key is in `memoryCache`, so every removal goes through
 * `deleteCacheEntry` / `clearMemoryCache`, which keep the two in step.
 */
const accentTintedCustomFolderKeys = new Map<string, string | null>()

/** Removes one cache entry, with its accent flag. */
function deleteCacheEntry(key: string): void {
  memoryCache.delete(key)
  accentTintedCustomFolderKeys.delete(key)
}

/** Empties the in-memory cache, with every accent flag. */
function clearMemoryCache(): void {
  memoryCache.clear()
  accentTintedCustomFolderKeys.clear()
}

/** Number of per-path keys (`path:*` + `pkg:*`) currently held in `memoryCache`. */
function countPathKeys(): number {
  let count = 0
  for (const key of memoryCache.keys()) {
    if (isPerPathKey(key)) count++
  }
  return count
}

/**
 * Sets a cache entry, maintaining the per-path-key LRU. For `path:`/`pkg:` keys,
 * deletes any existing entry first so the re-insert lands at the back (most recent),
 * then evicts the oldest per-path keys until the count is within `pathKeyCap`. Bounded
 * keys are inserted as-is and never evicted by the cap.
 */
function setCacheEntry(id: string, url: string): void {
  if (isPerPathKey(id)) {
    memoryCache.delete(id)
    memoryCache.set(id, url)
    while (countPathKeys() > pathKeyCap) {
      // Front-most per-path key is the oldest; evict it.
      for (const key of memoryCache.keys()) {
        if (isPerPathKey(key)) {
          deleteCacheEntry(key)
          break
        }
      }
    }
  } else {
    memoryCache.set(id, url)
  }
}

/** Pending retry timer for timed-out prefetchIcons calls */
let prefetchRetryTimer: ReturnType<typeof setTimeout> | undefined

/** Pending retry timer for timed-out refreshDirectoryIcons calls */
let refreshRetryTimer: ReturnType<typeof setTimeout> | undefined

/**
 * Reactive version counter - increments when cache updates.
 * Components can subscribe to this to know when to re-render.
 */
export const iconCacheVersion = writable(0)

/**
 * Reactive counter that increments when part of the icon cache is cleared
 * (extension icons, directory icons, etc.).
 * List components subscribe to this to re-fetch icons for visible files.
 */
export const iconCacheCleared = writable(0)

/** Load persisted cache from localStorage */
function loadFromStorage(): void {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) {
      const parsed = JSON.parse(stored) as Partial<StoredCache>
      // Written by a build whose bounded icons may differ from what this one
      // would fetch. Drop the lot rather than guess which entries still hold:
      // they refetch on first use, and a stale one would never be revisited.
      if (parsed.version !== CACHE_SCHEMA || !parsed.icons || !parsed.variant) {
        localStorage.removeItem(STORAGE_KEY)
        return
      }
      activeVariant = parsed.variant
      for (const [id, url] of Object.entries(parsed.icons)) {
        // Defensive: skip any per-path (`path:`/`pkg:`) keys left by an older build.
        // They're no longer persisted, and feeding them in would seed the
        // bounded-keys-only cache.
        if (isPerPathKey(id)) continue
        memoryCache.set(id, url)
      }
    }
  } catch {
    // Ignore storage errors
  }
}

/** Persist cache to localStorage */
function saveToStorage(): void {
  if (activeVariant === null) return
  try {
    const obj: Record<string, string> = {}
    for (const [id, url] of memoryCache) {
      // Don't persist per-path (`path:`/`pkg:`) keys — they're unbounded and
      // session-scoped, and the Rust on-disk cache already persists them keyed by
      // folder mtime. Only the bounded `dir` / `ext:` / `special:` keys survive
      // restarts in localStorage.
      if (isPerPathKey(id)) continue
      obj[id] = url
    }
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ version: CACHE_SCHEMA, variant: activeVariant, icons: obj } satisfies StoredCache),
    )
  } catch {
    // Ignore storage errors
  }
}

// Load on module init
if (typeof localStorage !== 'undefined') {
  loadFromStorage()
}

/**
 * Merges icons fetched under `epoch` into the cache, persists, and bumps the version counter.
 * Drops them if the cache was flushed since the fetch started. Returns true if any icons were added.
 */
function applyIconsToCache(icons: Record<string, string>, epoch: number): boolean {
  if (epoch !== cacheEpoch) return false
  let changed = false
  for (const [id, url] of Object.entries(icons)) {
    const existing = memoryCache.get(id)
    if (existing !== url) {
      setCacheEntry(id, url)
      changed = true
    }
  }
  if (changed) {
    saveToStorage()
    iconCacheVersion.update((v) => v + 1)
  }
  return changed
}

/**
 * Prefetches icons for the given IDs.
 * Fetches only those not already cached.
 * Increments iconCacheVersion when new icons are loaded, triggering re-renders.
 * If the backend times out, schedules a single silent retry after ~5 seconds.
 *
 * @param iconIds - Array of icon IDs to prefetch
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 */
export async function prefetchIcons(iconIds: string[], useAppIconsForDocuments: boolean): Promise<void> {
  const uncached = iconIds.filter((id) => !memoryCache.has(id))
  if (uncached.length === 0) return

  // Cancel any pending retry: a new fetch supersedes it
  clearTimeout(prefetchRetryTimer)
  prefetchRetryTimer = undefined

  // Deduplicate
  const unique = [...new Set(uncached)]
  const epoch = cacheEpoch
  const { data: icons, timedOut } = await getIcons(unique, useAppIconsForDocuments)

  applyIconsToCache(icons, epoch)

  if (timedOut && epoch === cacheEpoch) {
    prefetchRetryTimer = setTimeout(() => {
      prefetchRetryTimer = undefined
      void getIcons(unique, useAppIconsForDocuments)
        .then(({ data: retryIcons }) => applyIconsToCache(retryIcons, epoch))
        .catch(() => {
          // Give up silently on retry failure
        })
    }, retryDelayMs)
  }
}

/**
 * Detects and fetches custom-folder icons for the VISIBLE directory rows.
 *
 * Custom-icon detection (`kHasCustomIcon` xattr) is a `getxattr` per directory, so
 * the backend deliberately does NOT run it during the bulk listing — it'd regress
 * a 100k-entry directory. Instead the frontend calls this for the bounded set of
 * directory paths actually on screen: the backend filters down to the few that
 * truly carry a custom icon (returning their `path:{dir}` ids), and we then fetch
 * those through the normal `prefetchIcons` path. Folders without a custom icon
 * keep their generic `dir` glyph — purely additive.
 *
 * Best-effort: errors and timeouts are swallowed (the folder just stays generic).
 *
 * @param directoryPaths - Full paths of the visible directory rows
 * @param useAppIconsForDocuments - Passed through to the icon fetch
 */
export async function prefetchCustomFolderIcons(
  directoryPaths: string[],
  useAppIconsForDocuments: boolean,
): Promise<void> {
  if (directoryPaths.length === 0) return
  // Only ask about dirs we don't already have a per-path icon for, to keep the
  // getxattr batch small on re-scroll over the same rows.
  const unknown = directoryPaths.filter((p) => !memoryCache.has(`${PATH_KEY_PREFIX}${p}`))
  if (unknown.length === 0) return

  let found: CustomFolderIconId[]
  try {
    const { data } = await getCustomFolderIconIds(unknown)
    found = data
  } catch {
    return // Best-effort: keep the generic dir glyph.
  }
  if (found.length === 0) return
  for (const { iconId, followsAccent, emoji } of found) {
    if (followsAccent) accentTintedCustomFolderKeys.set(iconId, emoji)
    else accentTintedCustomFolderKeys.delete(iconId)
  }
  await prefetchIcons(
    found.map((f) => f.iconId),
    useAppIconsForDocuments,
  )
}

/**
 * True when a folder's cached custom icon is a macOS Customize Folder symbol or emoji that
 * macOS tints with the system accent, so the gold recolor applies to it like to a plain folder.
 * False for pasted artwork, a color the person chose with a tag, and folders with no custom icon.
 */
export function isAccentTintedCustomFolder(directoryPath: string): boolean {
  return accentTintedCustomFolderKeys.has(`${PATH_KEY_PREFIX}${directoryPath}`)
}

/**
 * The emoji of an accent-tinted Customize Folder emoji folder, or undefined (a symbol, artwork,
 * a tag color, no custom icon). The gold filter grays the whole bitmap, emoji included, so
 * `FileIcon` draws such a folder as the gold plain folder plus this emoji on top (#814).
 */
export function getAccentTintedCustomFolderEmoji(directoryPath: string): string | undefined {
  return accentTintedCustomFolderKeys.get(`${PATH_KEY_PREFIX}${directoryPath}`) ?? undefined
}

/**
 * Evicts the per-path icon keys (`path:*` + `pkg:*`) for the direct children of a
 * directory that's no longer visible (a pane navigated away / closed its listing).
 * The in-memory LRU already bounds these as a backstop; this keeps the working set
 * tight and ensures a re-icon is re-detected next time the folder is shown rather
 * than served from a now-stale session entry.
 *
 * Matches a key's embedded path against `${dirPath}/`-prefixed children so sibling
 * directories' icons are untouched. The Rust on-disk cache is the durable tier and
 * is unaffected (it invalidates by mtime).
 *
 * @param dirPath - The directory whose listing ended
 */
export function evictPerPathIconsForDir(dirPath: string): void {
  if (!dirPath) return
  // Normalize to a child-prefix: keys are `path:/abs/child` or `pkg:/abs/child`.
  // eslint-disable-next-line cmdr/no-raw-path-arithmetic -- a child-prefix test; a root (`/`, `C:/`) already ends in `/`
  const childPrefix = dirPath.endsWith('/') ? dirPath : `${dirPath}/`
  let removed = false
  for (const key of memoryCache.keys()) {
    if (!isPerPathKey(key)) continue
    const embeddedPath = key.slice(key.indexOf(':') + 1)
    if (embeddedPath.startsWith(childPrefix)) {
      deleteCacheEntry(key)
      removed = true
    }
  }
  if (removed) iconCacheVersion.update((v) => v + 1)
}

/**
 * Gets icon from cache only (no fetch).
 * Returns undefined if not cached.
 */
export function getCachedIcon(iconId: string): string | undefined {
  return memoryCache.get(iconId)
}

/**
 * Gets a folder's Finder custom icon from the cache, or undefined when it has
 * none (the overwhelmingly common case).
 *
 * A custom-icon folder keeps the generic `dir` iconId — the backend defers the
 * `kHasCustomIcon` getxattr off the bulk-listing hot path — so its icon arrives
 * under a `path:{dir}` key that no entry ever points at. Renderers must ask by
 * PATH here, not by `iconId`, or `prefetchCustomFolderIcons` fetches and caches
 * an icon nothing ever draws.
 */
export function getCachedCustomFolderIcon(directoryPath: string): string | undefined {
  return memoryCache.get(`${PATH_KEY_PREFIX}${directoryPath}`)
}

/**
 * Refreshes icons for a directory listing.
 * Fetches icons in parallel for:
 * - All directories by exact path (for custom folder icons)
 * - All unique extensions (for file association changes)
 *
 * Updates the cache and triggers re-render if any icons changed.
 * If the backend times out, schedules a single silent retry after ~5 seconds.
 * @param directoryPaths - Array of directory paths to fetch icons for
 * @param extensions - Array of file extensions (without dot)
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 * @public
 */
export async function refreshDirectoryIcons(
  directoryPaths: string[],
  extensions: string[],
  useAppIconsForDocuments: boolean,
): Promise<void> {
  if (directoryPaths.length === 0 && extensions.length === 0) return

  // Cancel any pending retry: a new refresh supersedes it
  clearTimeout(refreshRetryTimer)
  refreshRetryTimer = undefined

  const epoch = cacheEpoch
  const { data: icons, timedOut } = await refreshIconsCommand(directoryPaths, extensions, useAppIconsForDocuments)

  applyIconsToCache(icons, epoch)

  if (timedOut && epoch === cacheEpoch) {
    refreshRetryTimer = setTimeout(() => {
      refreshRetryTimer = undefined
      void refreshIconsCommand(directoryPaths, extensions, useAppIconsForDocuments)
        .then(({ data: retryIcons }) => applyIconsToCache(retryIcons, epoch))
        .catch(() => {
          // Give up silently on retry failure
        })
    }, retryDelayMs)
  }
}

/**
 * Clears all cached extension icons from both memory and localStorage.
 * Called when the "use app icons for documents" setting changes.
 * After calling this, extension icons will be re-fetched with the new setting.
 */
export async function clearExtensionIconCache(): Promise<void> {
  startNewEpoch()

  // Clear backend cache
  await clearExtensionIconCacheCommand()

  // Clear frontend cache (extension icons only)
  for (const key of memoryCache.keys()) {
    if (key.startsWith('ext:')) {
      deleteCacheEntry(key)
    }
  }

  // Persist the change
  saveToStorage()

  // Notify list components to re-fetch icons for visible files
  // This must happen BEFORE incrementing iconCacheVersion so components
  // can re-fetch before re-rendering with the cleared cache
  iconCacheCleared.update((v) => v + 1)

  // Trigger reactive update so components re-fetch icons
  iconCacheVersion.update((v) => v + 1)
}

/**
 * Clears all cached directory icons from both memory and localStorage.
 * Called when the system theme or accent color changes, since macOS renders
 * folder icons with the current accent color baked in.
 */
export async function clearDirectoryIconCache(): Promise<void> {
  startNewEpoch()

  await clearDirectoryIconCacheCommand()

  for (const key of memoryCache.keys()) {
    if (key === 'dir' || key === 'symlink-dir' || isPerPathKey(key) || key.startsWith(SPECIAL_KEY_PREFIX)) {
      deleteCacheEntry(key)
    }
  }

  saveToStorage()
  iconCacheCleared.update((v) => v + 1)
  iconCacheVersion.update((v) => v + 1)
}

/**
 * Starts a new cache epoch: fetches already in flight, and their pending retries, can no longer
 * write. Synchronous, so it takes effect before any `await` in the flush that calls it.
 */
function startNewEpoch(): void {
  cacheEpoch++
  clearTimeout(prefetchRetryTimer)
  prefetchRetryTimer = undefined
  clearTimeout(refreshRetryTimer)
  refreshRetryTimer = undefined
}

/**
 * Points the cache at the icons macOS draws under `options`, dropping EVERY icon (file icons
 * too) when Increase contrast or Invert colors changed. The backend has already flushed its own
 * tiers by the time the change event arrives, so the refetch gets the new look.
 */
function syncToDisplayOptions(options: IconDisplayOptions): void {
  const variant = iconDisplayVariantOf(options)
  if (variant === activeVariant) return
  const previous = activeVariant
  activeVariant = variant
  // Unknown until now, so whatever is in memory came from this session's backend, which already
  // drew it under the real options.
  if (previous === null) {
    saveToStorage()
    return
  }
  log.debug('Display options changed the icon look from {previous} to {variant}, refetching', { previous, variant })
  startNewEpoch()
  clearMemoryCache()
  saveToStorage()
  iconCacheCleared.update((v) => v + 1)
  iconCacheVersion.update((v) => v + 1)
}

let unlistenDisplayAccessibility: UnlistenFn | undefined

/**
 * Keeps the cache in step with the macOS Increase contrast and Invert colors options, under which
 * macOS draws different file and folder icons: on startup (a persisted cache may predate a change
 * made while Cmdr was closed) and live. Call once, from the window that shows file icons.
 */
export async function initIconDisplayVariant(): Promise<void> {
  try {
    syncToDisplayOptions(await getDisplayAccessibility())
  } catch (error) {
    log.warn('Could not read the display options, so icons keep their cached look: {error}', { error })
  }
  try {
    unlistenDisplayAccessibility = await onDisplayAccessibilityChanged((payload) => {
      syncToDisplayOptions(payload.options)
    })
  } catch (error) {
    log.warn('Could not subscribe to display option changes: {error}', { error })
  }
}

/** Stops following the display options. */
export function cleanupIconDisplayVariant(): void {
  unlistenDisplayAccessibility?.()
  unlistenDisplayAccessibility = undefined
}

/** Test-only: clears the in-memory cache so each test starts from a known state, drawn under the standard look. */
export function _resetIconCacheForTests(): void {
  clearMemoryCache()
  activeVariant = 'standard'
}

/** Test-only: applies fetched icons through the normal cache path (LRU + persist). */
export function _applyIconsToCacheForTests(icons: Record<string, string>): void {
  applyIconsToCache(icons, cacheEpoch)
}

/** Test-only: the `path:`-key LRU cap, exposed so tests don't hard-code the value. */
export const _pathKeyCapForTests = pathKeyCap
