/**
 * Tests for the OS-mount fallback notice's body: it names the share, its button
 * runs the ONE shared upgrade flow, and the notice retires itself exactly when it
 * has nothing left to say.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import type { DirectConnectOutcome } from './direct-connect'

const { connectDirectly } = vi.hoisted(() => ({
  connectDirectly: vi.fn<(target: { volumeId: string; shareName: string }) => Promise<DirectConnectOutcome>>(),
}))
vi.mock('./direct-connect', () => ({ connectDirectly }))

const { dismissToast } = vi.hoisted(() => ({ dismissToast: vi.fn() }))
vi.mock('$lib/ui/toast', () => ({ dismissToast }))

const { openLocalNetworkSettings } = vi.hoisted(() => ({
  openLocalNetworkSettings: vi.fn<() => Promise<void>>(() => Promise.resolve()),
}))
vi.mock('$lib/tauri-commands', () => ({ openLocalNetworkSettings }))

import { systemStrings } from '$lib/system-strings.svelte'

import SmbOsMountFallbackToastContent from './SmbOsMountFallbackToastContent.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SmbOsMountFallbackToastContent, {
    target,
    props: { toastId: 'smb-os-mount:smb-archive', volumeId: 'smb-archive', share: 'archive', retryable: true },
  })
  flushSync()
  const button = target.querySelector('button')
  if (!button) throw new Error('the notice has no button to press')
  return { target, button }
}

beforeEach(() => {
  connectDirectly.mockReset()
  connectDirectly.mockResolvedValue('connected')
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

  it('reuses the shared upgrade flow rather than a second way to connect, naming the share for its answer', async () => {
    const { button } = render()

    button.click()
    await vi.waitFor(() => {
      expect(connectDirectly).toHaveBeenCalledWith({ volumeId: 'smb-archive', shareName: 'archive' })
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

  it('retires the notice when the share is gone, because no retry can bring it back', async () => {
    connectDirectly.mockResolvedValue('gone')
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

  it('says what to switch when this Mac is what blocked the connection, and still offers the retry', async () => {
    // ERR-XGS9X: switching the Local Network permission off and on fixed it at
    // once, so the retry is worth keeping right beside the way to that switch.
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SmbOsMountFallbackToastContent, {
      target,
      props: {
        toastId: 'smb-os-mount:smb-sven',
        volumeId: 'smb-sven',
        share: 'Sven',
        retryable: true,
        blockedServer: 'Mars',
      },
    })
    flushSync()

    expect(target.textContent).toContain('Mars')
    expect(target.textContent).toContain(systemStrings.localNetwork)
    const buttons = [...target.querySelectorAll('button')]
    const openSettings = buttons.find((b) => b.textContent.includes(systemStrings.localNetwork))
    const retry = buttons.find((b) => b !== openSettings)
    if (!openSettings || !retry) throw new Error('expected both the settings button and the retry')

    openSettings.click()
    expect(openLocalNetworkSettings).toHaveBeenCalledOnce()
    retry.click()
    await vi.waitFor(() => {
      expect(connectDirectly).toHaveBeenCalledWith({ volumeId: 'smb-sven', shareName: 'Sven' })
    })
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
