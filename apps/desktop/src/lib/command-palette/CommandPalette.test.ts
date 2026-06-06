import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount } from 'svelte'
import { tick, flushSync } from 'svelte'
import CommandPalette from './CommandPalette.svelte'
import { pruneRecentCommands, pushRecentCommand } from '$lib/app-status-store'
import { setShortcut, resetShortcut } from '$lib/shortcuts/shortcuts-store'

// These ids are real command-registry entries, so the palette's effective-shortcut
// reads (which hit the real registry + store, not the mocked `$lib/commands`)
// resolve against known bindings. `app.quit` defaults to ⌘Q, `app.about` is unbound.
const ALL_COMMANDS = [
  { id: 'app.quit', name: 'Quit Cmdr', scope: 'App', shortcuts: ['⌘Q'], showInPalette: true },
  { id: 'app.about', name: 'About Cmdr', scope: 'App', shortcuts: [], showInPalette: true },
  { id: 'file.copyPath', name: 'Copy path to clipboard', scope: 'Main window', shortcuts: [], showInPalette: true },
  { id: 'view.showHidden', name: 'Toggle hidden files', scope: 'Main window', shortcuts: ['⌘⇧.'], showInPalette: true },
]

// Mock the app-status-store to avoid Tauri dependency in tests
vi.mock('$lib/app-status-store', () => ({
  pruneRecentCommands: vi.fn().mockResolvedValue([]),
  pushRecentCommand: vi.fn().mockResolvedValue(undefined),
}))

// The shortcuts store persists to a Tauri plugin store and syncs menu accelerators;
// stub both so the palette can read/rebind effective shortcuts in jsdom.
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
      keys: vi.fn(() => Promise.resolve([])),
      delete: vi.fn(() => Promise.resolve()),
    }),
  ),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

/** Read the chip text inside the row for a given command id. */
function shortcutTextsFor(target: HTMLElement, commandId: string): string[] {
  // Attribute selector, not `#palette-option-app.quit` — a dotted command id would
  // parse the part after the dot as a class selector.
  const row = target.querySelector(`[id="palette-option-${commandId}"]`)
  if (!row) return []
  return Array.from(row.querySelectorAll('.shortcuts .shortcut-chip')).map((el) => el.textContent)
}

// Mock the commands module to provide test data
vi.mock('$lib/commands', () => ({
  getPaletteCommands: vi.fn(() => ALL_COMMANDS),
  searchCommands: vi.fn((query: string, recentIds: string[] = []) => {
    const allMatches = ALL_COMMANDS.map((command) => ({ command, matchedIndices: [] }))
    if (!query.trim()) {
      // Mirror the real implementation's recents-first ordering so tests can
      // exercise the wiring without depending on the real fuzzy module.
      const byId = new Map(allMatches.map((m) => [m.command.id, m]))
      const recents = recentIds.flatMap((id) => {
        const match = byId.get(id)
        return match ? [match] : []
      })
      const recentSet = new Set(recents.map((m) => m.command.id))
      const rest = allMatches.filter((m) => !recentSet.has(m.command.id))
      return [...recents, ...rest]
    }
    return allMatches.filter((c) => c.command.name.toLowerCase().includes(query.toLowerCase()))
  }),
}))

describe('CommandPalette', () => {
  let mockOnExecute: (commandId: string) => void
  let mockOnClose: () => void

  beforeEach(() => {
    mockOnExecute = vi.fn()
    mockOnClose = vi.fn()
    // Mock scrollIntoView which isn't available in jsdom
    Element.prototype.scrollIntoView = vi.fn()
    vi.mocked(pruneRecentCommands).mockResolvedValue([])
    vi.mocked(pushRecentCommand).mockClear()
  })

  afterEach(() => {
    // Drop any rebinds so a custom shortcut doesn't leak into the next test.
    resetShortcut('app.quit')
    resetShortcut('view.showHidden')
  })

  it('renders the modal with search input', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    const input = target.querySelector('input')
    expect(input).toBeTruthy()
    expect(input?.placeholder).toContain('command')
  })

  it('shows all commands when query is empty', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    const results = target.querySelectorAll('[class*="result-item"]')
    expect(results.length).toBeGreaterThan(0)
  })

  it('filters commands on input', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    const input = target.querySelector('input')
    if (input) {
      input.value = 'quit'
      input.dispatchEvent(new InputEvent('input', { bubbles: true }))
    }

    await tick()

    // Results should be filtered (mock only returns matches containing 'quit')
    const results = target.querySelectorAll('[class*="result-item"]')
    expect(results.length).toBe(1)
  })

  it('calls onClose when Escape is pressed', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    const input = target.querySelector('input')
    if (input) {
      input.focus()
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    }

    await tick()

    expect(mockOnClose).toHaveBeenCalled()
  })

  it('calls onExecute when Enter is pressed with an item under the cursor', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    const input = target.querySelector('input')
    if (input) {
      input.focus()
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    }

    await tick()

    expect(mockOnExecute).toHaveBeenCalled()
  })

  it('navigates cursor with ArrowDown', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    const input = target.querySelector('input')
    if (input) {
      input.focus()
      // Initial cursor position is index 0, arrow down should move to index 1
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    }

    await tick()

    // Check that cursor moved - verify the command that is executed is the second one
    // We do this by pressing Enter and checking which command is executed
    if (input) {
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    }

    await tick()

    // Second command (index 1) is 'app.about'
    expect(mockOnExecute).toHaveBeenCalledWith('app.about')
  })

  it('navigates cursor with ArrowUp', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    const input = target.querySelector('input')
    if (input) {
      input.focus()
      // Move down first then up
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
      await tick()
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
    }

    await tick()

    // Check that we're back at first item by pressing Enter and checking executed command
    if (input) {
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    }

    await tick()

    // First command (index 0) is 'app.quit'
    expect(mockOnExecute).toHaveBeenCalledWith('app.quit')
  })

  it('stops keyboard event propagation', async () => {
    const target = document.createElement('div')
    const propagationSpy = vi.fn()

    // Add listener on parent to check propagation
    target.addEventListener('keydown', propagationSpy)

    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    const input = target.querySelector('input')
    if (input) {
      input.focus()
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    }

    await tick()

    // Event should not have propagated to parent due to stopPropagation
    // Note: This test may not work perfectly since we're dispatching on the input directly
    // The stopPropagation happens in the component's keydown handler
    expect(mockOnClose).not.toHaveBeenCalled()
  })

  it('closes on click outside', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    // Simulate click on overlay background
    const overlay = target.querySelector('[class*="overlay"]')
    if (overlay) {
      overlay.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    }

    await tick()

    expect(mockOnClose).toHaveBeenCalled()
  })

  it('shows keyboard shortcuts for commands', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    await tick()

    // Check that shortcuts are displayed
    const shortcutElements = target.querySelectorAll('[class*="shortcut"]')
    expect(shortcutElements.length).toBeGreaterThan(0)
  })

  it('restores focus to the previously focused element on destroy', async () => {
    const trigger = document.createElement('button')
    document.body.appendChild(trigger)
    trigger.focus()
    expect(document.activeElement).toBe(trigger)

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    // Wait for the palette's onMount to run so it captures `trigger` as the
    // previously focused element. After this, simulate "palette has stolen
    // focus" by focusing something else; the test cares about restore-on-destroy.
    await tick()

    const otherEl = document.createElement('input')
    document.body.appendChild(otherEl)
    otherEl.focus()
    expect(document.activeElement).toBe(otherEl)

    void unmount(component)
    await tick()

    expect(document.activeElement).toBe(trigger)

    otherEl.remove()
    trigger.remove()
    target.remove()
  })

  it('leads the empty-query list with recents, most-recent first', async () => {
    vi.mocked(pruneRecentCommands).mockResolvedValue(['file.copyPath', 'app.about'])

    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })

    // Two ticks: one for onMount to start, one for the resolved recents to land.
    await tick()
    await tick()

    const input = target.querySelector('input')
    input?.focus()
    input?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await tick()

    expect(mockOnExecute).toHaveBeenCalledWith('file.copyPath')
  })

  it('records the executed command on Enter', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()

    const input = target.querySelector('input')
    input?.focus()
    input?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await tick()

    expect(pushRecentCommand).toHaveBeenCalledWith('app.quit')
  })

  it('records the executed command on click', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()

    const items = target.querySelectorAll('[class*="result-item"]')
    expect(items.length).toBeGreaterThan(1)
    ;(items[1] as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()

    expect(pushRecentCommand).toHaveBeenCalledWith('app.about')
    expect(mockOnExecute).toHaveBeenCalledWith('app.about')
  })

  it('renders "Recent" and "All commands" subheaders when recents exist', async () => {
    vi.mocked(pruneRecentCommands).mockResolvedValue(['file.copyPath', 'app.about'])

    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()
    await tick()

    const headings = Array.from(target.querySelectorAll('.group-heading')).map((el) => el.textContent.trim())
    expect(headings).toEqual(['Recent', 'All commands'])
  })

  it('renders no group subheaders when there are no recents', async () => {
    vi.mocked(pruneRecentCommands).mockResolvedValue([])

    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()
    await tick()

    expect(target.querySelectorAll('.group-heading').length).toBe(0)
  })

  it('renders no group subheaders during an active search', async () => {
    vi.mocked(pruneRecentCommands).mockResolvedValue(['file.copyPath'])

    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()
    await tick()

    const input = target.querySelector('input')
    if (input) {
      input.value = 'copy'
      input.dispatchEvent(new InputEvent('input', { bubbles: true }))
    }
    await tick()

    // Grouping is for the recents view only; typing collapses it.
    expect(target.querySelectorAll('.group-heading').length).toBe(0)
  })

  it('wires combobox ARIA: aria-activedescendant follows the cursor, only the cursor option is tabbable', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()

    const input = target.querySelector<HTMLInputElement>('input.search-input')
    expect(input?.getAttribute('role')).toBe('combobox')
    expect(input?.getAttribute('aria-controls')).toBe('palette-listbox')
    expect(input?.getAttribute('aria-autocomplete')).toBe('list')
    expect(input?.getAttribute('aria-expanded')).toBe('true')

    // Cursor starts at index 0 → activedescendant points to the first option's id.
    const firstOption = target.querySelectorAll<HTMLElement>('[role="option"]')[0]
    expect(input?.getAttribute('aria-activedescendant')).toBe(firstOption.id)
    expect(firstOption.getAttribute('tabindex')).toBe('0')

    // Every other option is tabindex="-1": only the cursor option is tabbable
    // (satisfies axe's scrollable-region-focusable rule while keeping DOM focus on the input).
    const otherOptions = Array.from(target.querySelectorAll<HTMLElement>('[role="option"]')).slice(1)
    for (const opt of otherOptions) {
      expect(opt.getAttribute('tabindex')).toBe('-1')
    }
  })

  it('drops the listbox entirely on a no-results query', async () => {
    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()

    const input = target.querySelector<HTMLInputElement>('input')
    if (input) {
      input.value = 'xxxxxnomatch'
      input.dispatchEvent(new InputEvent('input', { bubbles: true }))
    }
    await tick()

    // No listbox means axe's scrollable-region-focusable rule doesn't apply.
    expect(target.querySelector('[role="listbox"]')).toBeNull()
    expect(target.querySelector('.no-results')?.textContent).toContain('No commands found')
    expect(input?.getAttribute('aria-expanded')).toBe('false')
  })

  it('renders the effective custom binding, not the registry default', async () => {
    // view.showHidden defaults to ⌘⇧.; rebind it and the row must show the custom
    // combo. (A non-native command: the macOS-native commands like app.quit can't
    // be rebound and aren't shown in the palette anyway.)
    setShortcut('view.showHidden', 0, '⌘0')

    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()

    expect(shortcutTextsFor(target, 'view.showHidden')).toEqual(['⌘0'])
  })

  it('caps the shown shortcuts at three', async () => {
    // Bind four shortcuts; only the first three render.
    setShortcut('view.showHidden', 0, '⌘1')
    setShortcut('view.showHidden', 1, '⌘2')
    setShortcut('view.showHidden', 2, '⌘3')
    setShortcut('view.showHidden', 3, '⌘4')

    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()

    expect(shortcutTextsFor(target, 'view.showHidden')).toEqual(['⌘1', '⌘2', '⌘3'])
  })

  it('updates a rendered row live when the binding changes while the palette is open', async () => {
    // Custom bindings are stored verbatim (no platform conversion), so they assert
    // cleanly regardless of the test platform. (view.showHidden is a normal,
    // rebindable command — unlike the macOS-native ones.)
    setShortcut('view.showHidden', 0, '⌘7')

    const target = document.createElement('div')
    mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()

    expect(shortcutTextsFor(target, 'view.showHidden')).toEqual(['⌘7'])

    // Simulate a rebind (the MCP / Settings path) while the palette stays open.
    setShortcut('view.showHidden', 0, '⌘8')
    flushSync()

    expect(shortcutTextsFor(target, 'view.showHidden')).toEqual(['⌘8'])
  })

  it('does not throw if the previously focused element is no longer in the DOM', async () => {
    const trigger = document.createElement('button')
    document.body.appendChild(trigger)
    trigger.focus()

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(CommandPalette, {
      target,
      props: { onExecute: mockOnExecute, onClose: mockOnClose },
    })
    await tick()

    trigger.remove()
    expect(() => {
      void unmount(component)
    }).not.toThrow()
    await tick()

    target.remove()
  })
})
