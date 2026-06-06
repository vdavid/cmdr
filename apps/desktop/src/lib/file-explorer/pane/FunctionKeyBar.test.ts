import { describe, it, expect, vi } from 'vitest'
import { mount, flushSync } from 'svelte'

// The reactivity test rebinds a shortcut, which fires the menu-accelerator sync
// IPC and touches the plugin store. Stub both so the in-memory rebind path runs
// without a backend (mirrors `SortableHeader.svelte.test.ts`).
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

import FunctionKeyBar from './FunctionKeyBar.svelte'
import { setShortcut, removeShortcut, resetShortcut } from '$lib/shortcuts/shortcuts-store'
import type { CommandId } from '$lib/commands'

describe('FunctionKeyBar', () => {
  /** Simulates pressing the Shift key, waits for effect and flushes Svelte reactivity. */
  async function pressShift() {
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift' }))
    // Effects run after microtask; flush to apply $state change to DOM
    await new Promise((r) => setTimeout(r, 0))
    flushSync()
  }

  /** Simulates releasing the Shift key, waits for effect and flushes Svelte reactivity. */
  async function releaseShift() {
    document.dispatchEvent(new KeyboardEvent('keyup', { key: 'Shift' }))
    await new Promise((r) => setTimeout(r, 0))
    flushSync()
  }

  it('renders 7 buttons when visible', () => {
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true } })

    const buttons = target.querySelectorAll('button')
    expect(buttons).toHaveLength(7)
  })

  it('renders nothing when visible is false', () => {
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: false } })

    expect(target.querySelector('.function-key-bar')).toBeNull()
  })

  it('enables the source-op buttons by default (a real pane has canBeSource: true)', () => {
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true } })

    const buttons = target.querySelectorAll('button')
    // F2 (0) … F8 (6). All enabled on a real folder (default props).
    for (const button of buttons) {
      expect(button.disabled).toBe(false)
    }
  })

  it('dispatches the matching file.* command for each default-state F-key', () => {
    const onCommand = vi.fn<(id: CommandId) => void>()
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true, onCommand } })

    const buttons = target.querySelectorAll('button')
    // F2 Rename, F3 View, F4 Edit, F5 Copy, F6 Move, F7 New folder, F8 Delete.
    const expected: CommandId[] = [
      'file.rename',
      'file.view',
      'file.edit',
      'file.copy',
      'file.move',
      'file.newFolder',
      'file.delete',
    ]
    expected.forEach((commandId, index) => {
      buttons[index].click()
      expect(onCommand).toHaveBeenNthCalledWith(index + 1, commandId)
    })
  })

  it('dispatches the matching file.* command for each shift-state F-key', async () => {
    const onCommand = vi.fn<(id: CommandId) => void>()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, { target, props: { visible: true, onCommand } })

    await pressShift()

    const buttons = target.querySelectorAll('button')
    // Shift+F4 New file (2), Shift+F6 Rename (4), Shift+F8 Permanently (6).
    buttons[2].click()
    expect(onCommand).toHaveBeenLastCalledWith('file.newFile')
    buttons[4].click()
    expect(onCommand).toHaveBeenLastCalledWith('file.rename')
    buttons[6].click()
    expect(onCommand).toHaveBeenLastCalledWith('file.deletePermanently')

    await releaseShift()
    document.body.removeChild(target)
  })

  it('shows correct key labels', () => {
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true } })

    const kbds = target.querySelectorAll('kbd')
    const keys = Array.from(kbds).map((kbd) => kbd.textContent)
    expect(keys).toEqual(['F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8'])
  })

  it('shows correct action labels', () => {
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true } })

    const buttons = target.querySelectorAll('button')
    const labels = Array.from(buttons).map((btn) => btn.querySelector('span')?.textContent)
    expect(labels).toEqual(['Rename', 'View', 'Edit', 'Copy', 'Move', 'New folder', 'Delete'])
  })

  it('shows shift-state buttons when Shift is held', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, { target, props: { visible: true } })

    await pressShift()

    const kbds = target.querySelectorAll('kbd')
    const keys = Array.from(kbds).map((kbd) => kbd.textContent)
    // The chip on each command button reads ITS command's effective FIRST binding
    // (platform-formatted: `⇧F4` → `Shift+F4` off macOS). The Shift fork is
    // presentational — which buttons appear is fixed — so the Shift-revealed
    // "Rename" button (index 4) shows `file.rename`'s first binding, which is `F2`,
    // not `⇧F6`. That's the decided, truthful behavior. The four empty slots stay
    // hardcoded F-key labels (F2, F3, F5, F7) since they map to no command.
    expect(keys).toEqual(['F2', 'F3', 'Shift+F4', 'F5', 'F2', 'F7', 'Shift+F8'])

    // Shift+F4, Shift+F6, and Shift+F8 should have labels
    const buttons = target.querySelectorAll('button')
    const labels = Array.from(buttons).map((btn) => btn.querySelector('span')?.textContent ?? null)
    expect(labels).toEqual([null, null, 'New file', null, 'Rename', null, 'Permanently'])

    await releaseShift()
    document.body.removeChild(target)
  })

  it('restores normal buttons when Shift is released', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, { target, props: { visible: true } })

    await pressShift()
    await releaseShift()

    const kbds = target.querySelectorAll('kbd')
    const keys = Array.from(kbds).map((kbd) => kbd.textContent)
    expect(keys).toEqual(['F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8'])

    document.body.removeChild(target)
  })

  it("shows each button's effective first shortcut, and updates it live on rebind", () => {
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true } })

    const f5Kbd = () => target.querySelectorAll('kbd')[3].textContent // F5 = Copy = index 3
    expect(f5Kbd()).toBe('F5')

    // Rebinding `file.copy` re-renders the Copy button's chip immediately: the bar
    // reads the live effective shortcut, it never shows a stale hardcoded F-key.
    setShortcut('file.copy', 0, '⌘K')
    flushSync()
    expect(f5Kbd()).toBe('⌘K')

    resetShortcut('file.copy')
  })

  it('renders no chip for a command with no binding (the button keeps its label)', () => {
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true } })

    // Remove the only binding for `file.copy` (default is a single `F5`), leaving an
    // empty custom array ("all removed"). The F5 button should drop its chip but keep
    // its label and stay a button (still clickable). An empty <kbd> would read as broken.
    removeShortcut('file.copy', 0)
    flushSync()

    const copyButton = target.querySelectorAll('button')[3]
    expect(copyButton.querySelector('kbd')).toBeNull()
    expect(copyButton.querySelector('span')?.textContent).toBe('Copy')

    resetShortcut('file.copy')
  })

  it('disables most buttons in shift state', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, { target, props: { visible: true } })

    await pressShift()

    const buttons = target.querySelectorAll('button')
    // All disabled except Shift+F4 (index 2), Shift+F6 (index 4), and Shift+F8 (index 6)
    expect(buttons[0].disabled).toBe(true)
    expect(buttons[1].disabled).toBe(true)
    expect(buttons[2].disabled).toBe(false)
    expect(buttons[3].disabled).toBe(true)
    expect(buttons[4].disabled).toBe(false)
    expect(buttons[5].disabled).toBe(true)
    expect(buttons[6].disabled).toBe(false)

    await releaseShift()
    document.body.removeChild(target)
  })
})
