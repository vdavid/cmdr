/**
 * Tier 3 a11y tests for the network browsing surfaces: the connect dialog, the
 * host list, the login form, the share list, and the OS-mount fallback toast.
 *
 * One file per component would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions, including the three `it.skip`s parked on a real, unfixed
 * `aria-required-parent` violation.
 *
 * One stub genuinely disagrees between blocks: `getShareState` is `undefined` for
 * the host list and a loaded result for the share list, so it reads a mutable each
 * block installs in its own `beforeEach`. The four `$lib/tauri-commands` sets are
 * disjoint, so their union is what each block always saw, and every `$lib/*` stub
 * spreads the real module first.
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import ConnectToServerDialog from './ConnectToServerDialog.svelte'
import NetworkBrowser from './NetworkBrowser.svelte'
import NetworkLoginForm from './NetworkLoginForm.svelte'
import ShareBrowser from './ShareBrowser.svelte'
import SmbOsMountFallbackToastContent from './SmbOsMountFallbackToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let mockHosts: Array<{
  id: string
  name: string
  hostname?: string
  ipAddress?: string
  port: number
  source?: string
}> = []

// What `getShareState` answers: `undefined` for the host list, a loaded result for
// the share list. Each block installs its own in `beforeEach`.
let mockShareState: unknown = undefined

vi.mock('./network-store.svelte', () => ({
  getNetworkHosts: () => mockHosts,
  getDiscoveryState: () => 'idle',
  isHostResolving: () => false,
  getShareState: () => mockShareState,
  getShareCount: () => null,
  isListingShares: () => false,
  isShareDataStale: () => false,
  refreshAllStaleShares: vi.fn(),
  clearShareState: vi.fn(),
  setShareState: vi.fn(),
  setCredentialStatus: vi.fn(),
  fetchShares: vi.fn(() => Promise.resolve()),
  getCredentialStatus: () => 'unknown',
  checkCredentialsForHost: vi.fn(() => Promise.resolve()),
  forgetCredentials: vi.fn(() => Promise.resolve()),
}))

// The union of the network IPC the five components reach for. The real module is
// spread first so a call outside the union behaves as it does un-merged.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  connectToServer: vi.fn(() => Promise.resolve({ host: { id: 'h', name: 'nas.local' }, sharePath: null })),
  ensureNetworkDiscoveryStarted: vi.fn(() => Promise.resolve()),
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  updateRightPaneState: vi.fn(() => Promise.resolve()),
  removeManualServer: vi.fn(() => Promise.resolve()),
  showNetworkHostContextMenu: vi.fn(() => Promise.resolve()),
  onNetworkHostContextAction: vi.fn(() => Promise.resolve(() => {})),
  disconnectNetworkHost: vi.fn(() => Promise.resolve()),
  getUsernameHints: vi.fn(() => Promise.resolve({})),
  getKnownShareByName: vi.fn(() => Promise.resolve(null)),
  listSharesWithCredentials: vi.fn(() => Promise.resolve([])),
  saveSmbCredentials: vi.fn(() => Promise.resolve()),
  getSmbCredentials: vi.fn(() => Promise.resolve(null)),
  isUsingCredentialFileFallback: vi.fn(() => Promise.resolve(false)),
  updateKnownShare: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({
  confirmDialog: vi.fn(() => Promise.resolve(false)),
}))

vi.mock('$lib/ui/toast', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  addToast: vi.fn(() => 'id'),
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings/network-settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getNetworkTimeoutMs: () => 5000,
  getShareCacheTtlMs: () => 300000,
}))

vi.mock('./direct-connect', () => ({
  connectDirectly: vi.fn(() => Promise.resolve('connected')),
}))

vi.mock('./smb-login-hosts', () => ({
  promptForSmbCredentials: vi.fn(() => true),
}))

// These components share one jsdom document, the dialog portals into
// `document.body`, and axe resolves ARIA id references document-wide. Clearing
// between tests keeps each audit looking at its own container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `ConnectToServerDialog.svelte`.
 *
 * Modal for entering a server address. Covers the idle, connecting, and
 * error states.
 */
describe('ConnectToServerDialog a11y', () => {
  it('default (idle state) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ConnectToServerDialog, {
      target,
      props: {
        onConnect: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `NetworkBrowser.svelte`.
 *
 * Discovered-host list with a "Connect to server..." pseudo-row. Tauri
 * IPC, network-store getters, and the context-menu listener are stubbed
 * so the component can mount. Tests cover an empty list and a
 * populated list.
 */
describe('NetworkBrowser a11y', () => {
  beforeEach(() => {
    mockShareState = undefined
  })

  // TODO: Host rows are `<div role="listitem">` but their parent container
  // has no `role="list"` (see NetworkBrowser.svelte around the .host-list
  // block). Axe flags every row including the "Connect to server..."
  // pseudo-row as `aria-required-parent`. Fix: add `role="list"` to the
  // parent `.host-list` `<div>` (or replace with a proper `<ul>/<li>`
  // structure). Leaving skipped until fixed so the suite stays green.
  it.skip('empty host list (only connect row) has no a11y violations (BLOCKED: aria-required-parent)', async () => {
    mockHosts = []
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkBrowser, {
      target,
      props: { paneId: 'left', isFocused: false, onHostSelect: () => {}, onConnectToServer: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it.skip('populated host list has no a11y violations (BLOCKED: aria-required-parent)', async () => {
    mockHosts = [
      { id: 'h1', name: 'nas.local', hostname: 'nas.local', ipAddress: '10.0.0.10', port: 445 },
      { id: 'h2', name: 'printer.local', hostname: 'printer.local', ipAddress: '10.0.0.20', port: 445 },
    ]
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkBrowser, {
      target,
      props: { paneId: 'left', isFocused: true, onHostSelect: () => {}, onConnectToServer: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `NetworkLoginForm.svelte`.
 *
 * SMB credential form rendered inline inside a pane. Tests cover each
 * `authMode` value, the connecting state (submit disabled), and the
 * error-visible state. Username-hint IPC is stubbed.
 */
describe('NetworkLoginForm a11y', () => {
  const host = { id: 'host-1', name: 'nas.local', hostname: 'nas.local', ipAddress: '10.0.0.10', port: 445 }

  it('credentials-required mode (no guest option) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkLoginForm, {
      target,
      props: {
        host,
        authMode: 'creds_required',
        onConnect: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('guest-allowed mode (radio choice visible) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkLoginForm, {
      target,
      props: {
        host,
        shareName: 'Public',
        authMode: 'guest_allowed',
        onConnect: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('connecting state (disabled inputs + spinner button) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkLoginForm, {
      target,
      props: {
        host,
        authMode: 'creds_required',
        isConnecting: true,
        onConnect: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with error message visible has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkLoginForm, {
      target,
      props: {
        host,
        authMode: 'creds_required',
        errorMessage: 'Authentication failed: wrong password',
        onConnect: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `ShareBrowser.svelte`.
 *
 * Share listing for a host. Covers the loaded-with-shares state and
 * (via authMode via NetworkLoginForm) the auth-required state. Auto-
 * mount and autoMountAttempted paths are not exercised; those flow
 * through the network-store into async mount IPC which we just stub.
 */
describe('ShareBrowser a11y', () => {
  beforeEach(() => {
    mockShareState = {
      status: 'loaded',
      result: {
        shares: [
          { name: 'Public', type: 'disk' },
          { name: 'Media', type: 'disk' },
        ],
        authMode: 'guest_allowed',
      },
      fetchedAt: Date.now(),
    }
  })

  // TODO: Share rows are `<div role="listitem">` without a parent
  // `role="list"` (ShareBrowser.svelte around the .share-list block).
  // Same fix as NetworkBrowser: add `role="list"` to the container
  // or replace with a proper `<ul>/<li>` structure.
  it.skip('loaded with shares has no a11y violations (BLOCKED: aria-required-parent)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ShareBrowser, {
      target,
      props: {
        host: { id: 'h1', name: 'nas.local', hostname: 'nas.local', ipAddress: '10.0.0.10', port: 445 },
        paneId: 'left',
        isFocused: true,
        onShareSelect: () => {},
        onBack: () => {},
      },
    })
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('SmbOsMountFallbackToastContent a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SmbOsMountFallbackToastContent, {
      target,
      props: { toastId: 'smb-os-mount:smb-archive', volumeId: 'smb-archive', share: 'archive' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
