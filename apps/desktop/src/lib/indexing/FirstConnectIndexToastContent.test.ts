/**
 * Tests for the first-connect indexing prompt toast: each of the three buttons
 * runs its callback with the volume id and self-dismisses the toast.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import FirstConnectIndexToastContent from './FirstConnectIndexToastContent.svelte'

const dismissToast = vi.fn()
vi.mock('$lib/ui/toast', () => ({
  dismissToast: (id: string) => {
    dismissToast(id)
  },
}))

function must(root: ParentNode, label: string): HTMLButtonElement {
  const btn = [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === label)
  if (!btn) throw new Error(`no button labeled "${label}"`)
  return btn
}

function render() {
  const onEnable = vi.fn()
  const onSilenceDrive = vi.fn()
  const onSilenceAll = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FirstConnectIndexToastContent, {
    target,
    props: {
      toastId: 'toast-1',
      volumeId: 'smb-backups',
      volumeName: 'Backups',
      onEnable,
      onSilenceDrive,
      onSilenceAll,
    },
  })
  flushSync()
  return { target, onEnable, onSilenceDrive, onSilenceAll }
}

beforeEach(() => {
  dismissToast.mockClear()
})

describe('FirstConnectIndexToastContent', () => {
  it('shows the drive name in the heading', () => {
    const { target } = render()
    expect(target.textContent).toContain('Backups')
  })

  it('"Enable indexing" enables the drive and dismisses', () => {
    const { target, onEnable } = render()
    must(target, 'Enable indexing').click()
    flushSync()
    expect(onEnable).toHaveBeenCalledWith('smb-backups')
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })

  it('"Don\'t ask again for this drive" silences the drive and dismisses', () => {
    const { target, onSilenceDrive } = render()
    must(target, "Don't ask again for this drive").click()
    flushSync()
    expect(onSilenceDrive).toHaveBeenCalledWith('smb-backups')
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })

  it('"Don\'t ask again for any drives" silences all and dismisses', () => {
    const { target, onSilenceAll } = render()
    must(target, "Don't ask again for any drives").click()
    flushSync()
    expect(onSilenceAll).toHaveBeenCalledTimes(1)
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })
})
