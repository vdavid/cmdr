/**
 * Tests for the "this machine is keeping Cmdr from connecting" toast body. On
 * macOS it names the server and the Local Network permission, and its button
 * opens that permission's pane. Windows and Linux have no such permission: there
 * it names the server and the firewall, and offers no pane to open.
 */
import { describe, it, expect, vi, beforeEach, onTestFinished } from 'vitest'
import { mount, flushSync } from 'svelte'
import { systemStrings } from '#lib/system-strings.svelte.js'
import { withCapabilities, withPlatform } from '#lib/platform/test-platform.js'

const { openLocalNetworkSettings } = vi.hoisted(() => ({
  openLocalNetworkSettings: vi.fn<() => Promise<void>>(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ openLocalNetworkSettings }))

import LocalNetworkBlockedToastContent from './LocalNetworkBlockedToastContent.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(LocalNetworkBlockedToastContent, { target, props: { server: 'Mars', toastId: 'local-network-blocked' } })
  flushSync()
  return { target, button: target.querySelector('button') }
}

beforeEach(() => {
  openLocalNetworkSettings.mockClear()
})

describe('LocalNetworkBlockedToastContent on macOS', () => {
  beforeEach(() => {
    onTestFinished(withPlatform('macos'))
    onTestFinished(withCapabilities('localNetworkPermission'))
  })

  it('names the server and the permission to switch', () => {
    const { target } = render()

    expect(target.textContent).toContain('Mars')
    expect(target.textContent).toContain(systemStrings.localNetwork)
  })

  it('opens the Local Network pane of System Settings', () => {
    const { button } = render()
    if (!button) throw new Error('the toast has no button to press')

    expect(button.textContent).toContain(systemStrings.localNetwork)
    button.click()
    expect(openLocalNetworkSettings).toHaveBeenCalledOnce()
  })
})

describe('LocalNetworkBlockedToastContent off macOS', () => {
  it.each([
    ['linux', 'this computer'],
    ['windows', 'this PC'],
  ] as const)('on %s, names the server and the firewall, with no permission and no pane', (platform, machine) => {
    onTestFinished(withPlatform(platform))
    const { target, button } = render()

    expect(target.textContent).toContain('Mars')
    expect(target.textContent).toContain(machine)
    expect(target.textContent).toContain('firewall')
    expect(target.textContent).not.toContain('this Mac')
    expect(target.textContent).not.toContain(systemStrings.localNetwork)
    expect(button).toBeNull()
  })
})
