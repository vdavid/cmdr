/**
 * The rendered conflict prompt: that it shows only when there's a question, that
 * it names which operation is asking, and that its buttons reach the host.
 *
 * The host itself is mocked here — its decisions have their own tests
 * (`operation-conflict.svelte.test.ts`), and mocking it lets each case set up
 * one prompt shape without a listener, a store, or an operation.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteConflictEvent } from '$lib/tauri-commands'
import type { ConflictPrompt } from './operation-conflict.svelte'

let prompt: ConflictPrompt | null = null
const resolveConflictPrompt = vi.fn(() => Promise.resolve())
const cancelConflictPrompt = vi.fn(() => Promise.resolve())

vi.mock('./operation-conflict.svelte', () => ({
  getConflictPrompt: () => prompt,
  isResolvingConflictPrompt: () => false,
  isCancellingConflictPrompt: () => false,
  resolveConflictPrompt: (...args: unknown[]) => resolveConflictPrompt(...(args as [])),
  cancelConflictPrompt: (...args: unknown[]) => cancelConflictPrompt(...(args as [])),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

import OperationConflictDialog from './OperationConflictDialog.svelte'

function snapshot(over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'paused',
    source: '/Users/me/Pictures/2026',
    destination: '/Volumes/Naspolya/Backup',
    supportsRollback: true,
    error: null,
    ...over,
  }
}

function conflictEvent(over: Partial<WriteConflictEvent> = {}): WriteConflictEvent {
  return {
    operationId: 'op-1',
    conflictId: 1,
    sourcePath: '/Users/me/Pictures/2026/sunset.jpg',
    destinationPath: '/Volumes/Naspolya/Backup/2026/sunset.jpg',
    sourceSize: 2048,
    destinationSize: 1024,
    sourceModified: 1_700_000_000,
    destinationModified: 1_699_000_000,
    destinationIsNewer: false,
    sizeDifference: -1024,
    ...over,
  }
}

function makePrompt(over: Partial<ConflictPrompt> = {}): ConflictPrompt {
  return {
    operationId: 'op-1',
    event: conflictEvent(),
    snapshot: snapshot(),
    pausedOthers: false,
    ...over,
  }
}

let component: Record<string, unknown> | null = null
let target: HTMLElement | null = null

function render(): HTMLElement {
  target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(OperationConflictDialog, { target })
  flushSync()
  return target
}

beforeEach(() => {
  vi.clearAllMocks()
  prompt = null
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  target?.remove()
  target = null
})

describe('OperationConflictDialog', () => {
  it('renders nothing while no operation is asking', () => {
    const host = render()
    expect(host.querySelector('[role="dialog"]')).toBeNull()
  })

  it('asks about the clashing file', () => {
    prompt = makePrompt()
    const host = render()

    expect(host.querySelector('[role="dialog"]')).not.toBeNull()
    expect(host.textContent).toContain('sunset.jpg')
  })

  it('names which operation is asking, so several at once stay distinguishable', () => {
    prompt = makePrompt()
    const host = render()

    // The catalog's ICU select resolves to the copy arm plus the destination.
    expect(host.textContent).toContain('Copying to Backup')
  })

  it('says nothing about the operation before its snapshot lands', () => {
    prompt = makePrompt({ snapshot: null })
    const host = render()

    expect(host.querySelector('#operation-conflict-context')).toBeNull()
  })

  it('says the rest is on hold only when the rest actually is', () => {
    prompt = makePrompt({ pausedOthers: true })
    const withOthers = render()
    expect(withOthers.textContent).toContain('Everything else is paused until you answer.')
  })

  it('claims no hold when the asking operation is the only one', () => {
    prompt = makePrompt()
    const host = render()
    expect(host.textContent).not.toContain('Everything else is paused')
  })

  it('sends a resolution to the host, apply-to-all included', () => {
    prompt = makePrompt()
    const host = render()

    const buttons = [...host.querySelectorAll('button')]
    const skipAll = buttons.find((b) => b.textContent.trim() === 'Skip all')
    skipAll?.click()

    expect(resolveConflictPrompt).toHaveBeenCalledWith('skip', true)
  })

  it('offers Rollback for a copy the backend can reverse, and asks before it deletes anything', () => {
    prompt = makePrompt()
    const host = render()

    const buttons = [...host.querySelectorAll('button')]
    const rollback = buttons.find((b) => b.textContent.trim() === 'Rollback')
    expect(rollback?.disabled).toBe(false)
    rollback?.click()
    flushSync()

    // Nothing is deleted on that click: rollback removes every file the
    // operation has written, and an overwritten one has no backup.
    expect(cancelConflictPrompt).not.toHaveBeenCalled()

    const confirm = [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Roll back')
    expect(confirm, 'the question is on screen').toBeDefined()
    confirm?.click()

    expect(cancelConflictPrompt).toHaveBeenCalledWith(true)
  })

  it('leaves the operation alone when the rollback question is declined', () => {
    prompt = makePrompt()
    const host = render()

    ;[...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Rollback')?.click()
    flushSync()
    ;[...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Keep them')?.click()
    flushSync()

    expect(cancelConflictPrompt).not.toHaveBeenCalled()
    // And the clash is still up, waiting for a real answer.
    expect(host.querySelector('.conflict-filename')).not.toBeNull()
  })

  it('offers a plain Cancel when the backend cannot reverse the operation', () => {
    // A same-volume move renames server-side and a cross-volume one has nothing
    // staged; both say so through `supportsRollback`, which is more than the
    // progress dialog knows about the second case.
    prompt = makePrompt({ snapshot: snapshot({ operationType: 'move', supportsRollback: false }) })
    const host = render()

    const buttons = [...host.querySelectorAll('button')]
    expect(buttons.find((b) => b.textContent.trim() === 'Rollback')?.disabled).toBe(true)
    buttons.find((b) => b.textContent.trim() === 'Cancel')?.click()

    expect(cancelConflictPrompt).toHaveBeenCalledWith(false)
  })

  it('has no × and ignores Escape, so a reflex cannot cancel a background transfer', () => {
    prompt = makePrompt()
    const host = render()

    expect(host.querySelector('.modal-close-button')).toBeNull()

    host.querySelector('[role="dialog"]')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    flushSync()

    expect(cancelConflictPrompt).not.toHaveBeenCalled()
    expect(host.querySelector('[role="dialog"]')).not.toBeNull()
  })
})
