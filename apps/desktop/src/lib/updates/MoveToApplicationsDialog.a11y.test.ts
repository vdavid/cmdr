/**
 * Tier 3 a11y + copy tests for `MoveToApplicationsDialog.svelte`.
 *
 * This dialog is the only thing an install running from a read-only spot ever hears about its
 * updates, so what it SAYS is the feature. The two blockers each get their own first paragraph;
 * the instruction under them is shared.
 */

import { describe, it, vi, expect } from 'vitest'
import { mount, tick } from 'svelte'
import MoveToApplicationsDialog from './MoveToApplicationsDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { BundleWriteBlocker } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

async function mountDialog(blocker: BundleWriteBlocker, onClose = vi.fn()): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MoveToApplicationsDialog, { target, props: { blocker, onClose } })
  await tick()
  return target
}

describe('MoveToApplicationsDialog', () => {
  it.each(['translocated', 'readOnlyVolume'] as const)('has no a11y violations for %s', async (blocker) => {
    const target = await mountDialog(blocker)
    await expectNoA11yViolations(target)
  })

  it('names the download-location arrangement when the app is translocated', async () => {
    const target = await mountDialog('translocated')
    expect(target.textContent).toContain('temporary read-only copy')
    expect(target.textContent).toContain('straight from where you downloaded it')
  })

  it('names the disk image when the bundle is on a read-only volume', async () => {
    const target = await mountDialog('readOnlyVolume')
    expect(target.textContent).toContain('disk image')
  })

  it('gives the same instruction either way, because the fix is the same', async () => {
    const instruction = 'drag it into your Applications folder'
    expect((await mountDialog('translocated')).textContent).toContain(instruction)
    expect((await mountDialog('readOnlyVolume')).textContent).toContain(instruction)
  })

  /**
   * `docs/style-guide.md`: error copy stays conversational and never uses these two words. This
   * dialog is the whole message for a permanently-stuck install, so it's the wrong place to
   * sound like a stack trace.
   */
  it.each(['translocated', 'readOnlyVolume'] as const)('avoids the words error and failed (%s)', async (blocker) => {
    const text = (await mountDialog(blocker)).textContent.toLowerCase()
    expect(text).not.toContain('error')
    expect(text).not.toContain('failed')
  })

  it('closes on its only button', async () => {
    const onClose = vi.fn()
    const target = await mountDialog('translocated', onClose)
    const button = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Got it')
    button?.click()
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})
