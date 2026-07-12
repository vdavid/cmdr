/**
 * Unit tests for the host-status classification extracted from `NetworkBrowser.svelte`.
 *
 * These pin the credential-aware mapping from a `NetworkHost` + network-store state to a typed
 * `HostStatus`, the error/tooltip nuance, and the three lookup maps that feed the UI and the MCP
 * host-name encoding. The store getters and `tString` are mocked so the pure logic is exercised
 * in isolation (the reason the logic lives in a `.ts` helper rather than the component).
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { NetworkHost } from '../types'
import {
  getHostStatus,
  isStatusError,
  getStatusTooltip,
  STATUS_TEXT_KEY,
  STATUS_ICON,
  STATUS_MCP_LABEL,
  type HostStatusKind,
} from './host-status'

const h = vi.hoisted(() => ({
  getShareState: vi.fn(),
  getCredentialStatus: vi.fn(),
  isHostResolving: vi.fn(),
  isShareDataStale: vi.fn(),
}))

vi.mock('./network-store.svelte', () => ({
  getShareState: h.getShareState,
  getCredentialStatus: h.getCredentialStatus,
  isHostResolving: h.isHostResolving,
  isShareDataStale: h.isShareDataStale,
}))

// Identity tString so tooltip assertions read the resolved message KEY (locale-independent).
vi.mock('$lib/intl/messages.svelte', () => ({
  tString: (key: string) => key,
}))

const host = (over: Partial<NetworkHost> = {}): NetworkHost => ({
  id: 'host-1',
  name: 'Naspolya',
  hostname: 'nas.local',
  port: 445,
  ...over,
})

const loading = { status: 'loading' as const }
const error = (type: string, message?: string) => ({ status: 'error' as const, error: { type, message }, fetchedAt: 0 })
const loaded = (authMode: string) => ({ status: 'loaded' as const, result: { authMode }, fetchedAt: 0 })

beforeEach(() => {
  h.getShareState.mockReturnValue(undefined)
  h.getCredentialStatus.mockReturnValue('unknown')
  h.isHostResolving.mockReturnValue(false)
  h.isShareDataStale.mockReturnValue(false)
})

const ALL_KINDS: HostStatusKind[] = [
  'resolving',
  'waitingForNetwork',
  'notChecked',
  'connecting',
  'loginNeeded',
  'loginFailed',
  'timeout',
  'unreachable',
  'error',
  'loggedIn',
  'loggedInOk',
  'guest',
  'connected',
]

describe('status lookup maps', () => {
  it('cover every HostStatusKind', () => {
    for (const kind of ALL_KINDS) {
      expect(STATUS_TEXT_KEY[kind], `text key for ${kind}`).toBeTruthy()
      expect(kind in STATUS_ICON, `icon entry for ${kind}`).toBe(true)
      expect(STATUS_MCP_LABEL[kind], `mcp label for ${kind}`).toBeTruthy()
    }
    expect(Object.keys(STATUS_TEXT_KEY)).toHaveLength(ALL_KINDS.length)
    expect(Object.keys(STATUS_MCP_LABEL)).toHaveLength(ALL_KINDS.length)
  })

  it('leave transient states icon-less and mark terminal ones with a glyph', () => {
    expect(STATUS_ICON.resolving).toBeNull()
    expect(STATUS_ICON.connecting).toBeNull()
    expect(STATUS_ICON.loginNeeded).toBe('lock')
    expect(STATUS_ICON.loggedInOk).toBe('check')
  })

  it('collapse loggedIn and loggedInOk to one stable MCP token', () => {
    expect(STATUS_MCP_LABEL.loggedIn).toBe('logged_in')
    expect(STATUS_MCP_LABEL.loggedInOk).toBe('logged_in')
  })
})

describe('getHostStatus', () => {
  it('reports resolving / waiting / not-checked when there is no share state yet', () => {
    h.isHostResolving.mockReturnValue(true)
    expect(getHostStatus(host()).kind).toBe('resolving')

    h.isHostResolving.mockReturnValue(false)
    expect(getHostStatus(host({ hostname: undefined })).kind).toBe('waitingForNetwork')
    expect(getHostStatus(host()).kind).toBe('notChecked')
  })

  it('maps a loading state to connecting', () => {
    h.getShareState.mockReturnValue(loading)
    expect(getHostStatus(host()).kind).toBe('connecting')
  })

  it('distinguishes auth-required by stored-credential status', () => {
    h.getShareState.mockReturnValue(error('auth_required'))

    h.getCredentialStatus.mockReturnValue('has_creds')
    expect(getHostStatus(host()).kind).toBe('loggedIn')

    h.getCredentialStatus.mockReturnValue('failed')
    expect(getHostStatus(host()).kind).toBe('loginFailed')

    h.getCredentialStatus.mockReturnValue('no_creds')
    expect(getHostStatus(host()).kind).toBe('loginNeeded')
  })

  it('maps the concrete error types', () => {
    h.getShareState.mockReturnValue(error('auth_failed'))
    expect(getHostStatus(host()).kind).toBe('loginFailed')
    h.getShareState.mockReturnValue(error('timeout'))
    expect(getHostStatus(host()).kind).toBe('timeout')
    h.getShareState.mockReturnValue(error('host_unreachable'))
    expect(getHostStatus(host()).kind).toBe('unreachable')
    h.getShareState.mockReturnValue(error('something_else'))
    expect(getHostStatus(host()).kind).toBe('error')
  })

  it('sets hasInfo on an error state (a tooltip explains it)', () => {
    h.getShareState.mockReturnValue(error('timeout'))
    expect(getHostStatus(host()).hasInfo).toBe(true)
  })

  it('maps a loaded state by credentials then auth mode, carrying the stale flag', () => {
    h.getCredentialStatus.mockReturnValue('has_creds')
    h.getShareState.mockReturnValue(loaded('creds_required'))
    expect(getHostStatus(host())).toMatchObject({ kind: 'loggedInOk', stale: false })

    h.isShareDataStale.mockReturnValue(true)
    expect(getHostStatus(host())).toMatchObject({ kind: 'loggedInOk', stale: true })

    h.getCredentialStatus.mockReturnValue('unknown')
    h.getShareState.mockReturnValue(loaded('guest_allowed'))
    expect(getHostStatus(host())).toMatchObject({ kind: 'guest', stale: true })

    h.getShareState.mockReturnValue(loaded('creds_required'))
    expect(getHostStatus(host()).kind).toBe('connected')
  })
})

describe('isStatusError', () => {
  it('is false without an error state', () => {
    expect(isStatusError(host())).toBe(false)
    h.getShareState.mockReturnValue(loaded('guest_allowed'))
    expect(isStatusError(host())).toBe(false)
  })

  it('treats auth-required as an error only once login actually failed', () => {
    h.getShareState.mockReturnValue(error('auth_required'))
    h.getCredentialStatus.mockReturnValue('failed')
    expect(isStatusError(host())).toBe(true)
    h.getCredentialStatus.mockReturnValue('has_creds')
    expect(isStatusError(host())).toBe(false)
    h.getCredentialStatus.mockReturnValue('no_creds')
    expect(isStatusError(host())).toBe(false)
  })

  it('treats timeout / unreachable / auth-failed as real errors', () => {
    for (const type of ['timeout', 'host_unreachable', 'auth_failed']) {
      h.getShareState.mockReturnValue(error(type))
      expect(isStatusError(host()), type).toBe(true)
    }
  })
})

describe('getStatusTooltip', () => {
  it('explains the no-state cases', () => {
    h.isHostResolving.mockReturnValue(true)
    expect(getStatusTooltip(host())).toBe('fileExplorer.network.browser.tooltip.resolving')
    h.isHostResolving.mockReturnValue(false)
    expect(getStatusTooltip(host({ hostname: undefined }))).toBe(
      'fileExplorer.network.browser.tooltip.waitingForNetwork',
    )
    expect(getStatusTooltip(host())).toBe('fileExplorer.network.browser.tooltip.doubleClickToConnect')
  })

  it('gives credential-nuanced text for an auth-required error', () => {
    h.getShareState.mockReturnValue(error('auth_required'))
    h.getCredentialStatus.mockReturnValue('has_creds')
    expect(getStatusTooltip(host())).toBe('fileExplorer.network.browser.tooltip.credsStored')
    h.getCredentialStatus.mockReturnValue('failed')
    expect(getStatusTooltip(host())).toBe('fileExplorer.network.browser.tooltip.credsRejected')
    h.getCredentialStatus.mockReturnValue('no_creds')
    expect(getStatusTooltip(host())).toBe('fileExplorer.network.browser.tooltip.requiresLogin')
  })

  it('prefers a concrete error message, falling back to the typed key', () => {
    h.getShareState.mockReturnValue(error('auth_failed'))
    expect(getStatusTooltip(host())).toBe('fileExplorer.network.browser.tooltip.authFailed')
    h.getShareState.mockReturnValue(error('host_unreachable', 'Host is down'))
    expect(getStatusTooltip(host())).toBe('Host is down')
    h.getShareState.mockReturnValue(error('host_unreachable'))
    expect(getStatusTooltip(host())).toBe('fileExplorer.network.browser.tooltip.errorWithType')
  })

  it('has no tooltip for a healthy loaded state', () => {
    h.getShareState.mockReturnValue(loaded('guest_allowed'))
    expect(getStatusTooltip(host())).toBeUndefined()
  })
})
