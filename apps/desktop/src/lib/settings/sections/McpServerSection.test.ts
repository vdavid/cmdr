/**
 * Component tests for `McpServerSection.svelte`.
 *
 * Focus: the "Server is running on port X" copy adapts to the port setting. When the
 * setting is 0 (ephemeral), the section appends "(ephemeral)" so the user knows the
 * displayed port came from the kernel, not from their preference. When the setting is
 * non-zero, the existing "(port N was in use)" suffix appears only on mismatch.
 *
 * See docs/tooling/instance-isolation.md § "Per-resource breakdown" (Cmdr MCP HTTP port row).
 */

import { afterEach, beforeEach, describe, it, expect, vi, type Mock } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'

const settingsState = {
  enabled: false as boolean,
  port: 0 as number,
}
const tauriState = {
  running: false as boolean,
  actualPort: null as number | null,
}
// Captured `onSpecificSettingChange` callbacks, keyed by setting id, so tests can simulate
// a user editing the port or flipping the enabled toggle (the component reacts to these).
const settingChangeCallbacks = new Map<string, (id: string, value: unknown) => void>()

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'developer.mcpEnabled') return settingsState.enabled
    if (key === 'developer.mcpPort') return settingsState.port
    return undefined
  }),
  setSetting: vi.fn((key: string, value: unknown) => {
    if (key === 'developer.mcpEnabled') settingsState.enabled = value as boolean
    if (key === 'developer.mcpPort') settingsState.port = value as number
    return Promise.resolve()
  }),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn((id: string, cb: (id: string, value: unknown) => void) => {
    settingChangeCallbacks.set(id, cb)
    return () => settingChangeCallbacks.delete(id)
  }),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  invoke: vi.fn(() => Promise.resolve()),
  checkPortAvailable: vi.fn(() => Promise.resolve(true)),
  findAvailablePort: vi.fn(() => Promise.resolve(57821)),
  setMcpEnabled: vi.fn(() => Promise.resolve({ kind: 'stopped' })),
  setMcpPort: vi.fn(() => Promise.resolve({ kind: 'stopped' })),
  getMcpRunning: vi.fn(() => Promise.resolve(tauriState.running)),
  getMcpPort: vi.fn(() => Promise.resolve(tauriState.actualPort)),
}))

import { checkPortAvailable, findAvailablePort, setMcpEnabled, setMcpPort } from '$lib/tauri-commands'

/** Drain the microtask queue so an enqueued operation chain (apply → outcome → sync) settles. */
async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve()
}

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
    settingChangeCallbacks.clear()
    ;(checkPortAvailable as Mock).mockClear()
    ;(setMcpPort as Mock).mockResolvedValue({ kind: 'stopped' })
    ;(setMcpEnabled as Mock).mockResolvedValue({ kind: 'stopped' })
    ;(findAvailablePort as Mock).mockResolvedValue(57821)
  })

  afterEach(() => {
    document.body.innerHTML = ''
    vi.useRealTimers()
  })

  it('appends "(ephemeral)" when setting=0 and server is running', async () => {
    settingsState.enabled = true
    settingsState.port = 0
    tauriState.running = true
    tauriState.actualPort = 57821

    const { target, component } = await render()
    const text = target.textContent
    expect(text).toContain('Server is running on port 57821')
    expect(text).toContain('(ephemeral)')
    void unmount(component)
  })

  it('shows neither "(ephemeral)" nor "(in use)" when setting matches the bound port', async () => {
    settingsState.enabled = true
    settingsState.port = 19225
    tauriState.running = true
    tauriState.actualPort = 19225

    const { target, component } = await render()
    const text = target.textContent
    expect(text).toContain('Server is running on port 19225')
    expect(text).not.toContain('(ephemeral)')
    expect(text).not.toContain('was in use')
    void unmount(component)
  })

  it('shows the "(port N was in use)" copy when the setting pinned a port the server could not bind', async () => {
    settingsState.enabled = true
    settingsState.port = 19225
    tauriState.running = true
    tauriState.actualPort = 19226

    const { target, component } = await render()
    const text = target.textContent
    expect(text).toContain('Server is running on port 19226')
    expect(text).toContain('(port 19225 was in use)')
    expect(text).not.toContain('(ephemeral)')
    void unmount(component)
  })

  it('keeps the server running and offers an alternative when a port change hits a busy port', async () => {
    settingsState.enabled = true
    settingsState.port = 19225
    tauriState.running = true
    tauriState.actualPort = 19225

    const { target, component } = await render()
    expect(target.textContent).toContain('Server is running on port 19225')

    // User bumps the port to a busy one: backend leaves the server on 19225 (zero-downtime).
    vi.useFakeTimers()
    settingsState.port = 19300
    ;(setMcpPort as Mock).mockResolvedValueOnce({ kind: 'portInUse', requested: 19300 })
    ;(findAvailablePort as Mock).mockResolvedValueOnce(57821)

    settingChangeCallbacks.get('developer.mcpPort')?.('developer.mcpPort', 19300)
    await vi.advanceTimersByTimeAsync(800) // elapse the change debounce
    await flushMicrotasks()
    await tick()

    const text = target.textContent
    // Still running on the original port, plus an honest in-use notice and a suggestion.
    expect(text).toContain('still running on port 19225')
    expect(text).toContain('Port 19300 is in use')
    expect(text).toContain('Use port 57821 instead')
    // The "(was in use)" startup-probe suffix is suppressed so we don't say it twice.
    expect(text).not.toContain('(port 19300 was in use)')
    expect(setMcpPort).toHaveBeenCalledWith(19300)
    void unmount(component)
  })

  it('does not probe the port the server is already bound to when "Check port" is clicked', async () => {
    settingsState.enabled = true
    settingsState.port = 19225
    tauriState.running = true
    tauriState.actualPort = 19225

    const { target, component } = await render()
    const checkBtn = [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Check port'))
    expect(checkBtn).toBeTruthy()
    checkBtn?.click()
    await flushMicrotasks()
    await tick()

    // The own-port guard short-circuits: no fresh probe (which would falsely report "in use").
    expect(checkPortAvailable).not.toHaveBeenCalled()
    expect(target.textContent).toContain('Server is running on port 19225')
    expect(target.textContent).not.toContain('is in use')
    void unmount(component)
  })

  it('surfaces an alternative when enabling on a busy port (server stays off)', async () => {
    settingsState.enabled = false
    settingsState.port = 19225
    tauriState.running = false
    tauriState.actualPort = null

    const { target, component } = await render()
    ;(setMcpEnabled as Mock).mockResolvedValueOnce({ kind: 'portInUse', requested: 19225 })
    ;(findAvailablePort as Mock).mockResolvedValueOnce(57821)

    // User flips the toggle on; the backend reports the pinned port busy and keeps the server off.
    settingChangeCallbacks.get('developer.mcpEnabled')?.('developer.mcpEnabled', true)
    await flushMicrotasks()
    await tick()

    const text = target.textContent
    expect(text).toContain('Port 19225 is in use')
    expect(text).toContain('Use port 57821 instead')
    expect(text).not.toContain('Server is running')
    void unmount(component)
  })
})
