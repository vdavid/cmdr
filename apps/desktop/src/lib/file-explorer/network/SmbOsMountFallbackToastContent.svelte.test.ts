/**
 * Tests for the OS-mount fallback notice's body: it names the share, its button
 * runs the ONE shared upgrade flow, and the notice retires itself exactly when it
 * has nothing left to say.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import type { DirectConnectOutcome } from './direct-connect'

const { connectDirectly } = vi.hoisted(() => ({
  connectDirectly: vi.fn<(volumeId: string, raise: unknown) => Promise<DirectConnectOutcome>>(),
}))
vi.mock('./direct-connect', () => ({ connectDirectly }))

const { promptForSmbCredentials } = vi.hoisted(() => ({ promptForSmbCredentials: vi.fn(() => true) }))
vi.mock('./smb-login-hosts', () => ({ promptForSmbCredentials }))

const { dismissToast } = vi.hoisted(() => ({ dismissToast: vi.fn() }))
vi.mock('$lib/ui/toast', () => ({ dismissToast }))

import SmbOsMountFallbackToastContent from './SmbOsMountFallbackToastContent.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SmbOsMountFallbackToastContent, {
    target,
    props: { toastId: 'smb-os-mount:smb-archive', volumeId: 'smb-archive', share: 'archive' },
  })
  flushSync()
  const button = target.querySelector('button')
  if (!button) throw new Error('the notice has no button to press')
  return { target, button }
}

beforeEach(() => {
  connectDirectly.mockReset()
  connectDirectly.mockResolvedValue('connected')
  promptForSmbCredentials.mockClear()
  dismissToast.mockClear()
})

describe('SmbOsMountFallbackToastContent', () => {
  it('names the share, so the user knows which one went slow', () => {
    const { target } = render()

    expect(target.textContent).toContain('archive')
  })

  it('sets the share name apart, because it is the one word the sentence is about', () => {
    const { target } = render()

    expect(target.querySelector('strong')?.textContent).toBe('archive')
  })

  it('reuses the shared upgrade flow rather than a second way to connect', async () => {
    const { button } = render()

    button.click()
    await vi.waitFor(() => {
      expect(connectDirectly).toHaveBeenCalledWith('smb-archive', promptForSmbCredentials)
    })
  })

  it('retires the notice once the share is direct', async () => {
    const { button } = render()

    button.click()
    await vi.waitFor(() => {
      expect(dismissToast).toHaveBeenCalledWith('smb-os-mount:smb-archive')
    })
  })

  it('retires the notice when the credential form takes over, so it does not shadow the form', async () => {
    connectDirectly.mockResolvedValue('askingForCredentials')
    const { button } = render()

    button.click()
    await vi.waitFor(() => {
      expect(dismissToast).toHaveBeenCalledWith('smb-os-mount:smb-archive')
    })
  })

  it('stays up when the retry lands right back on the OS mount, so the button can be pressed again', async () => {
    connectDirectly.mockResolvedValue('stillOnOsMount')
    const { button } = render()

    button.click()
    await vi.waitFor(() => {
      expect(connectDirectly).toHaveBeenCalled()
    })
    flushSync()

    expect(dismissToast).not.toHaveBeenCalled()
    expect(button.disabled).toBe(false)
  })

  it('ignores a second press while the first attempt is still running', () => {
    let settle: (outcome: DirectConnectOutcome) => void = () => {}
    connectDirectly.mockReturnValue(
      new Promise<DirectConnectOutcome>((resolve) => {
        settle = resolve
      }),
    )
    const { button } = render()

    button.click()
    flushSync()
    button.click()
    flushSync()

    expect(connectDirectly).toHaveBeenCalledTimes(1)
    settle('connected')
  })
})
