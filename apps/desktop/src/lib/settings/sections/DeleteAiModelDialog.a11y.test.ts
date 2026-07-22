/**
 * Tier 3 a11y tests for `DeleteAiModelDialog.svelte`, plus the behaviour that
 * has to survive its extraction out of `AiLocalSection`: mid-delete, nothing
 * can cancel or re-fire the uninstall.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DeleteAiModelDialog from './DeleteAiModelDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

interface Handlers {
  onConfirm: ReturnType<typeof vi.fn>
  onCancel: ReturnType<typeof vi.fn>
}

async function mountDialog(isDeleting: boolean): Promise<{ target: HTMLElement } & Handlers> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onConfirm = vi.fn()
  const onCancel = vi.fn()
  mount(DeleteAiModelDialog, {
    target,
    props: { modelSizeFormatted: '4.1 GB', isDeleting, onConfirm, onCancel },
  })
  await tick()
  return { target, onConfirm, onCancel }
}

describe('DeleteAiModelDialog a11y', () => {
  it('idle has no a11y violations', async () => {
    const { target } = await mountDialog(false)
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('deleting has no a11y violations', async () => {
    const { target } = await mountDialog(true)
    await expectNoA11yViolations(target)
    target.remove()
  })
})

describe('DeleteAiModelDialog behaviour', () => {
  it('is an alertdialog reporting the delete-ai-model id', async () => {
    const { target } = await mountDialog(false)
    expect(target.querySelector('[role="alertdialog"]')).not.toBeNull()
    const { notifyDialogOpened } = await import('$lib/tauri-commands')
    expect(vi.mocked(notifyDialogOpened).mock.calls.map(([id]) => id)).toContain('delete-ai-model')
    target.remove()
  })

  it('confirms on Enter while idle', async () => {
    const { target, onConfirm } = await mountDialog(false)
    target
      .querySelector('[role="alertdialog"]')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await tick()
    expect(onConfirm).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it('ignores Enter while deleting, so an uninstall in flight can’t be re-fired', async () => {
    const { target, onConfirm } = await mountDialog(true)
    target
      .querySelector('[role="alertdialog"]')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await tick()
    expect(onConfirm).not.toHaveBeenCalled()
    target.remove()
  })

  it('disables both buttons while deleting', async () => {
    const { target } = await mountDialog(true)
    const buttons = [...target.querySelectorAll('button')].filter((b) => !b.className.includes('close'))
    expect(buttons.length).toBeGreaterThan(0)
    expect(buttons.every((b) => b.disabled)).toBe(true)
    target.remove()
  })
})
