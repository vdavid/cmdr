/**
 * Component tests for `McpServerSection.svelte`.
 *
 * Focus: the "Server is running on port X" copy adapts to the port setting. When the
 * setting is 0 (ephemeral), the section appends "(ephemeral)" so the user knows the
 * displayed port came from the kernel, not from their preference. When the setting is
 * non-zero, the existing "(port N was in use)" suffix appears only on mismatch.
 *
 * See docs/specs/instance-isolation-plan.md § P2.
 */

import { afterEach, beforeEach, describe, it, expect, vi } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'

const settingsState = {
  enabled: false as boolean,
  port: 0 as number,
}
const tauriState = {
  running: false as boolean,
  actualPort: null as number | null,
}

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'developer.mcpEnabled') return settingsState.enabled
    if (key === 'developer.mcpPort') return settingsState.port
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  invoke: vi.fn(() => Promise.resolve()),
  checkPortAvailable: vi.fn(() => Promise.resolve(true)),
  findAvailablePort: vi.fn(() => Promise.resolve(57821)),
  setMcpEnabled: vi.fn(() => Promise.resolve()),
  setMcpPort: vi.fn(() => Promise.resolve()),
  getMcpRunning: vi.fn(() => Promise.resolve(tauriState.running)),
  getMcpPort: vi.fn(() => Promise.resolve(tauriState.actualPort)),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: () => {} }),
}))

import McpServerSection from './McpServerSection.svelte'

async function render(): Promise<{ target: HTMLElement; component: ReturnType<typeof mount> }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(McpServerSection, { target, props: { searchQuery: '' } })
  flushSync()
  await tick()
  // syncState() is async; allow microtasks to settle so runningPort lands before assertions.
  await Promise.resolve()
  await tick()
  return { target, component }
}

describe('McpServerSection', () => {
  beforeEach(() => {
    settingsState.enabled = false
    settingsState.port = 0
    tauriState.running = false
    tauriState.actualPort = null
  })

  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('appends "(ephemeral)" when setting=0 and server is running', async () => {
    settingsState.enabled = true
    settingsState.port = 0
    tauriState.running = true
    tauriState.actualPort = 57821

    const { target, component } = await render()
    const text = target.textContent ?? ''
    expect(text).toContain('Server is running on port 57821')
    expect(text).toContain('(ephemeral)')
    unmount(component)
  })

  it('shows neither "(ephemeral)" nor "(in use)" when setting matches the bound port', async () => {
    settingsState.enabled = true
    settingsState.port = 19225
    tauriState.running = true
    tauriState.actualPort = 19225

    const { target, component } = await render()
    const text = target.textContent ?? ''
    expect(text).toContain('Server is running on port 19225')
    expect(text).not.toContain('(ephemeral)')
    expect(text).not.toContain('was in use')
    unmount(component)
  })

  it('shows the "(port N was in use)" copy when the setting pinned a port the server could not bind', async () => {
    settingsState.enabled = true
    settingsState.port = 19225
    tauriState.running = true
    tauriState.actualPort = 19226

    const { target, component } = await render()
    const text = target.textContent ?? ''
    expect(text).toContain('Server is running on port 19226')
    expect(text).toContain('(port 19225 was in use)')
    expect(text).not.toContain('(ephemeral)')
    unmount(component)
  })
})
