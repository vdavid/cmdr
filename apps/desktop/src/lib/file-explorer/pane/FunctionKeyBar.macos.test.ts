/**
 * The F-key bar on macOS, where "Open terminal here" exists (`openTerminalHere`) and takes the F9 slot.
 * Its own file because the platform and capabilities are set before anything imports; the platform-neutral
 * cases live in `FunctionKeyBar.test.ts`.
 */
import { describe, it, expect, vi } from 'vitest'
import { mount, flushSync } from 'svelte'

vi.hoisted(() => {
  vi.stubGlobal('__CMDR_PLATFORM__', 'macos')
  vi.stubGlobal('__CMDR_CAPABILITIES__', { openTerminalHere: true })
})
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
vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import FunctionKeyBar from './FunctionKeyBar.svelte'
import type { CommandId } from '#lib/commands/index.js'

describe('FunctionKeyBar on macOS', () => {
  it('adds a Terminal button in the F9 slot that opens a terminal here', () => {
    const onCommand = vi.fn<(id: CommandId) => void>()
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true, onCommand } })

    const buttons = target.querySelectorAll('button')
    expect(buttons).toHaveLength(8)
    const terminal = buttons[7]
    expect(terminal.querySelector('span')?.textContent).toBe('Terminal')
    expect(terminal.querySelector('kbd')?.textContent).toBe('F9')
    expect(terminal.disabled).toBe(false)

    terminal.click()
    expect(onCommand).toHaveBeenCalledWith('file.openTerminalHere')
  })

  it('keeps an empty ⇧F9 slot in the Shift row, so both rows have the same width', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, { target, props: { visible: true } })

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift' }))
    await new Promise((r) => setTimeout(r, 0))
    flushSync()

    const buttons = target.querySelectorAll('button')
    expect(buttons).toHaveLength(8)
    expect(buttons[7].disabled).toBe(true)
    expect(buttons[7].querySelector('kbd')?.textContent).toBe('⇧F9')

    document.dispatchEvent(new KeyboardEvent('keyup', { key: 'Shift' }))
    document.body.removeChild(target)
  })
})
