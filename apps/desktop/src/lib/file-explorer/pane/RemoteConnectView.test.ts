/**
 * What `RemoteConnectView`'s buttons do. Its states and their a11y are pinned in
 * `connection-views.a11y.test.ts`; this pins that a button reaches the callback its
 * state hands over, since an inert button is the one thing the view refuses.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import RemoteConnectView from './RemoteConnectView.svelte'

describe('a changed host key', () => {
  it('opens the sheet on the new key from "Check the key", and disconnects from Disconnect', async () => {
    const checkKey = vi.fn()
    const disconnect = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RemoteConnectView, {
      target,
      props: { name: 'Naspolya', state: { kind: 'host_key_changed', checkKey, disconnect } },
    })
    await tick()

    const buttons = [...target.querySelectorAll('button')]
    const check = buttons.find((button) => button.textContent.trim() === 'Check the key')
    expect(check, 'the banner offers a way to see the key').toBeDefined()
    check?.click()
    expect(checkKey).toHaveBeenCalledTimes(1)
    expect(disconnect).not.toHaveBeenCalled()

    buttons.find((button) => button.textContent.trim() === 'Disconnect')?.click()
    expect(disconnect).toHaveBeenCalledTimes(1)
    target.remove()
  })
})

describe('a refused guest session', () => {
  it('names the refusal, and opens the sheet from "Try another password…" or disconnects (#457)', async () => {
    const signIn = vi.fn()
    const disconnect = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RemoteConnectView, {
      target,
      props: { name: 'Naspolya', state: { kind: 'guest_session_refused', signIn, disconnect } },
    })
    await tick()

    expect(target.textContent).toContain('Cmdr didn’t sign in to Naspolya as a guest')
    const buttons = [...target.querySelectorAll('button')]
    buttons.find((button) => button.textContent.trim() === 'Try another password…')?.click()
    expect(signIn).toHaveBeenCalledTimes(1)
    expect(disconnect).not.toHaveBeenCalled()
    buttons.find((button) => button.textContent.trim() === 'Disconnect')?.click()
    expect(disconnect).toHaveBeenCalledTimes(1)
    target.remove()
  })
})

describe('the address under the name', () => {
  it('shows what the pane is trying to reach, so a person can tell a LAN address from a VPN one (ERR-VZ4CV)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RemoteConnectView, {
      target,
      props: { name: 'Naspolya', address: '100.64.0.7:22', state: { kind: 'connecting', cancel: vi.fn() } },
    })
    await tick()

    expect(target.textContent).toContain('100.64.0.7:22')
    target.remove()
  })
})

describe('a single dial with its own hint', () => {
  it('says the producer’s sentence under the spinner instead of the generic one (#604)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RemoteConnectView, {
      target,
      props: {
        name: 'naspi',
        state: { kind: 'connecting', cancel: vi.fn(), hint: 'Still waiting for macOS to connect.' },
      },
    })
    await tick()

    expect(target.querySelector('.hint')?.textContent).toBe('Still waiting for macOS to connect.')
    expect(target.textContent).not.toContain('This can take a few seconds.')
    target.remove()
  })
})
