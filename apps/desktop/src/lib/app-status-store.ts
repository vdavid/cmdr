// App status persistence for paths and focus state

import { load } from '@tauri-apps/plugin-store'
import type { Store } from '@tauri-apps/plugin-store'
import type { SortColumn } from './file-explorer/types'
import { defaultSortOrders } from './file-explorer/types'
import type { PersistedTab, PersistedPaneTabs } from './file-explorer/tabs/tab-types'
import { resolveValidPath } from './file-explorer/navigation/path-resolution'
import { resolveStorePath } from './settings/store-path'

const STORE_NAME = 'app-status.json'
const DEFAULT_PATH = '~'
const DEFAULT_VOLUME_ID = 'root'
const DEFAULT_SORT_BY: SortColumn = 'name'

export type ViewMode = 'full' | 'brief'

export interface AppStatus {
  leftPath: string
  rightPath: string
  focusedPane: 'left' | 'right'
  leftViewMode: ViewMode
  rightViewMode: ViewMode
  leftVolumeId: string
  rightVolumeId: string
  leftSortBy: SortColumn
  rightSortBy: SortColumn
  /** Left pane width as percentage (25-75). Default: 50 */
  leftPaneWidthPercent: number
  /** Whether the Ask Cmdr rail is open. Default: false */
  askCmdrRailOpen: boolean
  /** Ask Cmdr rail width in px (280-520). Default: 340 */
  askCmdrRailWidth: number
}

const DEFAULT_LEFT_PANE_WIDTH_PERCENT = 50
const DEFAULT_ASK_CMDR_RAIL_WIDTH = 340
const ASK_CMDR_RAIL_MIN_WIDTH = 280
const ASK_CMDR_RAIL_MAX_WIDTH = 520

const DEFAULT_STATUS: AppStatus = {
  leftPath: DEFAULT_PATH,
  rightPath: DEFAULT_PATH,
  focusedPane: 'left',
  leftViewMode: 'brief',
  rightViewMode: 'brief',
  leftVolumeId: DEFAULT_VOLUME_ID,
  rightVolumeId: DEFAULT_VOLUME_ID,
  leftSortBy: DEFAULT_SORT_BY,
  rightSortBy: DEFAULT_SORT_BY,
  leftPaneWidthPercent: DEFAULT_LEFT_PANE_WIDTH_PERCENT,
  askCmdrRailOpen: false,
  askCmdrRailWidth: DEFAULT_ASK_CMDR_RAIL_WIDTH,
}

let storeInstance: Store | null = null

async function getStore(): Promise<Store> {
  if (!storeInstance) {
    // Resolve the store path so isolated instances (dev, per-worktree dev, E2E)
    // don't read the real production `app-status.json`. See `settings/store-path.ts`.
    const storePath = await resolveStorePath(STORE_NAME)
    storeInstance = await load(storePath)
  }
  return storeInstance
}

/**
 * Resolves a persisted path, falling back to ~ if nothing exists.
 * Uses resolveValidPath with no timeout (startup paths are local, no hung-mount risk at load time)
 * and the caller's pathExistsFn (which may be mocked in tests).
 */
async function resolvePersistedPath(path: string, pathExistsFn: (p: string) => Promise<boolean>): Promise<string> {
  return (await resolveValidPath(path, { pathExistsFn, timeoutMs: 0 })) ?? DEFAULT_PATH
}

function parseViewMode(raw: unknown): ViewMode {
  return raw === 'full' || raw === 'brief' ? raw : 'full'
}

function parseSortColumn(raw: unknown): SortColumn {
  const validColumns: SortColumn[] = ['name', 'extension', 'size', 'modified', 'created']
  if (typeof raw === 'string' && validColumns.includes(raw as SortColumn)) {
    return raw as SortColumn
  }
  return DEFAULT_SORT_BY
}

function parsePaneWidthPercent(raw: unknown): number {
  if (typeof raw === 'number' && raw >= 25 && raw <= 75) {
    return raw
  }
  return DEFAULT_LEFT_PANE_WIDTH_PERCENT
}

function parseRailWidth(raw: unknown): number {
  if (typeof raw === 'number' && raw >= ASK_CMDR_RAIL_MIN_WIDTH && raw <= ASK_CMDR_RAIL_MAX_WIDTH) {
    return raw
  }
  return DEFAULT_ASK_CMDR_RAIL_WIDTH
}

export async function loadAppStatus(pathExists: (p: string) => Promise<boolean>): Promise<AppStatus> {
  try {
    const store = await getStore()
    const leftPath = ((await store.get('leftPath')) as string) || DEFAULT_PATH
    const rightPath = ((await store.get('rightPath')) as string) || DEFAULT_PATH
    const rawFocusedPane = await store.get('focusedPane')
    const focusedPane: 'left' | 'right' = rawFocusedPane === 'right' ? 'right' : 'left'
    const leftViewMode = parseViewMode(await store.get('leftViewMode'))
    const rightViewMode = parseViewMode(await store.get('rightViewMode'))
    const leftVolumeId = ((await store.get('leftVolumeId')) as string) || DEFAULT_VOLUME_ID
    const rightVolumeId = ((await store.get('rightVolumeId')) as string) || DEFAULT_VOLUME_ID
    const leftSortBy = parseSortColumn(await store.get('leftSortBy'))
    const rightSortBy = parseSortColumn(await store.get('rightSortBy'))
    const leftPaneWidthPercent = parsePaneWidthPercent(await store.get('leftPaneWidthPercent'))
    const askCmdrRailOpen = (await store.get('askCmdrRailOpen')) === true
    const askCmdrRailWidth = parseRailWidth(await store.get('askCmdrRailWidth'))

    // Resolve paths with fallback - skip for virtual 'network' volume
    const resolvedLeftPath = leftVolumeId === 'network' ? leftPath : await resolvePersistedPath(leftPath, pathExists)
    const resolvedRightPath =
      rightVolumeId === 'network' ? rightPath : await resolvePersistedPath(rightPath, pathExists)

    return {
      leftPath: resolvedLeftPath,
      rightPath: resolvedRightPath,
      focusedPane,
      leftViewMode,
      rightViewMode,
      leftVolumeId,
      rightVolumeId,
      leftSortBy,
      rightSortBy,
      leftPaneWidthPercent,
      askCmdrRailOpen,
      askCmdrRailWidth,
    }
  } catch {
    // If store fails, return defaults
    return DEFAULT_STATUS
  }
}

const SAVE_DEBOUNCE_MS = 200
let saveDebounceTimer: ReturnType<typeof setTimeout> | null = null
let pendingSave: Partial<AppStatus> | null = null

/** Debounced save: merges with pending writes and flushes after 200ms of inactivity. */
export function saveAppStatus(status: Partial<AppStatus>): void {
  pendingSave = { ...pendingSave, ...status }

  if (saveDebounceTimer !== null) {
    clearTimeout(saveDebounceTimer)
  }

  saveDebounceTimer = setTimeout(() => {
    const toSave = pendingSave
    pendingSave = null
    saveDebounceTimer = null
    if (toSave) {
      void doSaveAppStatus(toSave)
    }
  }, SAVE_DEBOUNCE_MS)
}

async function doSaveAppStatus(status: Partial<AppStatus>): Promise<void> {
  try {
    const store = await getStore()
    if (status.leftPath !== undefined) {
      await store.set('leftPath', status.leftPath)
    }
    if (status.rightPath !== undefined) {
      await store.set('rightPath', status.rightPath)
    }
    if (status.focusedPane !== undefined) {
      await store.set('focusedPane', status.focusedPane)
    }
    if (status.leftViewMode !== undefined) {
      await store.set('leftViewMode', status.leftViewMode)
    }
    if (status.rightViewMode !== undefined) {
      await store.set('rightViewMode', status.rightViewMode)
    }
    if (status.leftVolumeId !== undefined) {
      await store.set('leftVolumeId', status.leftVolumeId)
    }
    if (status.rightVolumeId !== undefined) {
      await store.set('rightVolumeId', status.rightVolumeId)
    }
    if (status.leftSortBy !== undefined) {
      await store.set('leftSortBy', status.leftSortBy)
    }
    if (status.rightSortBy !== undefined) {
      await store.set('rightSortBy', status.rightSortBy)
    }
    if (status.leftPaneWidthPercent !== undefined) {
      await store.set('leftPaneWidthPercent', status.leftPaneWidthPercent)
    }
    if (status.askCmdrRailOpen !== undefined) {
      await store.set('askCmdrRailOpen', status.askCmdrRailOpen)
    }
    if (status.askCmdrRailWidth !== undefined) {
      await store.set('askCmdrRailWidth', status.askCmdrRailWidth)
    }
    await store.save()
  } catch {
    // Silently fail - persistence is nice-to-have
  }
}

/** Map of volumeId -> last used path for that volume */
export type VolumePathMap = Record<string, string>

function isValidPathMap(value: unknown): value is VolumePathMap {
  if (typeof value !== 'object' || value === null) return false
  return Object.entries(value).every(([k, v]) => typeof k === 'string' && typeof v === 'string')
}

/**
 * Gets the last used path for a specific volume.
 * Returns undefined if no path is stored.
 */
export async function getLastUsedPathForVolume(volumeId: string): Promise<string | undefined> {
  try {
    const store = await getStore()
    const lastUsedPaths = await store.get('lastUsedPaths')
    if (isValidPathMap(lastUsedPaths)) {
      return lastUsedPaths[volumeId]
    }
    return undefined
  } catch {
    return undefined
  }
}

/**
 * Saves the last used path for a specific volume.
 * This is more efficient than loading/saving the full status.
 */
export async function saveLastUsedPathForVolume(volumeId: string, path: string): Promise<void> {
  try {
    const store = await getStore()
    const lastUsedPaths = await store.get('lastUsedPaths')
    const paths: VolumePathMap = isValidPathMap(lastUsedPaths) ? lastUsedPaths : {}
    paths[volumeId] = path
    await store.set('lastUsedPaths', paths)
    await store.save()
  } catch {
    // Silently fail - persistence is nice-to-have
  }
}

// ============================================================================
// Command palette recents persistence
// ============================================================================

export const RECENT_COMMANDS_LIMIT = 10

/**
 * Pure update step for the recents list: move `commandId` to the front,
 * drop any prior occurrence, cap at RECENT_COMMANDS_LIMIT. Exposed for testing.
 */
export function dedupAndPrependRecent(existing: string[], commandId: string): string[] {
  return [commandId, ...existing.filter((id) => id !== commandId)].slice(0, RECENT_COMMANDS_LIMIT)
}

/**
 * Loads the list of recently executed command IDs, most-recent first.
 * Returns an empty array if nothing was saved or parsing fails.
 */
export async function loadRecentCommands(): Promise<string[]> {
  try {
    const store = await getStore()
    const raw = await store.get('recentCommandIds')
    if (!Array.isArray(raw)) return []
    return raw.filter((id): id is string => typeof id === 'string').slice(0, RECENT_COMMANDS_LIMIT)
  } catch {
    return []
  }
}

/**
 * Records a command execution. The given ID is moved to the front; if it was
 * already in the list, the previous entry is dropped (no duplicates). The list
 * is capped at RECENT_COMMANDS_LIMIT entries.
 */
export async function pushRecentCommand(commandId: string): Promise<void> {
  try {
    const store = await getStore()
    const existing = await loadRecentCommands()
    const next = dedupAndPrependRecent(existing, commandId)
    await store.set('recentCommandIds', next)
    await store.save()
  } catch {
    // Silently fail - persistence is nice-to-have
  }
}

/**
 * Loads recents and drops any IDs that aren't in `validIds`. If anything was
 * pruned, the cleaned list is written back. Returns the (possibly pruned) list.
 *
 * Call this on palette open: it self-heals the store against commands that were
 * renamed or removed since the user last used them. Without it, stale IDs would
 * just take up slots in the cap-10 list and reduce the visible recents count.
 */
export async function pruneRecentCommands(validIds: ReadonlySet<string>): Promise<string[]> {
  try {
    const existing = await loadRecentCommands()
    const pruned = existing.filter((id) => validIds.has(id))
    if (pruned.length !== existing.length) {
      const store = await getStore()
      await store.set('recentCommandIds', pruned)
      await store.save()
    }
    return pruned
  } catch {
    return []
  }
}

// ============================================================================
// Settings window section persistence
// ============================================================================

const DEFAULT_SETTINGS_SECTION = ['Appearance', 'Colors and formats']

/**
 * Loads the last viewed settings section.
 * Returns default section if not previously saved.
 */
export async function loadLastSettingsSection(): Promise<string[]> {
  try {
    const store = await getStore()
    const section = await store.get('lastSettingsSection')
    if (Array.isArray(section) && section.every((s): s is string => typeof s === 'string')) {
      return section
    }
    return DEFAULT_SETTINGS_SECTION
  } catch {
    return DEFAULT_SETTINGS_SECTION
  }
}

/**
 * Saves the current settings section for next time.
 */
export async function saveLastSettingsSection(section: string[]): Promise<void> {
  try {
    const store = await getStore()
    await store.set('lastSettingsSection', section)
    await store.save()
  } catch {
    // Silently fail
  }
}

// ============================================================================
// Tab persistence
// ============================================================================

function isValidPersistedTab(raw: unknown): raw is PersistedTab {
  if (typeof raw !== 'object' || raw === null) return false
  const obj = raw as Record<string, unknown>
  return (
    typeof obj.id === 'string' &&
    typeof obj.path === 'string' &&
    typeof obj.volumeId === 'string' &&
    parseSortColumn(obj.sortBy) === obj.sortBy &&
    (obj.sortOrder === 'ascending' || obj.sortOrder === 'descending') &&
    (obj.viewMode === 'full' || obj.viewMode === 'brief') &&
    typeof obj.pinned === 'boolean'
  )
}

function isValidPersistedPaneTabs(raw: unknown): raw is PersistedPaneTabs {
  if (typeof raw !== 'object' || raw === null) return false
  const obj = raw as Record<string, unknown>
  if (!Array.isArray(obj.tabs) || typeof obj.activeTabId !== 'string') return false
  return obj.tabs.length > 0 && obj.tabs.every(isValidPersistedTab)
}

/**
 * Loads persisted tab state for a pane side.
 * Falls back to migration from old scalar keys if no tab data exists.
 */
export async function loadPaneTabs(
  side: 'left' | 'right',
  pathExistsFn: (p: string) => Promise<boolean>,
): Promise<PersistedPaneTabs> {
  try {
    const store = await getStore()
    const key = `${side}Tabs`
    const raw = await store.get(key)

    if (isValidPersistedPaneTabs(raw)) {
      // Validate paths exist, fall back for any that don't
      const validatedTabs = await Promise.all(
        raw.tabs.map(async (tab) => {
          if (tab.volumeId === 'network') return tab
          const resolvedPath = await resolvePersistedPath(tab.path, pathExistsFn)
          return { ...tab, path: resolvedPath }
        }),
      )
      return { tabs: validatedTabs, activeTabId: raw.activeTabId }
    }

    // TODO(2026-04-01): remove migration
    // Migration from old scalar keys
    const path = ((await store.get(`${side}Path`)) as string) || DEFAULT_PATH
    const volumeId = ((await store.get(`${side}VolumeId`)) as string) || DEFAULT_VOLUME_ID
    const sortBy = parseSortColumn(await store.get(`${side}SortBy`))
    const viewMode = parseViewMode(await store.get(`${side}ViewMode`))
    const resolvedPath = volumeId === 'network' ? path : await resolvePersistedPath(path, pathExistsFn)

    const tab: PersistedTab = {
      id: crypto.randomUUID(),
      path: resolvedPath,
      volumeId,
      sortBy,
      sortOrder: defaultSortOrders[sortBy],
      viewMode,
      pinned: false,
    }

    return { tabs: [tab], activeTabId: tab.id }
  } catch {
    const id = crypto.randomUUID()
    return {
      tabs: [
        {
          id,
          path: DEFAULT_PATH,
          volumeId: DEFAULT_VOLUME_ID,
          sortBy: DEFAULT_SORT_BY,
          sortOrder: defaultSortOrders[DEFAULT_SORT_BY],
          viewMode: 'full',
          pinned: false,
        },
      ],
      activeTabId: id,
    }
  }
}

/** Saves tab state for a pane side. */
export async function savePaneTabs(side: 'left' | 'right', paneTabs: PersistedPaneTabs): Promise<void> {
  try {
    const store = await getStore()
    await store.set(`${side}Tabs`, paneTabs)
    await store.save()
  } catch {
    // Silently fail
  }
}
