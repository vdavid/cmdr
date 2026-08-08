/**
 * Tier 3 a11y tests for `OperationConflictDialog.svelte`.
 *
 * The chrome around a shared body, so what's audited here is what this dialog
 * adds: the context line it describes itself by, and the on-hold note. The body
 * itself has its own audit in `transfer/TransferConflictDialog.a11y.test.ts`.
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteConflictEvent } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { ConflictPrompt } from './operation-conflict.svelte'

let prompt: ConflictPrompt | null = null

vi.mock('./operation-conflict.svelte', () => ({
  getConflictPrompt: () => prompt,
  isResolvingConflictPrompt: () => false,
  isCancellingConflictPrompt: () => false,
  resolveConflictPrompt: vi.fn(() => Promise.resolve()),
  cancelConflictPrompt: vi.fn(() => Promise.resolve()),
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

function makePrompt(over: Partial<ConflictPrompt> = {}): ConflictPrompt {
  const snapshot: OperationSnapshot = {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'paused',
    source: '/Users/me/Pictures/2026',
    destination: '/Volumes/Naspolya/Backup',
    supportsRollback: true,
    error: null,
  }
  const event: WriteConflictEvent = {
    operationId: 'op-1',
    sourcePath: '/Users/me/Pictures/2026/sunset.jpg',
    destinationPath: '/Volumes/Naspolya/Backup/2026/sunset.jpg',
    sourceSize: 2048,
    destinationSize: 1024,
    sourceModified: 1_700_000_000,
    destinationModified: 1_699_000_000,
    destinationIsNewer: false,
    sizeDifference: -1024,
  }
  return { operationId: 'op-1', event, snapshot, pausedOthers: false, ...over }
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
  prompt = null
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  target?.remove()
  target = null
})

describe('OperationConflictDialog a11y', () => {
  it('a file clash on a backgrounded copy has no a11y violations', async () => {
    prompt = makePrompt()
    const host = render()
    await expectNoA11yViolations(host)
  })

  it('the on-hold note has no a11y violations', async () => {
    prompt = makePrompt({ pausedOthers: true })
    const host = render()
    await expectNoA11yViolations(host)
  })

  it('a prompt whose snapshot has not landed has no a11y violations', async () => {
    prompt = makePrompt({ snapshot: null })
    const host = render()
    await expectNoA11yViolations(host)
  })
})
