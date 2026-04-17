/**
 * Tier 3 a11y tests for `PtpcameradDialog.svelte`.
 *
 * macOS helper dialog for the ptpcamerad workaround.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import PtpcameradDialog from './PtpcameradDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
  getPtpcameradWorkaroundCommand: vi.fn(() =>
    Promise.resolve('sudo launchctl kickstart -k gui/501/com.apple.ptpcamerad'),
  ),
}))

describe('PtpcameradDialog a11y', () => {
  it('with blocking process name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PtpcameradDialog, {
      target,
      props: { blockingProcess: 'pid 45145, ptpcamerad', onClose: () => {}, onRetry: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('without blocking process name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PtpcameradDialog, {
      target,
      props: { onClose: () => {}, onRetry: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
