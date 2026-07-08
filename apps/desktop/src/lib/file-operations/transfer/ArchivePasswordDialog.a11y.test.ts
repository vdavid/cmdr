/**
 * Tier 3 a11y tests for `ArchivePasswordDialog.svelte`.
 *
 * Covers the first prompt and the wrong-attempt re-prompt. Tauri IPC is stubbed
 * so the dialog can mount cleanly in happy-dom.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ArchivePasswordDialog from './ArchivePasswordDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

describe('ArchivePasswordDialog a11y', () => {
  it('first prompt has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: false, onSubmit: () => {}, onCancel: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('wrong-attempt re-prompt has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: true, onSubmit: () => {}, onCancel: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
