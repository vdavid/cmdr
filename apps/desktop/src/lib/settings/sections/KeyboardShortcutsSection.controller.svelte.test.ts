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
import { describe, it, expect, vi, beforeEach, afterEach, onTestFinished } from 'vitest'
import { withCapabilities, withPlatform } from '#lib/platform/test-platform.js'

// ── Mocked store + registry ──────────────────────────────────────────────────
const effectiveShortcuts = new Map<string, string[]>()
const modifiedIds = new Set<string>()

const setShortcut = vi.fn<(payload: { commandId: string; index: number; shortcut: string }) => void>()
const addShortcut = vi.fn<(payload: { commandId: string; shortcut: string }) => void>()
const removeShortcut = vi.fn<(id: string, index: number) => void>()
const resetShortcut = vi.fn<(id: string) => void>()
const resetAllShortcuts = vi.fn(() => Promise.resolve())
const findConflictsForShortcut = vi.fn<
  (payload: { shortcut: string; scope: string; excludeCommandId: string }) => unknown[]
>(() => [])
const confirmDialog = vi.fn<(payload: { message: string; title: string }) => Promise<boolean>>(() =>
  Promise.resolve(false),
)

vi.mock('#lib/shortcuts/index.js', async () => ({
  getEffectiveShortcuts: (id: string) => effectiveShortcuts.get(id) ?? [],
  isShortcutModified: (id: string) => modifiedIds.has(id),
  setShortcut: (commandId: string, index: number, shortcut: string) => {
    setShortcut({ commandId, index, shortcut })
  },
  addShortcut: (commandId: string, shortcut: string) => {
    addShortcut({ commandId, shortcut })
  },
  removeShortcut: (...a: [string, number]) => {
    removeShortcut(...a)
  },
  resetShortcut: (...a: [string]) => {
    resetShortcut(...a)
  },
  resetAllShortcuts: () => resetAllShortcuts(),
  isModifierKey: (key: string) => ['Meta', 'Control', 'Alt', 'Shift'].includes(key),
  // The REAL capture helper: what the field records is the vocabulary contract
  // itself (typed symbols by character, retyped ⌥ keys by position), and a
  // hand-copied fake here would drift from it. It follows the platform each test sets.
  capturedKeyCombo: (
    await vi.importActual<typeof import('#lib/shortcuts/key-capture.js')>('#lib/shortcuts/key-capture.js')
  ).capturedKeyCombo,
  typingRefusal: (
    await vi.importActual<typeof import('#lib/shortcuts/key-capture.js')>('#lib/shortcuts/key-capture.js')
  ).typingRefusal,
  // The real token writer too: the live modifier value must spell what a stored combo holds.
  formatChord: (await vi.importActual<typeof import('#lib/shortcuts/key-capture.js')>('#lib/shortcuts/key-capture.js'))
    .formatChord,
  findConflictsForShortcut: (shortcut: string, scope: string, excludeCommandId: string) =>
    findConflictsForShortcut({ shortcut, scope, excludeCommandId }),
  getConflictingCommandIds: () => new Set<string>(),
  getConflictCount: () => 0,
}))

vi.mock('#lib/commands/command-registry.js', () => {
  const commands = [
    { id: 'app.about', name: 'About Cmdr', scope: 'App' },
    { id: 'file.copy', name: 'Copy', scope: 'Main window/File list' },
  ]
  // Neither command is one the app menu owns.
  return { commands, listedCommands: () => commands, ownedByAppMenu: () => false }
})

vi.mock('#lib/commands/fuzzy-search.js', () => ({
  searchAllCommands: (q: string) =>
    [
      { id: 'app.about', name: 'About Cmdr' },
      { id: 'file.copy', name: 'Copy' },
    ]
      .filter((c) => c.name.toLowerCase().includes(q.toLowerCase()))
      .map((command) => ({ command })),
}))

vi.mock('#lib/utils/confirm-dialog.js', () => ({
  confirmDialog: (message: string, title: string) => confirmDialog({ message, title }),
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
  onTestFinished(withPlatform('macos'))
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

  it('filters by the physical key, so a stored ⌥⇧= is findable by pressing it', () => {
    const c = create()
    c.handleKeyFilterKeyDown(keyEvent({ key: '±', code: 'Equal', altKey: true, shiftKey: true }))
    expect(c.keySearchQuery).toBe('⌥⇧=')
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
    onTestFinished(withPlatform('linux'))
    const c = create()
    c.handleKeyFilterKeyDown(keyEvent({ key: 'Control', ctrlKey: true }))
    expect(c.keySearchQuery).toBe('Ctrl')
    // The non-mac regex recognizes a Ctrl/Alt/Shift/Super-only value, so releasing
    // the last modifier clears it.
    c.handleKeyFilterKeyUp(keyEvent({ key: 'Control', ctrlKey: false }))
    expect(c.keySearchQuery).toBe('')
  })

  it.each(['windows', 'linux'] as const)(
    'names the Super key Super on %s, the token a stored combo holds, so held modifiers filter',
    (platform) => {
      onTestFinished(withPlatform(platform))
      effectiveShortcuts.set('file.copy', ['Ctrl+Super+C'])
      effectiveShortcuts.set('app.about', ['Ctrl+A'])
      const c = create()
      c.handleKeyFilterKeyDown(keyEvent({ key: 'Meta', ctrlKey: true, metaKey: true }))
      expect(c.keySearchQuery).toBe('Ctrl+Super')
      expect(c.filteredCommands.map((cmd) => cmd.id)).toEqual(['file.copy'])
      // Releasing the last modifier clears a Super-only value.
      c.handleKeyFilterKeyDown(keyEvent({ key: 'Meta', metaKey: true }))
      c.handleKeyFilterKeyUp(keyEvent({ key: 'Meta', metaKey: false }))
      expect(c.keySearchQuery).toBe('')
    },
  )
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
    onTestFinished(withCapabilities('globalShortcuts'))
    const c = create()
    expect(c.showGlobalGoToLatestRow).toBe(true)
    c.activeFilter = 'modified'
    expect(c.showGlobalGoToLatestRow).toBe(false)
  })

  it('hides the global go-to-latest row where no shortcut fires from other apps (Wayland)', () => {
    const c = create()
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
  // Regression anchor: both keys used to mean "remove this binding" on an empty
  // capture, so neither could ever be bound, not even forward delete to Delete
  // (issue #362). Removing is the pill's × button.
  it.each([
    ['⌫', 'Backspace'],
    ['⌦', 'Delete'],
  ])('captures %s like any other key', (_glyph, key) => {
    vi.useFakeTimers()
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.startEditingShortcut('file.copy', 0)
    c.handleKeyDown(keyEvent({ key, code: key }))
    expect(c.pendingKey).toBe(key)
    vi.advanceTimersByTime(500)
    expect(removeShortcut).not.toHaveBeenCalled()
    expect(setShortcut).toHaveBeenCalledWith({ commandId: 'file.copy', index: 0, shortcut: key })
    vi.useRealTimers()
  })

  it('captures a typed symbol by its character, on any layout', () => {
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.startEditingShortcut('file.copy', 0)
    c.handleKeyDown(keyEvent({ key: '*', code: 'Backslash', shiftKey: true }))
    expect(c.pendingKey).toBe('*')
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
    expect(setShortcut).toHaveBeenCalledWith({ commandId: 'file.copy', index: 0, shortcut: '⌘X' })
    vi.useRealTimers()
  })

  // ⇧+letter types a capital, so the file list takes it as a filter or jump keystroke
  // and the binding would never fire.
  it('refuses a Shift+letter combo: a banner, no save, and the next key still captures', () => {
    vi.useFakeTimers()
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.startEditingShortcut('file.copy', 0)

    c.handleKeyDown(keyEvent({ key: 'A', code: 'KeyA', shiftKey: true }))
    expect(c.pendingKey).toBe('⇧A')
    expect(c.conflictWarning).toEqual({ shortcut: '⇧A', conflict: { kind: 'shiftLetter' } })
    vi.advanceTimersByTime(500)
    expect(setShortcut).not.toHaveBeenCalled()
    expect(findConflictsForShortcut).not.toHaveBeenCalled()

    c.handleKeyDown(keyEvent({ key: 'X', metaKey: true }))
    expect(c.conflictWarning).toBe(null)
    vi.advanceTimersByTime(500)
    expect(setShortcut).toHaveBeenCalledWith({ commandId: 'file.copy', index: 0, shortcut: '⌘X' })
    vi.useRealTimers()
  })

  it.each([
    ['a letter', 'a', 'KeyA', 'A'],
    ['a digit', '7', 'Digit7', '7'],
  ])('refuses %s on its own on a main-window command', (_what, key, code, combo) => {
    vi.useFakeTimers()
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.startEditingShortcut('file.copy', 0)

    c.handleKeyDown(keyEvent({ key, code }))
    expect(c.conflictWarning).toEqual({ shortcut: combo, conflict: { kind: 'bareKey' } })
    vi.advanceTimersByTime(500)
    expect(setShortcut).not.toHaveBeenCalled()
    vi.useRealTimers()
  })

  it('persists the physical key for an ⌥-modified punctuation combo, not what the layout typed', () => {
    vi.useFakeTimers()
    effectiveShortcuts.set('file.copy', ['F5'])
    const c = create()
    c.startEditingShortcut('file.copy', 0)
    // macOS US reports `±` for ⌥⇧=; storing that would be dead on any keyboard.
    c.handleKeyDown(keyEvent({ key: '±', code: 'Equal', altKey: true, shiftKey: true }))
    expect(c.pendingKey).toBe('⌥⇧=')
    vi.advanceTimersByTime(500)
    expect(setShortcut).toHaveBeenCalledWith({ commandId: 'file.copy', index: 0, shortcut: '⌥⇧=' })
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
    expect(setShortcut).toHaveBeenCalledWith({ commandId: 'file.copy', index: 0, shortcut: '⌘X' })
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
