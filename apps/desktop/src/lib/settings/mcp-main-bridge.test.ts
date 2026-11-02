/**
 * The MCP settings bridge's `set_setting` handler. An AI client drives it with no person
 * confirming anything, so a setting that records a person's consent answer is refused with a
 * typed refusal: the registry marks it (`mcpSettable: false`), the bridge never matches ids.
 * A data-sharing switch (`mcpSettable: 'offOnly'`) lets a client help someone opt out, never in.
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
vi.mock('#lib/shortcuts/index.js', () => ({
  getEffectiveShortcuts: () => [],
  getDefaultShortcuts: () => [],
  isShortcutModified: () => false,
}))
vi.mock('#lib/commands/command-registry.js', () => ({ commands: [] }))

// The organization's policy, as `locked_settings` would name it.
const managedLocks = vi.hoisted(() => new Map<string, unknown>())
vi.mock('#lib/managed-policy/managed-policy.svelte.js', () => ({
  getSettingLock: (id: string) => managedLocks.get(id),
  isSettingManaged: (id: string) => managedLocks.has(id),
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { cleanupMcpMainBridge, setupMcpMainBridge } from './mcp-main-bridge'
import { getSettingDefinition, settingsRegistry } from './settings-registry'
import { getSetting } from './settings-store'

/** Every setting that records a person's consent answer. */
const CONSENT_BEARING = [
  'askCmdr.consentRevokePending',
  'ai.cloudConsentRevokePending',
  // Not consent itself, but on Local it starts a proactive loop, so a client mustn't switch it on.
  'askCmdr.enabled',
  'onboarding.termsAcceptedVersion',
  'onboarding.termsAcceptedAt',
] as const

/** Data-sharing switches: a client may turn them off for someone, never on. */
const OFF_ONLY = ['analytics.enabled', 'updates.crashReports', 'updates.errorReports'] as const

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

  // An MCP client must not be able to turn off the record of what MCP clients do, either way.
  it('refuses the security event log switch in both directions', async () => {
    expect(getSettingDefinition('securityLog.enabled')?.mcpSettable).toBe(false)
    for (const value of [false, true]) {
      const response = await setOverMcp('securityLog.enabled', value)
      expect(response, String(value)).toMatchObject({ ok: false, refusal: 'notSettableOverMcp' })
    }
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('turns a data-sharing switch off, so a client can help someone opt out', async () => {
    for (const id of OFF_ONLY) {
      expect(getSettingDefinition(id)?.mcpSettable, id).toBe('offOnly')
      const response = await setOverMcp(id, false)
      expect(response, id).toMatchObject({ ok: true })
      expect(setSettingMock).toHaveBeenCalledWith(id, false)
    }
  })

  it('refuses to turn a data-sharing switch on, with a typed refusal', async () => {
    for (const id of OFF_ONLY) {
      for (const value of [true, 'false', 0]) {
        emitMock.mockClear() // each call reuses the request id
        const response = await setOverMcp(id, value)
        expect(response, `${id} = ${JSON.stringify(value)}`).toMatchObject({ ok: false, refusal: 'onlyOffOverMcp' })
      }
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

  it('tells the client which settings it may only turn off, and which not at all', async () => {
    const yaml = await readAllSettings()

    expect(block(yaml, 'analytics.enabled')).toContain('    mcpSettable: offOnly\n')
    expect(block(yaml, 'onboarding.termsAcceptedAt')).toContain('    mcpSettable: false\n')
    expect(block(yaml, 'updates.autoCheck')).not.toContain('mcpSettable')
  })
})

describe("`mcp-describe-setting`, what `set_setting`'s question reads", () => {
  async function describeSetting(settingId: string, value: unknown): Promise<Record<string, unknown> | null> {
    emitMock.mockClear()
    await handlers.get('mcp-describe-setting')?.({ payload: { requestId: 'r', settingId, value } })
    const reply = emitMock.mock.calls.at(-1)?.[1] as { ok: boolean; setting: Record<string, unknown> | null }
    expect(reply.ok).toBe(true)
    return reply.setting
  }

  beforeEach(async () => {
    await setupMcpMainBridge()
  })

  afterEach(() => {
    cleanupMcpMainBridge()
  })

  it('a setting the registry has never heard of describes as nothing, which refuses', async () => {
    expect(await describeSetting('made.up', true)).toBeNull()
  })

  it('names a consent answer as not settable', async () => {
    expect((await describeSetting('onboarding.termsAcceptedVersion', 1))?.mcpSettable).toBe(false)
  })

  it('names a data-sharing switch as off only', async () => {
    expect((await describeSetting('analytics.enabled', false))?.mcpSettable).toBe('offOnly')
  })

  it('says a value the registry refuses is invalid', async () => {
    expect((await describeSetting('theme.mode', 'plaid'))?.invalid).toEqual(expect.any(String))
    expect((await describeSetting('theme.mode', 'dark'))?.invalid).toBeNull()
  })

  it('never sends the real form of a secret', async () => {
    const definition = getSettingDefinition('analytics.email')
    expect(definition).toBeDefined()
    const original = definition?.component
    Object.assign(definition ?? {}, { component: 'password-input' })
    vi.mocked(getSetting).mockReturnValueOnce('hunter2')
    const described = await describeSetting('analytics.email', 'new')
    Object.assign(definition ?? {}, { component: original })
    expect(described?.sensitive).toBe(true)
    expect(described?.oldValue).toBe('********')
  })
})

describe('settings that change with no question', () => {
  // ❌ Growing this is a decision David reads: only looks, trivially reversed, no reach past the window.
  const ASK_FREE = [
    'theme.mode',
    'appearance.appColor',
    'appearance.sizeColors',
    'appearance.dateColors',
    'appearance.dateTimeFormat',
    'appearance.customDateTimeFormat',
    'listing.stripedRows',
    'appearance.tintLocal',
    'appearance.tintSmb',
    'appearance.tintMtp',
    'appearance.textSize',
    'appearance.uiDensity',
    'listing.sizeDisplay',
    'listing.sizeUnit',
    'appearance.fileSizeFormat',
    'listing.sizeMismatchWarning',
    'appearance.useAppIconsForDocuments',
    'appearance.showFunctionKeyBar',
    'listing.showExtensionInName',
    'listing.showTags',
    'listing.showHiddenFiles',
    'listing.foldersFirst',
    'listing.directorySortMode',
    'listing.briefColumnWidthMode',
    'listing.briefColumnWidthMaxPx',
    'viewer.wordWrap',
    'viewer.showTextCursor',
  ]

  it('are exactly these, so a new setting asks', () => {
    const marked = settingsRegistry.filter((def) => def.mcpAskFree === true).map((def) => def.id)
    expect([...marked].sort()).toEqual([...ASK_FREE].sort())
  })

  it('never includes one a client may not freely write', () => {
    for (const def of settingsRegistry) {
      if (def.mcpAskFree === true) expect(def.mcpSettable, def.id).toBeUndefined()
    }
  })
})
