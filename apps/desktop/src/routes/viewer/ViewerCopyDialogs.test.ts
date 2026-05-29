import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewerCopyDialogs from './ViewerCopyDialogs.svelte'

// Provide a working `formatBytes` (used by the dialog titles) while stubbing the
// IPC side-effects ModalDialog fires on mount/destroy.
vi.mock('$lib/tauri-commands', () => ({
  formatBytes: (bytes: number) => `${String(bytes)} bytes`,
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

beforeEach(() => {
  document.body.innerHTML = ''
})

interface MountOpts {
  confirmBytes?: number | null
  refuseBytes?: number | null
  onCancelConfirm?: () => void
  onProceedConfirm?: () => void
  onDismissRefuse?: () => void
  onSaveAs?: () => void
}

function mountDialogs(opts: MountOpts = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(ViewerCopyDialogs, {
    target,
    props: {
      confirmBytes: opts.confirmBytes ?? null,
      refuseBytes: opts.refuseBytes ?? null,
      onCancelConfirm: opts.onCancelConfirm ?? (() => {}),
      onProceedConfirm: opts.onProceedConfirm ?? (() => {}),
      onDismissRefuse: opts.onDismissRefuse ?? (() => {}),
      onSaveAs: opts.onSaveAs ?? (() => {}),
    },
  })
  return { target, instance }
}

describe('ViewerCopyDialogs', () => {
  it('renders nothing when both byte counts are null', async () => {
    const { instance } = mountDialogs()
    await tick()

    expect(document.getElementById('viewer-copy-confirm-title')).toBeNull()
    expect(document.getElementById('viewer-copy-refuse-title')).toBeNull()

    void unmount(instance)
  })

  it('shows the confirm dialog with the formatted byte count', async () => {
    const { instance } = mountDialogs({ confirmBytes: 5000 })
    await tick()

    const title = document.getElementById('viewer-copy-confirm-title')
    expect(title?.textContent).toContain('5000 bytes')

    void unmount(instance)
  })

  it('shows a size-free prompt for the unknown-size sentinel (-1)', async () => {
    const { instance } = mountDialogs({ confirmBytes: -1 })
    await tick()

    const title = document.getElementById('viewer-copy-confirm-title')
    expect(title?.textContent).toContain('Copy this selection to the clipboard?')

    void unmount(instance)
  })

  it('fires onProceedConfirm when Copy is clicked', async () => {
    const onProceedConfirm = vi.fn()
    const { instance } = mountDialogs({ confirmBytes: 5000, onProceedConfirm })
    await tick()

    const copyBtn = Array.from(document.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Copy')
    expect(copyBtn).toBeDefined()
    copyBtn?.click()
    await tick()

    expect(onProceedConfirm).toHaveBeenCalledTimes(1)

    void unmount(instance)
  })

  it('fires onSaveAs when "Save as file…" is clicked in the confirm dialog', async () => {
    const onSaveAs = vi.fn()
    const { instance } = mountDialogs({ confirmBytes: 5000, onSaveAs })
    await tick()

    const saveBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Save as file…',
    )
    saveBtn?.click()
    await tick()

    expect(onSaveAs).toHaveBeenCalledTimes(1)

    void unmount(instance)
  })

  it('shows the refuse dialog with the over-limit copy', async () => {
    const { instance } = mountDialogs({ refuseBytes: 200_000_000 })
    await tick()

    const title = document.getElementById('viewer-copy-refuse-title')
    expect(title).not.toBeNull()
    expect(title?.textContent).toContain('200000000 bytes')

    void unmount(instance)
  })

  it('fires onDismissRefuse when Cancel is clicked in the refuse dialog', async () => {
    const onDismissRefuse = vi.fn()
    const { instance } = mountDialogs({ refuseBytes: 200_000_000, onDismissRefuse })
    await tick()

    const cancelBtn = Array.from(document.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Cancel')
    cancelBtn?.click()
    await tick()

    expect(onDismissRefuse).toHaveBeenCalledTimes(1)

    void unmount(instance)
  })
})
