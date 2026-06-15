/**
 * Unit tests for the controller extracted from `KeyboardShortcutsSection.svelte`.
 *
 * Uses Svelte runes (`$effect.root`) to instantiate the rune-based factory outside
 * a component, hence the `.svelte.test.ts` suffix. The add/conflict store-mutation
 * flow is pinned end-to-end by `KeyboardShortcutsSection.svelte.test.ts` (real
 * shortcuts store, in-memory disk); here we cover the pure-ish units the DOM test
 * doesn't reach: the key-filter field helpers (platform-aware combo splitting and
 * subset matching), the search/filter derivations, and the capture/conflict
 * branches, with the store + registry mocked for determinism.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// ── Mocked store + registry ──────────────────────────────────────────────────
let macOS = true
const effectiveShortcuts = new Map<string, string[]>()
const modifiedIds = new Set<string>()

const setShortcut = vi.fn<(id: string, index: number, combo: string) => void>()
const addShortcut = vi.fn<(id: string, combo: string) => void>()
const removeShortcut = vi.fn<(id: string, index: number) => void>()
const resetShortcut = vi.fn<(id: string) => void>()
const resetAllShortcuts = vi.fn(() => Promise.resolve())
const findConflictsForShortcut = vi.fn<(combo: string, scope: string, id: string) => unknown[]>(() => [])
const confirmDialog = vi.fn<(message: string, title: string) => Promise<boolean>>(() => Promise.resolve(false))

vi.mock('$lib/shortcuts', () => ({
  getEffectiveShortcuts: (id: string) => effectiveShortcuts.get(id) ?? [],
  isShortcutModified: (id: string) => modifiedIds.has(id),
  setShortcut: (...a: [string, number, string]) => { setShortcut(...a); },
  addShortcut: (...a: [string, string]) => { addShortcut(...a); },
  removeShortcut: (...a: [string, number]) => { removeShortcut(...a); },
  resetShortcut: (...a: [string]) => { resetShortcut(...a); },
  resetAllShortcuts: () => resetAllShortcuts(),
  isMacOS: () => macOS,
  isModifierKey: (key: string) => ['Meta', 'Control', 'Alt', 'Shift'].includes(key),
  // Minimal deterministic combo formatter: prefix held modifiers (mac glyphs), then the key.
  formatKeyCombo: (e: KeyboardEvent) => {
    let s = ''
    if (e.metaKey) s += '⌘'
    if (e.ctrlKey) s += '⌃'
    if (e.altKey) s += '⌥'
    if (e.shiftKey) s += '⇧'
    return s + e.key
  },
  findConflictsForShortcut: (...a: [string, string, string]) => findConflictsForShortcut(...a),
  getConflictingCommandIds: () => new Set<string>(),
  getConflictCount: () => 0,
}))

vi.mock('$lib/commands/command-registry', () => ({
  commands: [
    { id: 'app.about', name: 'About Cmdr', scope: 'App' },
    { id: 'file.copy', name: 'Copy', scope: 'Main window/File list' },
  ],
}))

vi.mock('$lib/commands/fuzzy-search', () => ({
  searchAllCommands: (q: string) =>
    [
      { id: 'app.about', name: 'About Cmdr' },
      { id: 'file.copy', name: 'Copy' },
    ]
      .filter((c) => c.name.toLowerCase().includes(q.toLowerCase()))
      .map((command) => ({ command })),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({
  confirmDialog: (...a: [string, string]) => confirmDialog(...a),
}))

import { createKeyboardShortcutsController } from './KeyboardShortcutsSection.controller.svelte'

let dispose: (() => void) | undefined

function create(searchQuery = '') {
  let controller!: ReturnType<typeof createKeyboardShortcutsController>
  dispose = $effect.root(() => {
    controller = createKeyboardShortcutsController(() => searchQuery)
  })
  return controller
}

/** A keydown-shaped object good enough for the controller's handlers. */
function keyEvent(init: Partial<KeyboardEvent> & { key: string }): KeyboardEvent {
  return {
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    stopImmediatePropagation: vi.fn(),
    ...init,
  } as unknown as KeyboardEvent
}

beforeEach(() => {
  macOS = true
  effectiveShortcuts.clear()
  modifiedIds.clear()
  vi.clearAllMocks()
  findConflictsForShortcut.mockReturnValue([])
})

afterEach(() => {
  dispose?.()
  dispose = undefined
})

describe('key-filter field (macOS)', () => {
  it('builds a glyph combo string on a complete keydown and clears it on Escape', () => {
    const c = create()
    c.handleKeyFilterKeyDown(keyEvent({ key: 'F', metaKey: true }))
    expect(c.keySearchQuery).toBe('⌘F')

    c.handleKeyFilterKeyDown(keyEvent({ key: 'Escape' }))
    expect(c.keySearchQuery).toBe('')
  })

  it('shows held modifiers on keydown then clears them on keyup once all are released', () => {
    const c = create()
    // Only a modifier held: shows the glyph(s).
    c.handleKeyFilterKeyDown(keyEvent({ key: 'Meta', metaKey: true }))
    expect(c.keySearchQuery).toBe('⌘')
    // Modifier released (none still held): the modifiers-only value clears.
    c.handleKeyFilterKeyUp(keyEvent({ key: 'Meta', metaKey: false }))
    expect(c.keySearchQuery).toBe('')
  })

  it('lets Tab through without capturing', () => {
    const c = create()
    c.keySearchQuery = '⌘'
    c.handleKeyFilterKeyDown(keyEvent({ key: 'Tab' }))
    expect(c.keySearchQuery).toBe('⌘')
  })
})

describe('key-filter field (non-macOS)', () => {
  it('shows word-form modifiers on keydown and clears a modifiers-only value on full release', () => {
    macOS = false
    const c = create()
    c.handleKeyFilterKeyDown(keyEvent({ key: 'Control', ctrlKey: true }))
    expect(c.keySearchQuery).toBe('Ctrl')
    // The non-mac regex recognizes a Ctrl/Alt/Shift/Win-only value, so releasing
    // the last modifier clears it.
    c.handleKeyFilterKeyUp(keyEvent({ key: 'Control', ctrlKey: false }))
    expect(c.keySearchQuery).toBe('')
  })
})

describe('filtering', () => {
  it('filters by name search via searchAllCommands', () => {
    const c = create()
    c.localNameSearchQuery = 'copy'
    expect(c.filteredCommands.map((cmd) => cmd.id)).toEqual(['file.copy'])
  })

  it('prefers the global searchQuery prop over the local search', () => {
    const c = create('about')
    // Local is ignored while the prop is non-empty.
    c.localNameSearchQuery = 'copy'
    expect(c.nameSearchQuery).toBe('about')
    expect(c.filteredCommands.map((cmd) => cmd.id)).toEqual(['app.about'])
  })

  it('filters by key search (subset match against effective shortcuts)', () => {
    effectiveShortcuts.set('file.copy', ['⌘C'])
    effectiveShortcuts.set('app.about', ['⌘A'])
    const c = create()
    c.keySearchQuery = '⌘C'
    expect(c.filteredCommands.map((cmd) => cmd.id)).toEqual(['file.copy'])
  })

  it('filters by the Modified chip', () => {
    modifiedIds.add('app.about')
    const c = create()
    c.activeFilter = 'modified'
    expect(c.filteredCommands.map((cmd) => cmd.id)).toEqual(['app.about'])
  })

  it('resetFilters clears name, key, and chip filters', () => {
    const c = create()
    c.localNameSearchQuery = 'copy'
    c.keySearchQuery = '⌘C'
    c.activeFilter = 'conflicts'
    c.resetFilters()
    expect(c.localNameSearchQuery).toBe('')
    expect(c.keySearchQuery).toBe('')
    expect(c.activeFilter).toBe('all')
  })

  it('shows the global go-to-latest row only when relevant', () => {
    const c = create()
    expect(c.showGlobalGoToLatestRow).toBe(true)
    c.activeFilter = 'modified'
    expect(c.showGlobalGoToLatestRow).toBe(false)
  })

  it('groups filtered commands by scope', () => {
    const c = create()
    const scopes = c.groupedCommands.map((g) => g.scope)
    expect(scopes).toContain('App')
    expect(scopes).toContain('Main window/File list')
  })
})

describe('capture + conflict engine', () => {
  it('Backspace on an empty existing slot removes that binding', () => {
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.startEditingShortcut('file.copy', 0)
    c.handleKeyDown(keyEvent({ key: 'Backspace' }))
    expect(removeShortcut).toHaveBeenCalledWith('file.copy', 0)
    expect(c.editingShortcut).toBe(null)
  })

  it('Escape cancels the edit without touching the store', () => {
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.startEditingShortcut('file.copy', 0)
    c.handleKeyDown(keyEvent({ key: 'Escape' }))
    expect(c.editingShortcut).toBe(null)
    expect(removeShortcut).not.toHaveBeenCalled()
  })

  it('capturing a non-conflicting combo saves it after the confirm delay', () => {
    vi.useFakeTimers()
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.startEditingShortcut('file.copy', 0)
    c.handleKeyDown(keyEvent({ key: 'X', metaKey: true }))
    expect(c.pendingKey).toBe('⌘X')
    vi.advanceTimersByTime(500)
    expect(setShortcut).toHaveBeenCalledWith('file.copy', 0, '⌘X')
    vi.useRealTimers()
  })

  it('a normal conflict raises the banner and waits (no auto-save), then Remove-from-other resolves it', () => {
    effectiveShortcuts.set('file.copy', ['F5'])
    effectiveShortcuts.set('app.about', ['⌘X'])
    findConflictsForShortcut.mockReturnValue([{ id: 'app.about', name: 'About Cmdr', scope: 'App' }])
    const c = create()
    c.startEditingShortcut('file.copy', 0)
    c.handleKeyDown(keyEvent({ key: 'X', metaKey: true }))
    expect(c.conflictWarning).not.toBe(null)
    expect(setShortcut).not.toHaveBeenCalled()

    c.handleRemoveFromOther()
    // Removes the combo from the other command, then saves ours.
    expect(removeShortcut).toHaveBeenCalledWith('app.about', 0)
    expect(setShortcut).toHaveBeenCalledWith('file.copy', 0, '⌘X')
  })

  it('handleAddShortcut targets one-past-the-end and never writes a placeholder', () => {
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.handleAddShortcut('file.copy')
    expect(c.editingShortcut).toEqual({ commandId: 'file.copy', index: 1 })
    expect(c.isAddingNewShortcut).toBe(true)
    expect(addShortcut).not.toHaveBeenCalled()
  })

  it('handleResetAll confirms before resetting', async () => {
    confirmDialog.mockResolvedValueOnce(true)
    const c = create()
    await c.handleResetAll()
    expect(resetAllShortcuts).toHaveBeenCalledOnce()
  })
})
