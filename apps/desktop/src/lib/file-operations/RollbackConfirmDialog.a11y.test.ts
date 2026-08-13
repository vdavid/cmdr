/**
 * Tier 3 a11y test for `RollbackConfirmDialog.svelte`.
 *
 * The question in front of the one control that can destroy a file the user had
 * before the operation started, so its title, body, and two answers have to
 * reach a screen reader as one described dialog.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import RollbackConfirmDialog from './RollbackConfirmDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

beforeEach(() => {
  document.body.innerHTML = ''
})

async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(RollbackConfirmDialog, { target, props: { onConfirm: () => {}, onCancel: () => {} } })
  await tick()
  return target
}

describe('RollbackConfirmDialog a11y', () => {
  it('has no a11y violations', async () => {
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })

  it('describes itself with the sentence that says what will be deleted', async () => {
    const target = await mountDialog()
    const dialog = target.querySelector('[role="dialog"]')
    expect(dialog?.getAttribute('aria-describedby')).toBe('rollback-confirmation-body')
    expect(target.querySelector('#rollback-confirmation-body')?.textContent).toContain("won't come back")
  })
})
