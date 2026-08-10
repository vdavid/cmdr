/**
 * What the quit prompt renders, and the one structural claim it makes: that it
 * sits above every other modal.
 */

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import QuitConfirmationDialog from './QuitConfirmationDialog.svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

function operation(operationId: string, operationType: OperationSnapshot['operationType']): OperationSnapshot {
  return {
    operationId,
    operationType,
    status: 'running',
    source: '/Volumes/Naspolya/media/Holiday.mov',
    destination: '/Users/dave/Backup',
    supportsRollback: true,
    error: null,
  }
}

async function renderDialog(
  operations: OperationSnapshot[],
  secondsLeft: number,
  handlers: { onQuit?: () => void; onKeepWorking?: () => void } = {},
): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(QuitConfirmationDialog, {
    target,
    props: {
      operations,
      secondsLeft,
      onQuit: handlers.onQuit ?? (() => {}),
      onKeepWorking: handlers.onKeepWorking ?? (() => {}),
    },
  })
  await tick()
  return target
}

describe('QuitConfirmationDialog', () => {
  it('names how many operations are running, in the plural the count needs', async () => {
    const one = await renderDialog([operation('op-1', 'copy')], 15)
    expect(one.textContent).toContain('Quit while an operation is running?')

    const several = await renderDialog([operation('op-1', 'copy'), operation('op-2', 'move')], 15)
    expect(several.textContent).toContain('Quit while 2 operations are running?')
  })

  it('counts down in seconds, singular on the last one', async () => {
    const many = await renderDialog([operation('op-1', 'copy')], 12)
    expect(many.textContent).toContain('Quitting in 12 seconds')

    const last = await renderDialog([operation('op-1', 'copy')], 1)
    expect(last.textContent).toContain('Quitting in 1 second,')
  })

  it('lists each operation by what it is doing and the names it touches', async () => {
    const target = await renderDialog([operation('op-1', 'copy'), operation('op-2', 'delete')], 15)
    const rows = target.querySelectorAll('.operation')
    expect(rows).toHaveLength(2)
    // Basenames, not the full paths: the paths ride in the tooltip.
    expect(rows[0]?.textContent).toContain('Copying')
    expect(rows[0]?.textContent).toContain('Holiday.mov')
    expect(rows[0]?.textContent).toContain('Backup')
    expect(rows[0]?.textContent).not.toContain('/Volumes')
    expect(rows[1]?.textContent).toContain('Deleting')
  })

  it('offers exactly two answers, with quitting as the destructive one', async () => {
    const target = await renderDialog([operation('op-1', 'copy')], 15)
    const buttons = [...target.querySelectorAll<HTMLButtonElement>('.modal-footer button')]
    expect(buttons.map((b) => b.textContent?.trim())).toEqual(['Keep working', 'Quit now'])
    expect(buttons[1]?.className).toContain('btn-danger')
  })

  it('answers through its callbacks, never on its own', async () => {
    const onQuit = vi.fn()
    const onKeepWorking = vi.fn()
    const target = await renderDialog([operation('op-1', 'copy')], 15, { onQuit, onKeepWorking })
    const buttons = [...target.querySelectorAll<HTMLButtonElement>('.modal-footer button')]

    buttons[0]?.click()
    expect(onKeepWorking).toHaveBeenCalledOnce()
    expect(onQuit).not.toHaveBeenCalled()

    buttons[1]?.click()
    expect(onQuit).toHaveBeenCalledOnce()
  })

  it('Escape means "keep working", the answer that loses nothing', async () => {
    const onKeepWorking = vi.fn()
    const target = await renderDialog([operation('op-1', 'copy')], 15, { onKeepWorking })
    const overlay = target.querySelector('.modal-overlay')
    overlay?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(onKeepWorking).toHaveBeenCalledOnce()
  })

  it('opts into the topmost layer', async () => {
    const target = await renderDialog([operation('op-1', 'copy')], 15)
    expect(target.querySelector('.modal-overlay')?.classList.contains('topmost')).toBe(true)
  })
})

// The layering claim, checked at the source rather than through happy-dom: the
// component styles aren't applied in this environment, so a `getComputedStyle`
// assertion here would pass on an empty string and prove nothing. These two read
// the actual declarations, which IS where the guarantee lives.
describe('the topmost layer really is above every other modal', () => {
  function read(relative: string): string {
    return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8')
  }

  function tokenValue(css: string, token: string): number {
    const match = new RegExp(`${token}:\\s*(\\d+);`).exec(css)
    expect(match, `${token} is defined in app.css`).not.toBeNull()
    return Number(match?.[1])
  }

  it('--z-modal-top outranks --z-modal', () => {
    const css = read('../../app.css')
    expect(tokenValue(css, '--z-modal-top')).toBeGreaterThan(tokenValue(css, '--z-modal'))
  })

  it('the `topmost` class is what spends that token', () => {
    const modalDialog = read('../ui/ModalDialog.svelte')
    expect(modalDialog).toContain('.modal-overlay.topmost {\n        z-index: var(--z-modal-top);')
  })
})
