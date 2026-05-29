import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

import ViewerCopyDialogs from './ViewerCopyDialogs.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  formatBytes: (bytes: number) => `${String(bytes)} bytes`,
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('ViewerCopyDialogs a11y', () => {
  it('confirm dialog has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerCopyDialogs, {
      target,
      props: {
        confirmBytes: 5000,
        refuseBytes: null,
        onCancelConfirm: () => {},
        onProceedConfirm: () => {},
        onDismissRefuse: () => {},
        onSaveAs: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
  })

  it('refuse dialog has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerCopyDialogs, {
      target,
      props: {
        confirmBytes: null,
        refuseBytes: 200_000_000,
        onCancelConfirm: () => {},
        onProceedConfirm: () => {},
        onDismissRefuse: () => {},
        onSaveAs: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
  })
})
