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
