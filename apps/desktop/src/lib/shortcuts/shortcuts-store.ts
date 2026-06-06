/**
 * Shortcuts persistence layer - stores custom keyboard shortcuts.
 */

import { load, type Store } from '@tauri-apps/plugin-store'
import { commands as ipcCommands } from '$lib/ipc/bindings'
import { commands } from '$lib/commands/command-registry'
import { resolveStorePath } from '$lib/settings/store-path'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'
import { toPlatformShortcut } from './key-capture'

const log = getAppLogger('shortcuts')

// ============================================================================
// Store configuration
// ============================================================================

const STORE_NAME = 'shortcuts.json'
const SCHEMA_VERSION = 1

let storeInstance: Store | null = null

// In-memory cache of custom shortcuts
const customShortcuts = new Map<string, string[]>()
let initialized = false

// ============================================================================
// Initialization
// ============================================================================

async function getStore(): Promise<Store> {
  if (!storeInstance) {
    // Resolve the store path so isolated instances (dev, per-worktree dev, E2E)
    // don't read the real production `shortcuts.json`. See `settings/store-path.ts`.
    const storePath = await resolveStorePath(STORE_NAME, (e) => {
      log.warn('Could not resolve isolated shortcuts path, using default: {error}', { error: String(e) })
    })
    storeInstance = await load(storePath, {
      defaults: { _schemaVersion: SCHEMA_VERSION },
      autoSave: false,
    })
  }
  return storeInstance
}

/**
 * Initialize the shortcuts store. Must be called before using other functions.
 */
export async function initializeShortcuts(): Promise<void> {
  if (initialized) {
    log.debug('Shortcuts already initialized, skipping')
    return
  }

  log.debug('Initializing shortcuts store')

  const store = await getStore()

  // Check schema version and migrate if needed
  const version = await store.get<number>('_schemaVersion')
  if (version !== undefined && version !== SCHEMA_VERSION) {
    log.info('Migrating shortcuts from version {version}', { version })
    await migrateShortcuts(store, version)
  }

  // Clear in-memory cache and load fresh from store
  customShortcuts.clear()

  // Load custom shortcuts from top-level keys (format: shortcut:commandId)
  // This is similar to how settings-store stores values
  const keys = await store.keys()
  const shortcutKeys = keys.filter((k) => k.startsWith('shortcut:'))

  for (const key of shortcutKeys) {
    const commandId = key.replace('shortcut:', '')
    const shortcuts = await store.get<string[]>(key)
    // An empty array is a real, persisted state ("user removed all shortcuts,
    // don't fall back to defaults") and must load, so we accept any array and
    // only skip non-array garbage. See CLAUDE.md § "Empty array vs missing key".
    if (Array.isArray(shortcuts)) {
      customShortcuts.set(commandId, shortcuts)
    }
  }

  if (customShortcuts.size > 0) {
    log.debug('Loaded {count} {shortcutsNoun}: {ids}', {
      count: customShortcuts.size,
      shortcutsNoun: pluralize(customShortcuts.size, 'custom shortcut'),
      ids: [...customShortcuts.keys()].join(', '),
    })
  } else {
    log.debug('No custom shortcuts found in store')
  }

  initialized = true

  // Notify listeners (reactive shortcut reads, the dispatch map) and sync menu
  // accelerators for every loaded customization. Components can mount before this
  // async init finishes; without the notification they'd keep showing registry
  // defaults until the next manual shortcut change. `notifyListeners` routes through
  // `updateMenuAccelerator`, which no-ops for commands without a menu item.
  for (const commandId of customShortcuts.keys()) {
    notifyListeners(commandId)
  }
}

/**
 * Commands that have corresponding native menu items with accelerators.
 *
 * Exported so tests can verify the list matches the items the platform menu builders register
 * in `src-tauri/src/menu/{macos,linux}.rs`. Any command listed here without a matching menu
 * item silently drops the `updateMenuAccelerator` call on the Rust side (no-op lookup), and
 * any menu item not listed here misses out on accelerator sync from custom shortcuts.
 */
export const menuCommands = [
  // View modes (CheckMenuItems, special handling in Rust)
  'view.fullMode',
  'view.briefMode',
  // Zoom (text-size) presets and step
  'view.zoom.set75',
  'view.zoom.set100',
  'view.zoom.set125',
  'view.zoom.set150',
  'view.zoom.in',
  'view.zoom.out',
  // App-level
  'app.commandPalette',
  'app.settings',
  'app.checkForUpdates',
  // File operations
  'file.view',
  'file.edit',
  'file.copy',
  'file.move',
  'file.newFolder',
  'file.delete',
  'file.deletePermanently',
  'file.rename',
  'file.showInFinder',
  'file.getInfo',
  'file.quickLook',
  'file.copyPath',
  'file.copyFilename',
  // Cloud actions (macOS File Provider, items only show when the right-clicked file is in a cloud folder)
  'cloud.makeOffline',
  'cloud.removeDownload',
  // Selection
  'selection.selectAll',
  'selection.deselectAll',
  'selection.selectFiles',
  'selection.deselectFiles',
  // Panes
  'pane.switch',
  'pane.swap',
  // Search
  'search.open',
  // Sort
  'sort.byName',
  'sort.byExtension',
  'sort.byModified',
  'sort.bySize',
  // Navigation
  'nav.back',
  'nav.forward',
  'nav.parent',
  'nav.goToPath',
  // Downloads
  'downloads.goToLatest',
  // Tabs
  'tab.new',
  'tab.close',
  'tab.reopen',
  'tab.next',
  'tab.prev',
  'tab.closeOthers',
]

/**
 * Force save immediately. Should be called before window/page unload.
 * Note: Since shortcuts now save immediately (no debouncing), this is a no-op
 * but kept for API compatibility.
 */
export function flushPendingSave(): Promise<void> {
  // Shortcuts save immediately now, so nothing to flush
  log.debug('flushPendingSave called (no-op since saves are immediate)')
  return Promise.resolve()
}

/**
 * Migrate shortcuts from older schema versions.
 */
async function migrateShortcuts(store: Store, fromVersion: number): Promise<void> {
  // Currently no migrations needed
  if (fromVersion < 1) {
    // Future migrations would go here
  }

  await store.set('_schemaVersion', SCHEMA_VERSION)
  await store.save()
}

// ============================================================================
// Core API
// ============================================================================

/**
 * Get effective shortcuts for a command (custom if set, otherwise defaults).
 * Always returns a copy to prevent mutation of the original arrays.
 */
export function getEffectiveShortcuts(commandId: string): string[] {
  const custom = customShortcuts.get(commandId)
  if (custom) {
    return [...custom]
  }

  // Defaults are stored in macOS format; convert to current platform
  const command = commands.find((c) => c.id === commandId)
  return (command?.shortcuts ?? []).map(toPlatformShortcut)
}

/**
 * Get default shortcuts for a command from the registry.
 * Always returns a copy to prevent mutation of the original arrays.
 */
export function getDefaultShortcuts(commandId: string): string[] {
  const command = commands.find((c) => c.id === commandId)
  // Defaults are stored in macOS format; convert to current platform
  return (command?.shortcuts ?? []).map(toPlatformShortcut)
}

/**
 * Check if a command's shortcuts have been modified from defaults.
 */
export function isShortcutModified(commandId: string): boolean {
  return customShortcuts.has(commandId)
}

/**
 * Check if shortcuts array matches defaults and clean up if so.
 */
function cleanupIfMatchesDefaults(commandId: string): void {
  const current = customShortcuts.get(commandId)
  if (!current) return

  const defaults = getDefaultShortcuts(commandId)

  // Check if they match (same length and same values in same order)
  const matches = current.length === defaults.length && current.every((shortcut, i) => shortcut === defaults[i])

  if (matches) {
    customShortcuts.delete(commandId)
  }
}

/**
 * Set a specific shortcut for a command at an index.
 */
export function setShortcut(commandId: string, index: number, shortcut: string): void {
  log.debug('setShortcut({commandId}, {index}, {shortcut})', { commandId, index, shortcut })
  const current = getEffectiveShortcuts(commandId)

  if (index >= 0 && index < current.length) {
    current[index] = shortcut
  } else if (index === current.length) {
    current.push(shortcut)
  }

  customShortcuts.set(commandId, current)
  cleanupIfMatchesDefaults(commandId)
  // Save immediately (no debounce) since shortcut changes are rare user actions
  // and we need persistence before the Settings window might close
  void saveToStore()
  notifyListeners(commandId)
}

/**
 * Add a new shortcut to a command.
 */
export function addShortcut(commandId: string, shortcut: string): void {
  const current = getEffectiveShortcuts(commandId)
  current.push(shortcut)
  customShortcuts.set(commandId, current)
  cleanupIfMatchesDefaults(commandId)
  // Save immediately for reliable persistence
  void saveToStore()
  notifyListeners(commandId)
}

/**
 * Remove a shortcut from a command at an index.
 */
export function removeShortcut(commandId: string, index: number): void {
  const current = getEffectiveShortcuts(commandId)

  if (index >= 0 && index < current.length) {
    current.splice(index, 1)
    customShortcuts.set(commandId, current)
    cleanupIfMatchesDefaults(commandId)
    // Save immediately for reliable persistence
    void saveToStore()
    notifyListeners(commandId)
  }
}

/**
 * Reset a command's shortcuts to defaults.
 */
export function resetShortcut(commandId: string): void {
  if (customShortcuts.has(commandId)) {
    customShortcuts.delete(commandId)
    // Save immediately for reliable persistence
    void saveToStore()
    notifyListeners(commandId)
  }
}

/**
 * Reset all shortcuts to defaults.
 */
export async function resetAllShortcuts(): Promise<void> {
  const modifiedIds = [...customShortcuts.keys()]

  // Clear all customizations. With the map empty, saveToStore's reconcile step
  // deletes every stale `shortcut:*` key from disk, so we don't duplicate the
  // delete-loop here.
  customShortcuts.clear()
  await saveToStore()

  // Notify listeners for all modified commands
  for (const id of modifiedIds) {
    notifyListeners(id)
  }
}

// ============================================================================
// Persistence
// ============================================================================

async function saveToStore(): Promise<void> {
  try {
    const store = await getStore()
    log.debug('Saving shortcuts: {shortcuts}', { shortcuts: JSON.stringify(Object.fromEntries(customShortcuts)) })

    // Reconcile disk with the in-memory map so the save reflects the full current
    // state, not just additions. Delete every `shortcut:*` key that no longer has
    // a map entry: when `resetShortcut` or `cleanupIfMatchesDefaults` drops an
    // entry, the matching disk key must go too, or it resurrects on next load.
    const keys = await store.keys()
    for (const key of keys) {
      if (key.startsWith('shortcut:') && !customShortcuts.has(key.replace('shortcut:', ''))) {
        await store.delete(key)
      }
    }

    // Store each command's shortcuts at top level (like settings-store does)
    // This avoids potential issues with nested objects
    for (const [commandId, shortcuts] of customShortcuts) {
      await store.set(`shortcut:${commandId}`, shortcuts)
    }
    await store.set('_schemaVersion', SCHEMA_VERSION)
    await store.save()
    log.debug('Shortcuts saved successfully')
  } catch (error) {
    log.error('Failed to save shortcuts: {error}', { error })
  }
}

// ============================================================================
// Change listeners
// ============================================================================

type ShortcutChangeListener = (commandId: string) => void

const listeners = new Set<ShortcutChangeListener>()

/**
 * Subscribe to shortcut changes.
 */
export function onShortcutChange(listener: ShortcutChangeListener): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

function notifyListeners(commandId: string): void {
  for (const listener of listeners) {
    try {
      listener(commandId)
    } catch (error) {
      log.error('Shortcut change listener error: {error}', { error })
    }
  }

  // Update menu accelerator for commands that have menu items
  void updateMenuAccelerator(commandId)
}

/**
 * Update the menu accelerator for a command.
 * Called automatically when shortcuts change.
 * Only affects commands that have corresponding menu items.
 */
async function updateMenuAccelerator(commandId: string): Promise<void> {
  if (!menuCommands.includes(commandId)) return

  try {
    const shortcuts = getEffectiveShortcuts(commandId)
    // Use the first shortcut for the menu accelerator (menus only show one)
    const shortcut = shortcuts[0] ?? ''
    const res = await ipcCommands.updateMenuAccelerator(commandId, shortcut)
    if (res.status === 'error') throw new Error(res.error)
  } catch (error) {
    log.error('Failed to update menu accelerator: {error}', { error })
  }
}
