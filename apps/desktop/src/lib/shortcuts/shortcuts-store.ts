/**
 * Shortcuts persistence layer - stores custom keyboard shortcuts.
 */

import { load, type Store } from '@tauri-apps/plugin-store'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { commands as ipcCommands } from '$lib/ipc/bindings'
import { commands, FIXED_KEY_COMMAND_IDS, NATIVE_SHORTCUT_COMMAND_IDS } from '$lib/commands/command-registry'
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
// Cross-window propagation
// ============================================================================

// Event name for cross-window shortcut changes. Mirrors settings-store's
// `settings:changed` pattern (see `$lib/settings/settings-store.ts`).
const SHORTCUTS_CHANGED_EVENT = 'shortcuts:changed'

// A per-window id stamped on every emit. The receiving listener drops any event
// whose `senderId` matches its own, so the originating window doesn't re-apply
// (and re-emit) its own change. Unlike settings-store — which dedupes via a
// strict-equality idempotency guard on the cached value — shortcut payloads are
// arrays that arrive as fresh references, so there's nothing to compare by
// identity; an explicit sender id is the clean loop guard here.
const SENDER_ID = crypto.randomUUID()

interface ShortcutsChangedPayload {
  senderId: string
  // Present for a single-command change. `shortcuts` is the new custom list, or
  // `null` when the command reverted to its registry default (no custom entry).
  commandId?: string
  shortcuts?: string[] | null
  // Present for a reset-all broadcast: clear every local customization.
  resetAll?: boolean
}

let crossWindowUnlisten: UnlistenFn | null = null

/** Broadcast a single-command change to other windows. */
function emitShortcutChange(commandId: string): void {
  // `customShortcuts.get` returns undefined once the command reverted to default
  // (cleanup/reset dropped the entry); send `null` so receivers clear their own.
  const shortcuts = customShortcuts.get(commandId) ?? null
  void emit(SHORTCUTS_CHANGED_EVENT, {
    senderId: SENDER_ID,
    commandId,
    shortcuts: shortcuts ? [...shortcuts] : null,
  } satisfies ShortcutsChangedPayload)
}

/** Broadcast a reset-all to other windows. */
function emitResetAll(): void {
  void emit(SHORTCUTS_CHANGED_EVENT, { senderId: SENDER_ID, resetAll: true } satisfies ShortcutsChangedPayload)
}

/**
 * Install the cross-window listener. Called once from `initializeShortcuts`.
 * Re-init is guarded both by `initialized` and by `crossWindowUnlisten`, so a
 * window never double-subscribes.
 *
 * On a remote change it updates the local `customShortcuts` map directly and
 * fires `notifyListeners` so reactive consumers (chips, F-key bar, palette) and
 * the dispatch map rebuild. It deliberately does NOT save to disk (the writer
 * window already persisted) and does NOT re-emit (that would loop).
 */
async function setupCrossWindowListener(): Promise<void> {
  if (crossWindowUnlisten) return // already listening

  crossWindowUnlisten = await listen<ShortcutsChangedPayload>(SHORTCUTS_CHANGED_EVENT, (event) => {
    const { senderId, commandId, shortcuts, resetAll } = event.payload
    if (senderId === SENDER_ID) return // our own broadcast; ignore (loop guard)

    if (resetAll) {
      // Capture affected ids before clearing so we can notify each.
      const affected = [...customShortcuts.keys()]
      customShortcuts.clear()
      for (const id of affected) {
        notifyListeners(id)
      }
      return
    }

    if (commandId === undefined) return

    if (shortcuts == null) {
      // Remote reverted this command to its default (`null`, or a malformed
      // single-command payload missing `shortcuts`). `==` catches both null and
      // undefined; an empty array stays truthy here, so the "removed all
      // shortcuts" state is preserved as `[]`, not collapsed to a reset.
      customShortcuts.delete(commandId)
    } else {
      customShortcuts.set(commandId, [...shortcuts])
    }
    notifyListeners(commandId)
  })
}

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

    // Drop any persisted customization for a macOS-native command. AppKit owns
    // these accelerators, so the entry is a no-op illusion (David's dev
    // shortcuts.json carries `app.hide: []` from testing). Not loading it leaves
    // the map without an entry, so the registry default applies and the next
    // save reconciles the stale disk key away.
    if (isNativeShortcutCommand(commandId) || isFixedKeyCommand(commandId)) continue

    const shortcuts = await store.get<string[]>(key)
    if (!Array.isArray(shortcuts)) continue // skip non-array garbage

    // Heal leaked empty-string entries. A `''` is never a real shortcut; it's
    // junk an older "+ add" flow could persist when the user clicked away from a
    // half-started add. Drop every `''`, but keep the distinction that matters:
    //   - a genuine `[]` (length 0) is "user removed all shortcuts" — load it.
    //   - a non-empty array that's ALL `''` heals to empty, which we must NOT
    //     store as `[]` (that would wrongly suppress a default-bound command's
    //     defaults); skip the entry entirely so the registry default applies.
    //   - `['⌘X', '']` heals to `['⌘X']`.
    // See CLAUDE.md § "Empty array vs missing key".
    if (shortcuts.length === 0) {
      customShortcuts.set(commandId, []) // real removed-all state
      continue
    }
    const healed = shortcuts.filter((s) => s !== '')
    if (healed.length === 0) continue // was all-'' junk; fall back to default
    customShortcuts.set(commandId, healed)
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

  // Listen for cross-window shortcut changes (Settings ↔ main window). Installed
  // once per window; `setupCrossWindowListener` is a no-op if already subscribed.
  await setupCrossWindowListener()

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

// Fast-lookup set for the macOS-native commands. AppKit owns both their behavior
// and accelerator (PredefinedMenuItems), so a persisted customization is a pure
// illusion: it can't disable the OS accelerator and can't dispatch anything.
const nativeShortcutIds = new Set<string>(NATIVE_SHORTCUT_COMMAND_IDS)

// Fast-lookup set for the fixed-key commands. Their keys are hardcoded in the
// owning component's keydown handler and never consult this store, so a
// customization would be a no-op illusion (new key dead, built-in key still live).
const fixedKeyIds = new Set<string>(FIXED_KEY_COMMAND_IDS)

/**
 * Whether this command's key is hardcoded in its component (Family-2/3 fixed-key
 * command). The editor renders these read-only; the mutators below refuse to
 * write them.
 */
export function isFixedKeyCommand(commandId: string): boolean {
  return fixedKeyIds.has(commandId)
}

/**
 * Whether macOS owns this command's shortcut outright (Family-1 native command).
 * The editor renders these read-only; the mutators below refuse to write them.
 */
export function isNativeShortcutCommand(commandId: string): boolean {
  return nativeShortcutIds.has(commandId)
}

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
  if (isNativeShortcutCommand(commandId) || isFixedKeyCommand(commandId)) {
    log.warn(
      'Refusing to set shortcut for non-rebindable command {commandId}: the key is owned by macOS or hardcoded in its component',
      { commandId },
    )
    return
  }
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
  // Broadcast AFTER updating local state so other windows catch up live.
  emitShortcutChange(commandId)
}

/**
 * Add a new shortcut to a command.
 */
export function addShortcut(commandId: string, shortcut: string): void {
  if (isNativeShortcutCommand(commandId) || isFixedKeyCommand(commandId)) {
    log.warn(
      'Refusing to add shortcut for non-rebindable command {commandId}: the key is owned by macOS or hardcoded in its component',
      { commandId },
    )
    return
  }
  const current = getEffectiveShortcuts(commandId)
  current.push(shortcut)
  customShortcuts.set(commandId, current)
  cleanupIfMatchesDefaults(commandId)
  // Save immediately for reliable persistence
  void saveToStore()
  notifyListeners(commandId)
  emitShortcutChange(commandId)
}

/**
 * Remove a shortcut from a command at an index.
 */
export function removeShortcut(commandId: string, index: number): void {
  if (isNativeShortcutCommand(commandId) || isFixedKeyCommand(commandId)) {
    log.warn(
      'Refusing to remove shortcut for non-rebindable command {commandId}: the key is owned by macOS or hardcoded in its component',
      { commandId },
    )
    return
  }
  const current = getEffectiveShortcuts(commandId)

  if (index >= 0 && index < current.length) {
    current.splice(index, 1)
    customShortcuts.set(commandId, current)
    cleanupIfMatchesDefaults(commandId)
    // Save immediately for reliable persistence
    void saveToStore()
    notifyListeners(commandId)
    emitShortcutChange(commandId)
  }
}

/**
 * Reset a command's shortcuts to defaults. Stays permissive for native commands:
 * it only ever DELETES a custom entry (it never writes the illusion), so letting
 * it clear a leaked native customization is safe and useful.
 */
export function resetShortcut(commandId: string): void {
  if (customShortcuts.has(commandId)) {
    customShortcuts.delete(commandId)
    // Save immediately for reliable persistence
    void saveToStore()
    notifyListeners(commandId)
    emitShortcutChange(commandId)
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

  // Broadcast a single reset-all marker; receivers clear their whole map.
  emitResetAll()
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
