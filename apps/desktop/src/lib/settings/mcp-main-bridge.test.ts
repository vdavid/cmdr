/**
 * The MCP settings bridge's `set_setting` handler. An AI client drives it with no person
 * confirming anything, so a setting that records a person's consent answer is refused with a
 * typed refusal: the registry marks it (`mcpSettable: false`), the bridge never matches ids.
 * Every other setting, hidden ones included, stays settable, which harness flows rely on.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const { handlers, emitMock, setSettingMock } = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => unknown>(),
  emitMock: vi.fn<(event: string, payload: unknown) => Promise<void>>(() => Promise.resolve()),
  setSettingMock: vi.fn<(...args: unknown[]) => void>(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, handler: (event: { payload: unknown }) => unknown) => {
    handlers.set(event, handler)
    return Promise.resolve(() => {
      handlers.delete(event)
    })
  },
  emit: (event: string, payload: unknown) => emitMock(event, payload),
}))
vi.mock('./settings-store', () => ({
  getSetting: vi.fn(),
  setSetting: (...args: unknown[]) => {
    setSettingMock(...args)
  },
  isModified: vi.fn(() => false),
}))
vi.mock('$lib/shortcuts', () => ({
  getEffectiveShortcuts: () => [],
  getDefaultShortcuts: () => [],
  isShortcutModified: () => false,
}))
vi.mock('$lib/commands/command-registry', () => ({ commands: [] }))

// The organization's policy, as `locked_settings` would name it.
const managedLocks = vi.hoisted(() => new Map<string, unknown>())
vi.mock('$lib/managed-policy/managed-policy.svelte', () => ({
  getSettingLock: (id: string) => managedLocks.get(id),
  isSettingManaged: (id: string) => managedLocks.has(id),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { cleanupMcpMainBridge, setupMcpMainBridge } from './mcp-main-bridge'
import { getSettingDefinition } from './settings-registry'

/** Every setting that records a person's consent answer. */
const CONSENT_BEARING = [
  'askCmdr.consentRevokePending',
  'ai.cloudConsentRevokePending',
  // Not consent itself, but on Local it starts a proactive loop, so a client mustn't switch it on.
  'askCmdr.enabled',
  'analytics.enabled',
  'updates.crashReports',
  'updates.errorReports',
  'onboarding.termsAcceptedVersion',
  'onboarding.termsAcceptedAt',
] as const

async function setOverMcp(settingId: string, value: unknown): Promise<unknown> {
  const handler = handlers.get('mcp-set-setting')
  if (!handler) throw new Error('mcp-set-setting has no listener')
  handler({ payload: { requestId: `req-${settingId}`, settingId, value } })
  await vi.waitFor(() => {
    expect(emitMock).toHaveBeenCalledWith('mcp-response', expect.objectContaining({ requestId: `req-${settingId}` }))
  })
  return emitMock.mock.calls.find(
    ([, payload]) => (payload as { requestId: string }).requestId === `req-${settingId}`,
  )?.[1]
}

beforeEach(async () => {
  managedLocks.clear()
  handlers.clear()
  emitMock.mockClear()
  setSettingMock.mockClear()
  await setupMcpMainBridge()
})

afterEach(() => {
  cleanupMcpMainBridge()
})

describe('MCP set_setting', () => {
  it('refuses to clear a held "no" to Ask Cmdr, with a typed refusal', async () => {
    // Pre-fix this wrote the flag and answered ok, so an AI client could undo the person's "no".
    const response = await setOverMcp('askCmdr.consentRevokePending', false)

    expect(setSettingMock).not.toHaveBeenCalled()
    expect(response).toMatchObject({ ok: false, refusal: 'notSettableOverMcp' })
  })

  it('refuses every setting that records a consent answer, and the registry is what marks them', async () => {
    for (const id of CONSENT_BEARING) {
      expect(getSettingDefinition(id)?.mcpSettable, id).toBe(false)
      const response = await setOverMcp(id, true)
      expect(response, id).toMatchObject({ ok: false, refusal: 'notSettableOverMcp' })
    }
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('still sets ordinary settings and other hidden ones, which harness flows rely on', async () => {
    for (const [id, value] of [
      ['appearance.language', 'system'],
      ['onboarding.completed', true],
    ] as const) {
      const response = await setOverMcp(id, value)
      expect(response, id).toMatchObject({ ok: true })
      expect(setSettingMock).toHaveBeenCalledWith(id, value)
    }
  })

  it('refuses a value the organization rules out, as the backstop to the backend’s own refusal', async () => {
    managedLocks.set('ai.provider', { kind: 'disallowedValues', values: ['cloud'], fallback: 'off' })

    expect(await setOverMcp('ai.provider', 'cloud')).toMatchObject({ ok: false, refusal: 'managedByOrganization' })
    expect(setSettingMock).not.toHaveBeenCalled()
    emitMock.mockClear() // the next call reuses the request id
    expect(await setOverMcp('ai.provider', 'local')).toMatchObject({ ok: true })
  })
})

describe('MCP settings resource', () => {
  async function readAllSettings(): Promise<string> {
    const handler = handlers.get('mcp-get-all-settings')
    if (!handler) throw new Error('mcp-get-all-settings has no listener')
    handler({ payload: { requestId: 'req-all' } })
    await vi.waitFor(() => {
      expect(emitMock).toHaveBeenCalledWith('mcp-response', expect.objectContaining({ requestId: 'req-all' }))
    })
    const response = emitMock.mock.calls.find(([, p]) => (p as { requestId: string }).requestId === 'req-all')?.[1]
    return (response as { data: string }).data
  }

  /** The YAML block for one setting, from its `- id:` line to the next one. */
  function block(yaml: string, id: string): string {
    const start = yaml.indexOf(`  - id: ${id}\n`)
    const end = yaml.indexOf('  - id: ', start + 1)
    return yaml.slice(start, end === -1 ? undefined : end)
  }

  it('marks a setting the organization manages, and only that one', async () => {
    managedLocks.set('analytics.enabled', { kind: 'fixed', value: false })
    const yaml = await readAllSettings()

    expect(block(yaml, 'analytics.enabled')).toContain('    managed: true\n')
    expect(block(yaml, 'updates.autoCheck')).not.toContain('managed:')
  })
})
