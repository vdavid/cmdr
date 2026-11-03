/**
 * How each refused-access notice reaches the person (#457): its level, how it leaves, its id
 * (one toast per source, updated in place as the count grows), its group, and that a notice the
 * person dismissed stays quiet for the rest of the session.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AccessNotice, SecurityNoticeRaised } from '#lib/ipc/bindings.js'

const { addToast, dismissToast } = vi.hoisted(() => ({ addToast: vi.fn(), dismissToast: vi.fn() }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast, dismissToast }))

const unlisten = vi.fn()
let raised: ((payload: SecurityNoticeRaised) => void) | null = null
vi.mock('#lib/tauri-commands/index.js', () => ({
  onSecurityNoticeRaised: (callback: (payload: SecurityNoticeRaised) => void) => {
    raised = callback
    return Promise.resolve(unlisten)
  },
}))

import {
  SECURITY_NOTICE_GROUP,
  announceSecurityNotice,
  dismissSecurityNotice,
  startSecurityNotices,
  stopSecurityNotices,
  _resetSecurityNoticesForTest,
} from './security-notices'
import SecurityNoticeToastContent from './SecurityNoticeToastContent.svelte'

type Options = Record<string, unknown> & { onDismiss?: () => void }

function lastToast(): { content: unknown; options: Options } {
  const call = addToast.mock.calls.at(-1) as [unknown, Options]
  return { content: call[0], options: call[1] }
}

const origin: AccessNotice = { kind: 'mcpForeignOrigin' }
const smb: AccessNotice = { kind: 'smbSignatureRefused', volumeId: 'smb-nas-photos' }
const update: AccessNotice = { kind: 'updateNotGenuine', version: '0.53.0', check: 'minisign' }

beforeEach(() => {
  stopSecurityNotices()
  _resetSecurityNoticesForTest()
  vi.clearAllMocks()
  raised = null
})

describe('announceSecurityNotice', () => {
  it('raises a web page at the MCP server as a persistent warning', () => {
    announceSecurityNotice({ notice: origin, count: 1 })
    const { content, options } = lastToast()
    expect(content).toBe(SecurityNoticeToastContent)
    expect(options).toMatchObject({
      id: 'security-notice:mcp-foreign-origin',
      level: 'warn',
      dismissal: 'persistent',
      toastGroup: SECURITY_NOTICE_GROUP,
      maxInGroup: 3,
      props: { notice: origin, count: 1 },
    })
  })

  it('raises a refused server reply as a persistent warning, one per server', () => {
    announceSecurityNotice({ notice: smb, count: 1 })
    expect(lastToast().options).toMatchObject({
      id: 'security-notice:smb-signature:smb-nas-photos',
      level: 'warn',
      dismissal: 'persistent',
    })
    announceSecurityNotice({ notice: { kind: 'smbSignatureRefused', volumeId: 'smb-other' }, count: 1 })
    expect(lastToast().options.id).toBe('security-notice:smb-signature:smb-other')
  })

  it('raises an update that isn’t genuine as a persistent error that offers a report with the typed facts', () => {
    announceSecurityNotice({ notice: update, count: 1 })
    expect(lastToast().options).toMatchObject({
      id: 'security-notice:update-not-genuine:0.53.0',
      level: 'error',
      dismissal: 'persistent',
      report: { kind: 'updateNotGenuine', version: '0.53.0', check: 'minisign' },
    })
  })

  /** The backend re-raises with the grown count at most once a minute; the same id replaces the
   *  toast's content in place, so a client retrying in a loop never stacks a second one. */
  it('updates the same toast in place as the count grows', () => {
    announceSecurityNotice({ notice: origin, count: 1 })
    announceSecurityNotice({ notice: origin, count: 1_000 })
    const ids = addToast.mock.calls.map((call) => (call[1] as Options).id)
    expect(ids).toEqual(['security-notice:mcp-foreign-origin', 'security-notice:mcp-foreign-origin'])
    expect(lastToast().options.props).toMatchObject({ count: 1_000 })
  })

  it('stays quiet for the rest of the session once the person closes it', () => {
    announceSecurityNotice({ notice: origin, count: 1 })
    lastToast().options.onDismiss?.()
    announceSecurityNotice({ notice: origin, count: 7 })
    expect(addToast).toHaveBeenCalledOnce()
    // Another source still speaks.
    announceSecurityNotice({ notice: smb, count: 1 })
    expect(addToast).toHaveBeenCalledTimes(2)
  })

  it('stays quiet after its own Dismiss button too, which also closes it', () => {
    announceSecurityNotice({ notice: smb, count: 1 })
    dismissSecurityNotice(smb)
    expect(dismissToast).toHaveBeenCalledWith('security-notice:smb-signature:smb-nas-photos')
    announceSecurityNotice({ notice: smb, count: 2 })
    expect(addToast).toHaveBeenCalledOnce()
  })
})

describe('startSecurityNotices', () => {
  it('turns every raised event into its notice, and stops listening on stop', async () => {
    await startSecurityNotices()
    raised?.({ notice: origin, count: 1 })
    expect(addToast).toHaveBeenCalledOnce()
    stopSecurityNotices()
    expect(unlisten).toHaveBeenCalledOnce()
  })
})
