/**
 * Tier 3 a11y tests for `QuitConfirmationDialog.svelte`.
 *
 * The prompt the quit gate raises over everything else, so it has to be
 * answerable by keyboard and readable by a screen reader in every state it can
 * open in.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import QuitConfirmationDialog from './QuitConfirmationDialog.svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

function operation(operationId: string, operationType: OperationSnapshot['operationType']): OperationSnapshot {
  return {
    operationId,
    operationType,
    status: 'running',
    source: 'Holiday.mov',
    destination: operationType === 'copy' || operationType === 'move' ? 'Backup' : null,
    supportsRollback: true,
    error: null,
  }
}

async function renderDialog(operations: OperationSnapshot[], secondsLeft: number): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(QuitConfirmationDialog, {
    target,
    props: { operations, secondsLeft, onQuit: () => {}, onKeepWorking: () => {} },
  })
  await tick()
  return target
}

describe('QuitConfirmationDialog a11y', () => {
  it('one running copy has no a11y violations', async () => {
    await expectNoA11yViolations(await renderDialog([operation('op-1', 'copy')], 15))
  })

  it('several operations have no a11y violations', async () => {
    const operations = [
      operation('op-1', 'copy'),
      operation('op-2', 'move'),
      operation('op-3', 'delete'),
      operation('op-4', 'archive_edit'),
    ]
    await expectNoA11yViolations(await renderDialog(operations, 9))
  })

  it('the last second has no a11y violations', async () => {
    await expectNoA11yViolations(await renderDialog([operation('op-1', 'copy')], 1))
  })
})
