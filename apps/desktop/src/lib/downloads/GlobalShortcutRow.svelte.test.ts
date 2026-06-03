/**
 * Contract for the global go-to-latest-download shortcut row that lives in
 * the Keyboard shortcuts section.
 *
 *   - Renders the action name with a `(global)` marker.
 *   - Shows the current binding (macOS-symbol form) as a recorder pill.
 *   - Recording a new combo writes the binding through `setGlobalGoToLatestBinding`
 *     (which resets `acknowledged`) AND calls the `set_global_go_to_latest_shortcut`
 *     IPC for live-apply.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { getSettingMock, setSettingMock, setGlobalGoToLatestShortcutMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
  setGlobalGoToLatestShortcutMock: vi.fn(),
}))

vi.mock('$lib/settings', () => ({
  getSetting: getSettingMock,
  setSetting: setSettingMock,
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    setGlobalGoToLatestShortcut: setGlobalGoToLatestShortcutMock,
  },
}))

// The global go-to-latest hotkey is macOS-only; force `isMacOS()` (which reads the
// user agent) to report macOS so `formatKeyCombo` emits the symbol form.
Object.defineProperty(navigator, 'userAgent', {
  value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)',
  configurable: true,
})

import GlobalShortcutRow from './GlobalShortcutRow.svelte'

const BINDING_KEY = 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding'
const ACK_KEY = 'behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged'

function setBinding(binding: string): void {
  getSettingMock.mockImplementation((key: string): unknown => {
    if (key === BINDING_KEY) return binding
    return undefined
  })
}

beforeEach(() => {
  getSettingMock.mockReset()
  setSettingMock.mockReset()
  setGlobalGoToLatestShortcutMock.mockReset().mockResolvedValue({
    status: 'ok',
    data: { status: 'registered', binding: '\u{2303}\u{2325}\u{2318}J', enabled: true },
  })
  setBinding('\u{2303}\u{2325}\u{2318}J')
})

function mountRow(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(GlobalShortcutRow, { target })
  return target
}

describe('GlobalShortcutRow', () => {
  it('renders the action name with a (global) marker', () => {
    const target = mountRow()
    expect(target.textContent).toContain('Go to latest download')
    expect(target.textContent).toContain('(global)')
    target.remove()
  })

  it('shows the current binding on the pill', () => {
    const target = mountRow()
    const pill = target.querySelector<HTMLButtonElement>('.shortcut-pill')
    expect(pill?.textContent).toContain('\u{2303}\u{2325}\u{2318}J')
    target.remove()
  })

  it('records a new combo: writes binding (+ resets acknowledged) and calls the live-apply IPC', async () => {
    const target = mountRow()
    const pill = target.querySelector<HTMLButtonElement>('.shortcut-pill')
    if (!pill) throw new Error('shortcut pill not found')
    pill.click()
    await tick()

    // Press ⌃⌥⌘K (a complete combo with modifiers).
    const event = new KeyboardEvent('keydown', {
      key: 'k',
      code: 'KeyK',
      ctrlKey: true,
      altKey: true,
      metaKey: true,
      bubbles: true,
    })
    document.dispatchEvent(event)
    await tick()
    await Promise.resolve()
    await tick()

    // Binding written through the reset-aware helper (binding + ack reset).
    expect(setSettingMock).toHaveBeenCalledWith(BINDING_KEY, expect.stringContaining('K'))
    expect(setSettingMock).toHaveBeenCalledWith(ACK_KEY, false)
    // Live-apply IPC fired.
    expect(setGlobalGoToLatestShortcutMock).toHaveBeenCalled()
    target.remove()
  })

  it('ignores a modifier-only combo (no binding write)', async () => {
    const target = mountRow()
    const pill = target.querySelector<HTMLButtonElement>('.shortcut-pill')
    if (!pill) throw new Error('shortcut pill not found')
    pill.click()
    await tick()

    // A bare key with no modifier is rejected (global shortcuts need a modifier).
    const event = new KeyboardEvent('keydown', { key: 'k', code: 'KeyK', bubbles: true })
    document.dispatchEvent(event)
    await tick()
    await Promise.resolve()

    expect(setSettingMock).not.toHaveBeenCalledWith(BINDING_KEY, expect.anything())
    target.remove()
  })
})
