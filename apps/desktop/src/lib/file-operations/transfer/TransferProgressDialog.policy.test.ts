/**
 * The progress dialog's conflict-policy caption (#483): it names the policy the
 * OPERATION runs under, read off the backend snapshot, so a dialog that adopted
 * the operation (and never saw its setup) says it too, and a mid-run "… all"
 * answer changes it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { ConflictResolution } from '#lib/file-explorer/types.js'
import type { OperationSnapshot, WriteOperationType } from '#lib/ipc/bindings.js'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '#lib/file-operations/operation-session/window-operation-sessions.svelte.js'
import TransferProgressDialog from './TransferProgressDialog.svelte'

/** A fresh id per test: the window's operation rows outlive one test's sessions. */
let testIndex = 0
let adopted = 'op-0'

const { hooks } = vi.hoisted(() => ({
  hooks: {
    opsChanged: [] as ((event: { operations: unknown[] }) => void)[],
    seed: [] as unknown[],
  },
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  onOperationsChanged: vi.fn((cb: (event: { operations: unknown[] }) => void) => {
    hooks.opsChanged.push(cb)
    return Promise.resolve(() => {})
  }),
  listOperations: vi.fn(() => Promise.resolve(hooks.seed)),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

vi.mock('#lib/stores/volume-store.svelte.js', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

function row(operationType: WriteOperationType, conflictPolicy: ConflictResolution | null): OperationSnapshot {
  return {
    operationId: adopted,
    operationType,
    status: 'running',
    source: '/Users/test/big',
    destination: '/Volumes/backup/dest',
    rollbackSupport: 'supported',
    reverses: null,
    error: null,
    canRetry: false,
    conflictPolicy,
    items: null,
  }
}

async function flushPromises(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

beforeEach(async () => {
  hooks.opsChanged.length = 0
  testIndex++
  adopted = `op-${String(testIndex)}`
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

async function mountAdopted(
  operationType: 'copy' | 'move' | 'delete',
  conflictPolicy: ConflictResolution | null,
): Promise<{ component: ReturnType<typeof mount>; target: HTMLDivElement }> {
  hooks.seed = [row(operationType, conflictPolicy)]
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialog, {
    target,
    props: {
      adoptOperationId: adopted,
      operationType,
      sourceFolderPath: '/Users/test/big',
      destinationPath: operationType === 'delete' ? undefined : '/Volumes/backup/dest',
      onComplete: vi.fn(),
      onCancelled: vi.fn(),
      onError: vi.fn(),
      onQueue: vi.fn(),
    },
  })
  await flushPromises()
  return { component, target }
}

function caption(target: HTMLElement): string | null {
  return target.querySelector('.conflict-policy-caption')?.textContent.trim() ?? null
}

describe('TransferProgressDialog, conflict-policy caption', () => {
  it('names the policy of an adopted copy, which only the backend knows', async () => {
    const { component, target } = await mountAdopted('copy', 'overwrite_older')

    expect(caption(target)).toBe('If a file already exists: Overwrite if older')

    void unmount(component)
  })

  it('says "Ask for each" for the policy that asks', async () => {
    const { component, target } = await mountAdopted('move', 'stop')

    expect(caption(target)).toBe('If a file already exists: Ask for each')

    void unmount(component)
  })

  it('follows a mid-run "… all" answer', async () => {
    const { component, target } = await mountAdopted('copy', 'stop')

    for (const cb of hooks.opsChanged) cb({ operations: [row('copy', 'overwrite')] })
    await flushPromises()

    expect(caption(target)).toBe('If a file already exists: Overwrite')

    void unmount(component)
  })

  it('shows nothing for an operation with no policy', async () => {
    const { component, target } = await mountAdopted('delete', null)

    expect(caption(target)).toBeNull()

    void unmount(component)
  })
})
