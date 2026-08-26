/**
 * The trash toast: the sentence, and the two ways out of a mistake.
 *
 * The button set itself is part of the contract. ❌ No "delete permanently" here:
 * this toast renders after EVERY trash, and a one-click irreversible action on a
 * surface that appears that often is a misclick away from the one thing the
 * journal can never reverse.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import type { PaneRevealAPI } from '$lib/file-explorer/navigation/navigate-and-select'

const runTrashUndo = vi.fn<(operationId: string) => Promise<void>>()
vi.mock('./trash-undo', () => ({
  runTrashUndo: (operationId: string): Promise<void> => runTrashUndo(operationId),
}))

/** One object payload, so the two `string` arguments can't be swapped unnoticed
 *  (`cmdr/no-confusable-callback-params`); the assertions read it by name. */
interface GoToTrashedCall {
  explorer: unknown
  operationId: string
  sourceFolderPath: string
}
const goToTrashedItems = vi.fn<(call: GoToTrashedCall) => Promise<void>>()
vi.mock('./go-to-trash', () => ({
  goToTrashedItems: (explorer: unknown, operationId: string, sourceFolderPath: string): Promise<void> =>
    goToTrashedItems({ explorer, operationId, sourceFolderPath }),
}))

const dismissToast = vi.fn<(id: string) => void>()
vi.mock('$lib/ui/toast', () => ({
  dismissToast: (id: string): void => {
    dismissToast(id)
  },
}))

import TrashCompleteToastContent from './TrashCompleteToastContent.svelte'

const explorer = { getFocusedPane: () => 'left' } as unknown as PaneRevealAPI

let target: HTMLElement

function render(): void {
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(TrashCompleteToastContent, {
    target,
    props: {
      toastId: 'toast-1',
      message: 'Moved 3 files to trash',
      operationId: 'op-1',
      sourceFolderPath: '/Users/me/Documents',
      explorer,
    },
  })
  flushSync()
}

function buttonLabelled(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === label)
}

beforeEach(() => {
  document.body.innerHTML = ''
  runTrashUndo.mockClear()
  runTrashUndo.mockResolvedValue(undefined)
  goToTrashedItems.mockClear()
  goToTrashedItems.mockResolvedValue(undefined)
  dismissToast.mockClear()
})

describe('TrashCompleteToastContent', () => {
  it('shows the composed sentence it was handed', () => {
    render()
    expect(target.querySelector('.message')?.textContent.trim()).toBe('Moved 3 files to trash')
  })

  it('offers exactly Undo and Go to trash', () => {
    render()
    const labels = [...target.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(labels).toEqual(['Go to trash', 'Undo'])
  })

  it('never offers a permanent delete', () => {
    render()
    const labels = [...target.querySelectorAll('button')].map((b) => b.textContent.trim().toLowerCase())
    expect(labels.some((l) => l.includes('permanent') || l.includes('delete'))).toBe(false)
  })

  it('puts Undo last, where the pointer travels least', () => {
    render()
    const labels = [...target.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(labels.at(-1)).toBe('Undo')
  })

  it('runs the undo for this operation and steps aside', () => {
    render()
    buttonLabelled('Undo')?.click()
    flushSync()

    expect(runTrashUndo).toHaveBeenCalledWith('op-1')
    // The undo raises its own progress toast; two voices about one restore is one
    // too many, so this one goes.
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })

  it('navigates to what this operation trashed, and steps aside', () => {
    render()
    buttonLabelled('Go to trash')?.click()
    flushSync()

    expect(goToTrashedItems).toHaveBeenCalledWith({
      explorer,
      operationId: 'op-1',
      sourceFolderPath: '/Users/me/Documents',
    })
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })
})
