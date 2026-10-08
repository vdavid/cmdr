/**
 * Tests for the "this Mac is keeping Cmdr from connecting" toast body: it names
 * the server and the permission, and its button opens that permission's pane.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import { systemStrings } from '$lib/system-strings.svelte'

const { openLocalNetworkSettings } = vi.hoisted(() => ({
  openLocalNetworkSettings: vi.fn<() => Promise<void>>(() => Promise.resolve()),
}))
vi.mock('$lib/tauri-commands', () => ({ openLocalNetworkSettings }))

import LocalNetworkBlockedToastContent from './LocalNetworkBlockedToastContent.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(LocalNetworkBlockedToastContent, { target, props: { server: 'Mars' } })
  flushSync()
  const button = target.querySelector('button')
  if (!button) throw new Error('the toast has no button to press')
  return { target, button }
}

beforeEach(() => {
  openLocalNetworkSettings.mockClear()
})

describe('LocalNetworkBlockedToastContent', () => {
  it('names the server and the permission to switch', () => {
    const { target } = render()

    expect(target.textContent).toContain('Mars')
    expect(target.textContent).toContain(systemStrings.localNetwork)
  })

  it('opens the Local Network pane of System Settings', () => {
    const { button } = render()

    expect(button.textContent).toContain(systemStrings.localNetwork)
    button.click()
    expect(openLocalNetworkSettings).toHaveBeenCalledOnce()
  })
})
