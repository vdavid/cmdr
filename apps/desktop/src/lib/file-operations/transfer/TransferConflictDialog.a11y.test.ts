/**
 * Tier 3 a11y tests for `TransferConflictDialog.svelte`.
 *
 * The conflict-resolution UI extracted from `TransferProgressDialog`: a
 * source-vs-destination comparison grid plus the resolution button grid and a
 * bottom Rollback/Cancel row. It's props-driven (no Tauri coupling), so each
 * test renders one conflict shape and audits the initial render.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TransferConflictDialog from './TransferConflictDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { WriteConflictEvent } from '$lib/tauri-commands'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

function fileConflict(overrides: Partial<WriteConflictEvent> = {}): WriteConflictEvent {
  return {
    operationId: 'op-1',
    sourcePath: '/Users/test/report.pdf',
    destinationPath: '/Users/test/dest/report.pdf',
    sourceSize: 2048,
    destinationSize: 1024,
    sourceModified: 1_700_000_000,
    destinationModified: 1_699_000_000,
    destinationIsNewer: false,
    sizeDifference: -1024,
    ...overrides,
  }
}

function mountDialog(opts: {
  conflictEvent: WriteConflictEvent
  isCopy: boolean
  isMove: boolean
  isSameVolumeMove: boolean
}): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferConflictDialog, {
    target,
    props: {
      conflictEvent: opts.conflictEvent,
      isCopy: opts.isCopy,
      isMove: opts.isMove,
      isSameVolumeMove: opts.isSameVolumeMove,
      isCancelling: false,
      isResolvingConflict: false,
      onResolve: () => {},
      onCancel: () => {},
    },
  })
  return target
}

describe('TransferConflictDialog a11y', () => {
  it('file-over-file copy conflict has no a11y violations', async () => {
    const target = mountDialog({
      conflictEvent: fileConflict(),
      isCopy: true,
      isMove: false,
      isSameVolumeMove: false,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('same-volume move (rollback disabled) has no a11y violations', async () => {
    const target = mountDialog({
      conflictEvent: fileConflict(),
      isCopy: false,
      isMove: true,
      isSameVolumeMove: true,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('type-mismatch (folder over file) conflict has no a11y violations', async () => {
    const target = mountDialog({
      conflictEvent: fileConflict({
        sourceIsDirectory: true,
        destinationIsDirectory: false,
        sourceSize: null,
        sizeDifference: null,
      }),
      isCopy: true,
      isMove: false,
      isSameVolumeMove: false,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('unknown sizes (network/MTP destination) conflict has no a11y violations', async () => {
    const target = mountDialog({
      conflictEvent: fileConflict({
        sourceSize: null,
        destinationSize: null,
        sourceModified: null,
        destinationModified: null,
        sizeDifference: null,
      }),
      isCopy: false,
      isMove: true,
      isSameVolumeMove: false,
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
