/**
 * Shortcuts persistence layer - stores custom keyboard shortcuts.
 */

import { load, type Store } from '@tauri-apps/plugin-store'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  getRestrictedWindowShortcuts,
  updateMenuAccelerator as updateMenuAcceleratorCommand,
} from '#lib/tauri-commands/index.js'
import { commands, defaultShortcuts, FIXED_KEY_COMMAND_IDS, ownedByAppMenu } from '#lib/commands/command-registry.js'
import { resolveStorePath } from '#lib/settings/store-path.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { pluralize } from '#lib/utils/pluralize.js'
import { toCanonicalShortcut, typingRefusal } from './key-capture'
import { firstLiveShortcut, isShadowedDefault, liveShortcuts, type BindingReader } from './binding-priority'

const log = getAppLogger('shortcuts')

// ============================================================================
// Store configuration
// ============================================================================

const STORE_NAME = 'shortcuts.json'
const SCHEMA_VERSION = 1

let storeInstance: Store | null = null

// In-memory cache of custom shortcuts
const customShortcuts = new Map<string, string[]>()
// Commands THIS window rebound or reset since its last save: the patch
// `performSave` applies. Windows share one in-memory plugin store per file, and a
// window that loaded before another's rebind (it listens only after loading)
// never hears it, so saving its whole map would delete or roll back a binding it
// never saw. A change heard from another window stays out: that window saved it.
const changedHere = new Set<string>()
// A reset-all from this window: its save deletes every `shortcut:*` key.
let resetAllHere = false
let initialized = false

// ============================================================================
// Cross-window propagation
// ============================================================================

// Event name for cross-window shortcut changes. Mirrors settings-store's
// `settings:changed` pattern (see `#lib/settings/settings-store.ts`).
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
      log.warn('Could not resolve isolated shortcuts path, using default: {error}', { error: e })
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
 *
 * `restrictedWindow` is for a window with no `tauri-plugin-store` access (the viewer): it
 * seeds from the typed `get_restricted_window_shortcuts` snapshot instead and follows
 * rebinds through `shortcuts:changed` like any window. It only READS: rebinding happens in
 * Settings, so the mutators are never called there (a save would have no store to write).
 */
export async function initializeShortcuts(options: { restrictedWindow?: boolean } = {}): Promise<void> {
  if (initialized) {
    log.debug('Shortcuts already initialized, skipping')
    return
  }

  log.debug('Initializing shortcuts store')

  const entries = options.restrictedWindow ? await readRestrictedSnapshot() : await readStoreEntries()

  // Clear in-memory cache and load fresh
  customShortcuts.clear()
  for (const [commandId, shortcuts] of entries) seedCustomShortcut(commandId, shortcuts)
  if (!options.restrictedWindow) await writeBackHealedEntries(entries)

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

/** Every `shortcut:<commandId>` entry in the store, as `[commandId, raw value]`, after any schema migration. */
async function readStoreEntries(): Promise<[string, unknown][]> {
  const store = await getStore()

  // Check schema version and migrate if needed
  const version = await store.get<number>('_schemaVersion')
  if (version !== undefined && version !== SCHEMA_VERSION) {
    log.info('Migrating shortcuts from version {version}', { version })
    await migrateShortcuts(store, version)
  }

  // Custom shortcuts live in top-level keys (format: shortcut:commandId), like settings-store's values.
  const shortcutKeys = (await store.keys()).filter((k) => k.startsWith('shortcut:'))
  return Promise.all(
    shortcutKeys.map(async (key): Promise<[string, unknown]> => [key.replace('shortcut:', ''), await store.get(key)]),
  )
}

/**
 * Puts what load healing changed (a dropped native or junk entry, a canonicalized
 * combo) back into the shared store at load, saved by the next save of any window.
 * Not deferred to this window's next save, which writes only what it changed.
 */
async function writeBackHealedEntries(entries: [string, unknown][]): Promise<void> {
  const store = await getStore()
  for (const [commandId, raw] of entries) {
    const healed = customShortcuts.get(commandId)
    if (healed === undefined) await store.delete(`shortcut:${commandId}`)
    else if (JSON.stringify(healed) !== JSON.stringify(raw)) await store.set(`shortcut:${commandId}`, healed)
  }
}

/**
 * A restricted window's customizations, from the backend's read of `shortcuts.json`. A failed
 * read leaves the registry defaults in force, at warn level: an error-level log here would
 * file an error report on every viewer open (the reason restricted settings warn too).
 */
async function readRestrictedSnapshot(): Promise<[string, unknown][]> {
  try {
    return Object.entries(await getRestrictedWindowShortcuts())
  } catch (error) {
    log.warn('Could not read custom shortcuts, using defaults: {error}', { error })
    return []
  }
}

/** Loads one persisted customization into the map, healed. Both init paths come through here. */
function seedCustomShortcut(commandId: string, shortcuts: unknown): void {
  // Drop any persisted customization for a command the app menu owns on this OS
  // (macOS). AppKit owns these accelerators, so the entry is a no-op illusion (David's dev
  // shortcuts.json carries `app.hide: []` from testing). Not loading it leaves
  // the map without an entry, so the registry default applies, and
  // `writeBackHealedEntries` deletes the stale disk key.
  if (isNativeShortcutCommand(commandId) || isFixedKeyCommand(commandId)) return
  if (!Array.isArray(shortcuts)) return // skip non-array garbage

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
    return
  }
  const healed = shortcuts.filter((s): s is string => typeof s === 'string' && s !== '')
  if (healed.length === 0) return // was all-'' junk; fall back to default
  // Bring display-form key names back to the canonical vocabulary (`⌘⌫` →
  // `⌘Backspace`). A file written before the two spellings were unified would
  // otherwise hold a binding no keypress can produce and no menu accelerator
  // can parse. See `key-capture.ts` for the canonical-vs-display split. Healing
  // can fold two entries into one (`['⇧8', '*']` → `*` twice), so dedupe.
  customShortcuts.set(commandId, [...new Set(healed.map(toCanonicalShortcut))])
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
  // View > Full screen (Windows and Linux; macOS's is AppKit's own item, so nothing syncs there)
  'view.toggleFullScreen',
  // File > Quit / Exit Cmdr (Windows and Linux; macOS quits from AppKit's app menu)
  'app.quit',
  // App-level
  'app.commandPalette',
  'app.settings',
  'app.checkForUpdates',
  'cmdr.openOnboarding',
  'app.toggleCliCommand',
  // Help (registered in MenuState.items so a future custom binding syncs its accelerator)
  'help.whatsNew',
  'help.viewDebugLog',
  'help.accessibility',
  // View > Operation queue (default ⌥⌘Q; registered so a custom binding syncs its accelerator)
  'queue.show',
  // View > Operation log (default ⌥⌘L; registered so a custom binding syncs its accelerator)
  'log.operationLog',
  // View > Suggested ops (no default; registered so a binding the user adds syncs its accelerator)
  'suggestedOps.show',
  // View > Ask Cmdr (default ⌥⌘A; registered so a custom binding syncs its accelerator)
  'askCmdr.toggle',
  // Edit menu (custom MenuItems; the focus-routed dispatch exception in
  // handle_menu_event doesn't change that their accelerators sync generically)
  'edit.cut',
  'edit.copy',
  'edit.paste',
  'edit.pasteAsMove',
  // File operations
  'file.view',
  'file.edit',
  'file.copy',
  'file.move',
  'file.duplicate',
  'file.compress',
  'file.newFolder',
  'file.newFile',
  'file.delete',
  'file.deletePermanently',
  'file.rename',
  'file.multiRename',
  'file.showInFinder',
  'file.openTerminalHere',
  'file.getInfo',
  'file.quickLook',
  'file.copyPath',
  'file.copyFilename',
  'edit.undoTrash',
  'file.copyShareLink',
  'file.copyShareLinkOneDay',
  'file.copyShareLinkOneHour',
  // Cloud actions (macOS File Provider, items only show when the right-clicked file is in a cloud folder)
  'cloud.makeOffline',
  'cloud.removeDownload',
  // Google Drive (items only show when the right-clicked item resolves to a Drive ID)
  'cloud.openInGoogleDrive',
  'cloud.copyGoogleDriveLink',
  'cloud.askGemini',
  // Selection
  'selection.selectAll',
  'selection.deselectAll',
  'selection.selectSameKind',
  'selection.compareDirectories',
  'selection.invert',
  'selection.selectFiles',
  'selection.deselectFiles',
  // Panes
  'pane.switch',
  'pane.swap',
  'pane.clone',
  'view.calculateFolderSizes',
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
  'nav.goToRoot',
  'nav.goHome',
  'nav.goToPath',
  // Downloads
  'downloads.goToLatest',
  // Favorites
  'favorites.add',
  'favorites.open',
  // Servers
  'servers.connect',
  'servers.show',
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
 * Whether the OS's app menu owns this command's shortcut outright on this OS
 * (`ownedByAppMenu`: one of the four native commands, on macOS). AppKit owns both
 * the behavior and the accelerator there (PredefinedMenuItems), so a customization
 * would be a pure illusion: the editor renders these read-only, the mutators below
 * refuse to write them, and a saved one is dropped at load. Off macOS Quit is an
 * ordinary, rebindable command.
 */
const commandById = new Map<string, (typeof commands)[number]>(commands.map((command) => [command.id, command]))

export function isNativeShortcutCommand(commandId: string): boolean {
  const command = commandById.get(commandId)
  return command !== undefined && ownedByAppMenu(command)
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

  return [...defaultShortcuts(commandId)]
}

/**
 * Get default shortcuts for a command from the registry.
 * Always returns a copy to prevent mutation of the original arrays.
 */
export function getDefaultShortcuts(commandId: string): string[] {
  return [...defaultShortcuts(commandId)]
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

/** A combo typing takes first never reaches its command, so no writer may bind one. */
function refusesTypedCombo(commandId: string, shortcut: string): boolean {
  const scope = commands.find((c) => c.id === commandId)?.scope
  if (!scope || !typingRefusal(shortcut, scope)) return false
  log.warn('Refusing to bind {shortcut} to {commandId}: typing takes that key first', { shortcut, commandId })
  return true
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
  if (refusesTypedCombo(commandId, shortcut)) return
  log.debug('setShortcut({commandId}, {index}, {shortcut})', { commandId, index, shortcut })
  // Canonicalize at the store boundary so every writer (the Settings capture, MCP
  // events, any future caller) lands one vocabulary in `shortcuts.json`.
  const canonical = toCanonicalShortcut(shortcut)
  const current = getEffectiveShortcuts(commandId)

  if (index >= 0 && index < current.length) {
    current[index] = canonical
  } else if (index === current.length) {
    current.push(canonical)
  }

  customShortcuts.set(commandId, current)
  cleanupIfMatchesDefaults(commandId)
  changedHere.add(commandId)
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
  if (refusesTypedCombo(commandId, shortcut)) return
  const current = getEffectiveShortcuts(commandId)
  current.push(toCanonicalShortcut(shortcut)) // canonical at the boundary, see `setShortcut`
  customShortcuts.set(commandId, current)
  cleanupIfMatchesDefaults(commandId)
  changedHere.add(commandId)
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
    changedHere.add(commandId)
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
    changedHere.add(commandId)
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

  // Clear all customizations; the save deletes every `shortcut:*` key on disk,
  // including ones another window wrote that this one never heard of: it's a reset.
  customShortcuts.clear()
  resetAllHere = true
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

// Saves are fire-and-forget from every mutator (`void saveToStore()`), so two
// rapid mutations used to run their delete-set-save loops interleaved over the
// same store. Chaining serializes them: each save starts only after the previous
// one finished, and takes the pending changes at its own start.
let saveChain: Promise<void> = Promise.resolve()

function saveToStore(): Promise<void> {
  saveChain = saveChain.then(performSave)
  return saveChain
}

/**
 * Applies this window's changes to the shared store and saves it: a command with
 * a map entry is written (an empty array is "removed all", a real state), one
 * without is deleted so the registry default applies. Commands this window didn't
 * change aren't touched. A failed save puts its changes back for the next one.
 */
async function performSave(): Promise<void> {
  const patch = [...changedHere]
  const resetAll = resetAllHere
  changedHere.clear()
  resetAllHere = false
  try {
    const store = await getStore()
    log.debug('Saving shortcuts for {ids}', { ids: patch.join(', ') })

    if (resetAll) {
      for (const key of await store.keys()) {
        if (key.startsWith('shortcut:') && !customShortcuts.has(key.replace('shortcut:', ''))) await store.delete(key)
      }
    }
    for (const commandId of patch) {
      const shortcuts = customShortcuts.get(commandId)
      if (shortcuts === undefined) await store.delete(`shortcut:${commandId}`)
      else await store.set(`shortcut:${commandId}`, shortcuts)
    }
    await store.set('_schemaVersion', SCHEMA_VERSION)
    await store.save()
    log.debug('Shortcuts saved successfully')
  } catch (error) {
    log.error('Failed to save shortcuts: {error}', { error })
    for (const commandId of patch) changedHere.add(commandId)
    resetAllHere ||= resetAll
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

/** Every command with a custom entry, for the user-binding rule (`binding-priority.ts`). */
export function getCustomizedCommandIds(): string[] {
  return [...customShortcuts.keys()]
}

const bindingReader: BindingReader = {
  effective: getEffectiveShortcuts,
  defaults: getDefaultShortcuts,
  customizedIds: () => customShortcuts.keys(),
  isRebindable: (commandId) => !isNativeShortcutCommand(commandId) && !isFixedKeyCommand(commandId),
}

/**
 * The one key a menu shows for a command: its first effective shortcut that no user
 * binding of another command shadows (`binding-priority.ts`), or `''`. A shadowed
 * default runs the other command, so a menu registering or showing it would lie (and on
 * macOS a registered one would fire this command first).
 */
export function getMenuShortcut(commandId: string): string {
  return firstLiveShortcut(bindingReader, commandId) ?? ''
}

/**
 * The keys that run a command: its effective shortcuts minus any default another command's user
 * binding shadows (`binding-priority.ts`). What every hint shows (chips, tooltips, the palette,
 * the `{combo}` in a message), so none names a key that runs something else. A fresh array.
 */
export function getLiveShortcuts(commandId: string): string[] {
  return liveShortcuts(bindingReader, commandId)
}

/**
 * True when `combo` is one of `commandId`'s defaults that another command's user binding shadows
 * (`binding-priority.ts`), so the keypress means that other command. The dispatcher's reader.
 */
export function isShadowed(commandId: string, combo: string): boolean {
  return isShadowedDefault(bindingReader, commandId, combo)
}

// What each menu item was last told. A never-told item still carries the key the bar
// was built with, which is its command's first default.
const pushedMenuShortcut = new Map<string, string>()

function menuShortcutNow(commandId: string): string {
  return pushedMenuShortcut.get(commandId) ?? getDefaultShortcuts(commandId).at(0) ?? ''
}

/**
 * Pushes the menu items that differ from what the bar was built with back onto the
 * native menu, for after the menu bar has been rebuilt in a new language.
 *
 * A rebuild replaces every menu item, so every item is back at its registry default on
 * a brand-new object. The customized ones need re-pushing, and so does any item whose
 * default another command's user binding shadows.
 */
export function resyncMenuAccelerators(): void {
  pushedMenuShortcut.clear()
  for (const commandId of menuCommands) {
    if (customShortcuts.has(commandId) || getMenuShortcut(commandId) !== menuShortcutNow(commandId)) {
      void updateMenuAccelerator(commandId)
    }
  }
}

function notifyListeners(commandId: string): void {
  for (const listener of listeners) {
    try {
      listener(commandId)
    } catch (error) {
      log.error('Shortcut change listener error: {error}', { error })
    }
  }

  // The changed command's own menu item, plus any other item whose key this change
  // shadowed or freed (a user binding on one command moves another's menu key).
  void updateMenuAccelerator(commandId)
  for (const other of menuCommands) {
    if (other !== commandId && getMenuShortcut(other) !== menuShortcutNow(other)) void updateMenuAccelerator(other)
  }
}

/**
 * Update the menu accelerator for a command.
 * Called automatically when shortcuts change.
 * Only affects commands that have corresponding menu items.
 */
async function updateMenuAccelerator(commandId: string): Promise<void> {
  if (!menuCommands.includes(commandId)) return

  try {
    // Menus show one key: the first one that really runs this command.
    const shortcut = getMenuShortcut(commandId)
    pushedMenuShortcut.set(commandId, shortcut)
    await updateMenuAcceleratorCommand(commandId, shortcut)
  } catch (error) {
    log.error('Failed to update menu accelerator: {error}', { error })
  }
}
