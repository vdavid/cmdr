/**
 * Host-status classification for the network browser.
 *
 * Pure logic that turns a `NetworkHost` plus the network store's current state into a typed
 * status descriptor, the matching Lucide glyph and localized text key, a stable MCP token, and
 * an error tooltip. Kept out of `NetworkBrowser.svelte` so the component stays presentation-only,
 * following the sibling `network-store.svelte.ts` "pure logic → a `*.ts` helper" convention.
 */
import type { IconName } from '$lib/ui/icons/icon-map'
import type { MessageKey } from '$lib/intl/keys.gen'
import { tString } from '$lib/intl/messages.svelte'
import type { NetworkHost } from '../types'
import { getShareState, getCredentialStatus, isHostResolving, isShareDataStale } from './network-store.svelte'

/**
 * One host's status as a TYPED descriptor, not a pre-rendered string. The visible cell
 * renders an icon + localized text from `kind`; the MCP host-name encoding uses a stable
 * locale-independent token. `stale` adds a refresh glyph on a loaded-but-stale row;
 * `hasInfo` adds an info glyph when a tooltip explains an error state. Keeping status typed
 * is what lets one source feed both the icon-bearing UI and the plain-text MCP feed without
 * an emoji lowest-common-denominator (AGENTS.md § no-string-matching).
 */
export type HostStatusKind =
  | 'resolving'
  | 'waitingForNetwork'
  | 'notChecked'
  | 'connecting'
  | 'loginNeeded'
  | 'loginFailed'
  | 'timeout'
  | 'unreachable'
  | 'error'
  | 'loggedIn'
  | 'loggedInOk'
  | 'guest'
  | 'connected'

export interface HostStatus {
  kind: HostStatusKind
  stale: boolean
  hasInfo: boolean
}

export const STATUS_TEXT_KEY: Record<HostStatusKind, MessageKey> = {
  resolving: 'fileExplorer.network.browser.status.resolving',
  waitingForNetwork: 'fileExplorer.network.browser.status.waitingForNetwork',
  notChecked: 'fileExplorer.network.browser.status.notChecked',
  connecting: 'fileExplorer.network.connecting',
  loginNeeded: 'fileExplorer.network.browser.status.loginNeeded',
  loginFailed: 'fileExplorer.network.browser.status.loginFailed',
  timeout: 'fileExplorer.network.browser.status.timeout',
  unreachable: 'fileExplorer.network.browser.status.unreachable',
  error: 'fileExplorer.network.browser.status.error',
  loggedIn: 'fileExplorer.network.browser.status.loggedIn',
  loggedInOk: 'fileExplorer.network.browser.status.loggedInOk',
  guest: 'fileExplorer.network.browser.status.guest',
  connected: 'fileExplorer.network.browser.status.connected',
}

// Lucide glyph per status (null = transient/neutral states show text only).
export const STATUS_ICON: Record<HostStatusKind, IconName | null> = {
  resolving: null,
  waitingForNetwork: null,
  notChecked: null,
  connecting: null,
  loginNeeded: 'lock',
  loginFailed: 'triangle-alert',
  timeout: 'clock',
  unreachable: 'circle-x',
  error: 'triangle-alert',
  loggedIn: 'key',
  loggedInOk: 'check',
  guest: 'check',
  connected: 'check',
}

// Stable, locale-independent token for the MCP host-name encoding (agents read this token,
// never the localized label, so the encoding doesn't drift with the UI language).
export const STATUS_MCP_LABEL: Record<HostStatusKind, string> = {
  resolving: 'resolving',
  waitingForNetwork: 'waiting_for_network',
  notChecked: 'not_checked',
  connecting: 'connecting',
  loginNeeded: 'login_needed',
  loginFailed: 'login_failed',
  timeout: 'timeout',
  unreachable: 'unreachable',
  error: 'error',
  loggedIn: 'logged_in',
  loggedInOk: 'logged_in',
  guest: 'guest',
  connected: 'connected',
}

function errorStatusKind(errorType: string, hostName: string): HostStatusKind {
  // Auth required - distinguish by whether we have stored credentials.
  if (errorType === 'auth_required' || errorType === 'signing_required') {
    const credStatus = getCredentialStatus(hostName)
    if (credStatus === 'has_creds') return 'loggedIn'
    if (credStatus === 'failed') return 'loginFailed'
    return 'loginNeeded'
  }
  if (errorType === 'auth_failed') return 'loginFailed'
  if (errorType === 'timeout') return 'timeout'
  if (errorType === 'host_unreachable') return 'unreachable'
  return 'error'
}

// Credential-aware status as a typed descriptor (see HostStatus).
export function getHostStatus(host: NetworkHost): HostStatus {
  const state = getShareState(host.id)

  // No state yet - show helpful status
  if (!state) {
    if (isHostResolving(host.id)) return { kind: 'resolving', stale: false, hasInfo: false }
    if (!host.hostname) return { kind: 'waitingForNetwork', stale: false, hasInfo: false }
    return { kind: 'notChecked', stale: false, hasInfo: false }
  }

  if (state.status === 'loading') return { kind: 'connecting', stale: false, hasInfo: false }

  if (state.status === 'error') {
    return {
      kind: errorStatusKind(state.error.type, host.name),
      stale: false,
      hasInfo: !!getStatusTooltip(host),
    }
  }

  // status === 'loaded'
  const stale = isShareDataStale(host.id)
  const credStatus = getCredentialStatus(host.name)

  // If we have credentials stored, show "Logged in" regardless of auth mode
  if (credStatus === 'has_creds') return { kind: 'loggedInOk', stale, hasInfo: false }
  // Guest access (no stored credentials)
  if (state.result.authMode === 'guest_allowed') return { kind: 'guest', stale, hasInfo: false }
  return { kind: 'connected', stale, hasInfo: false }
}

// Helper to check if status should be styled as an error
export function isStatusError(host: NetworkHost): boolean {
  const state = getShareState(host.id)
  if (!state || state.status !== 'error') return false

  // Auth required with no credentials is NOT an error, just needs action
  if (state.error.type === 'auth_required' || state.error.type === 'signing_required') {
    const credStatus = getCredentialStatus(host.name)
    // Only show as error if login actually failed
    return credStatus === 'failed'
  }

  // Other errors (timeout, unreachable, auth_failed) are real errors
  return true
}

// Helper to get error tooltip text with nuanced explanations
export function getStatusTooltip(host: NetworkHost): string | undefined {
  const state = getShareState(host.id)

  // No state - explain what's happening
  if (!state) {
    if (isHostResolving(host.id)) return tString('fileExplorer.network.browser.tooltip.resolving')
    if (!host.hostname) return tString('fileExplorer.network.browser.tooltip.waitingForNetwork')
    return tString('fileExplorer.network.browser.tooltip.doubleClickToConnect')
  }

  if (state.status === 'error') {
    // Auth required with credentials context
    if (state.error.type === 'auth_required' || state.error.type === 'signing_required') {
      const credStatus = getCredentialStatus(host.name)
      if (credStatus === 'has_creds') {
        return tString('fileExplorer.network.browser.tooltip.credsStored')
      }
      if (credStatus === 'failed') {
        return tString('fileExplorer.network.browser.tooltip.credsRejected')
      }
      return tString('fileExplorer.network.browser.tooltip.requiresLogin')
    }
    if (state.error.type === 'auth_failed') {
      return tString('fileExplorer.network.browser.tooltip.authFailed')
    }
    return (
      state.error.message || tString('fileExplorer.network.browser.tooltip.errorWithType', { reason: state.error.type })
    )
  }
  return undefined
}
