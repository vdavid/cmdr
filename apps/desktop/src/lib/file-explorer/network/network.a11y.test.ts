/**
 * Tier 3 a11y tests for the network browsing surfaces: the host list, a row's menu, its status
 * bar, the share list and its header, the OS-mount fallback toast, and the Local Network toast.
 *
 * One file per component would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions, including the three `it.skip`s parked on a real, unfixed
 * `aria-required-parent` violation. Adding a server and signing in to one both
 * live in the sign-in sheet now; its own blocks are
 * `$lib/servers/servers.a11y.test.ts`.
 *
 * One stub genuinely disagrees between blocks: `getShareState` is `undefined` for
 * the host list and a loaded result for the share list, so it reads a mutable each
 * block installs in its own `beforeEach`. The four `$lib/tauri-commands` sets are
 * disjoint, so their union is what each block always saw, and every `$lib/*` stub
 * spreads the real module first.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { flushSync, mount, tick } from 'svelte'
import ServersHub from './ServersHub.svelte'
import ServersHubRowMenu from './ServersHubRowMenu.svelte'
import PlacesBrowser from './PlacesBrowser.svelte'
import SmbOsMountFallbackToastContent from './SmbOsMountFallbackToastContent.svelte'
import LocalNetworkBlockedToastContent from './LocalNetworkBlockedToastContent.svelte'
import PlacesHeader from './PlacesHeader.svelte'
import ServersHubStatusBar from './ServersHubStatusBar.svelte'
import type { HubActions, HubRowMenuAPI } from './servers-hub-actions'
import type { HubRow } from './servers-hub-rows'
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
  noteNetworkAction: vi.fn(() => Promise.resolve()),
  setServersViewShown: vi.fn(() => Promise.resolve()),
  updateLeftPaneState: vi.fn(() => Promise.resolve()),
  updateRightPaneState: vi.fn(() => Promise.resolve()),
  showNetworkHostContextMenu: vi.fn(() => Promise.resolve()),
  onNetworkHostContextAction: vi.fn(() => Promise.resolve(() => {})),
  disconnectNetworkHost: vi.fn(() => Promise.resolve()),
  getUsernameHint: vi.fn(() => Promise.resolve(null)),
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

// These components share one jsdom document, the dialog portals into
// `document.body`, and axe resolves ARIA id references document-wide. Clearing
// between tests keeps each audit looking at its own container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `ServersHub.svelte`.
 *
 * Discovered-host list with a "Connect to server..." pseudo-row. Tauri
 * IPC, network-store getters, and the context-menu listener are stubbed
 * so the component can mount. Tests cover an empty list and a
 * populated list.
 */
describe('ServersHub a11y', () => {
  beforeEach(() => {
    mockShareState = undefined
  })

  // TODO: Host rows are `<div role="listitem">` but their parent container
  // has no `role="list"` (see ServersHub.svelte around the .host-list
  // block). Axe flags every row including the "Connect to server..."
  // pseudo-row as `aria-required-parent`. Fix: add `role="list"` to the
  // parent `.host-list` `<div>` (or replace with a proper `<ul>/<li>`
  // structure). Leaving skipped until fixed so the suite stays green.
  it.skip('empty host list (only connect row) has no a11y violations (BLOCKED: aria-required-parent)', async () => {
    mockHosts = []
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ServersHub, {
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
    mount(ServersHub, {
      target,
      props: { paneId: 'left', isFocused: true, onHostSelect: () => {}, onConnectToServer: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `PlacesBrowser.svelte`.
 *
 * Share listing for a host. Covers the loaded-with-shares state and the
 * auth-required one. The sign-in itself is the modal sheet, audited in
 * `$lib/servers/servers.a11y.test.ts`. Auto-mount and autoMountAttempted paths
 * are not exercised; those flow through the network-store into async mount IPC
 * which we just stub.
 */
describe('PlacesBrowser a11y', () => {
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
  // `role="list"` (PlacesBrowser.svelte around the .share-list block).
  // Same fix as ServersHub: add `role="list"` to the container
  // or replace with a proper `<ul>/<li>` structure.
  it.skip('loaded with shares has no a11y violations (BLOCKED: aria-required-parent)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PlacesBrowser, {
      target,
      props: {
        account: {
          protocol: 'smb',
          host: { id: 'h1', name: 'nas.local', hostname: 'nas.local', ipAddress: '10.0.0.10', port: 445 },
        },
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

/**
 * Tier 3 a11y for `ServersHubRowMenu.svelte`, OPEN: a one-place row's right-click menu, with
 * an action group, a greyed action, and a checkbox row below it. The menu portals out of its
 * container, so the audit looks at the whole document.
 */
describe('ServersHubRowMenu a11y', () => {
  const row: HubRow = {
    id: 'sftp-nas-local-22-ada',
    kind: 'server',
    parentId: null,
    account: null,
    place: null,
    name: 'Naspolya',
    protocol: 'sftp',
    address: 'nas.local:22',
    status: 'connected',
    lastConnectedAt: null,
    volumeId: 'sftp-nas-local-22-ada',
    pinned: true,
    saved: null,
    host: null,
  }

  const actions: HubActions = {
    forget: () => Promise.resolve(),
    rowMenu: () => ({
      actions: [
        { type: 'action', action: 'open', label: 'Open', icon: 'arrow-right' },
        { type: 'action', action: 'disconnect', label: 'Disconnect', icon: 'unplug', disabled: true },
      ],
      fixes: [],
      settings: [
        {
          type: 'toggle',
          toggle: 'auto-reconnect',
          label: 'Reconnect automatically',
          checked: true,
          tooltip: 'If the connection drops, Cmdr reconnects to this server on its own.',
        },
      ],
    }),
    runRowEntry: () => Promise.resolve(),
    openHostMenu: () => Promise.resolve(),
    runHostAction: () => Promise.resolve(),
    rowById: () => row,
  }

  it('the open menu has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(ServersHubRowMenu, { target, props: { actions } }) as unknown as HubRowMenuAPI
    flushSync()
    void instance.open(row, { x: 10, y: 10 }, null)
    // The surface portals itself into `document.body`, which lands a beat after the open.
    await vi.waitFor(() => {
      expect(document.querySelector('[data-menu]')).not.toBeNull()
    })
    await expectNoA11yViolations(document.body)
  })
})

/**
 * Tier 3 a11y for `PlacesHeader.svelte` with every button showing: Back, Sign in as…,
 * Use guest, and Forget saved password, next to the account and the share count.
 */
describe('PlacesHeader a11y', () => {
  it('the header with every action has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PlacesHeader, {
      target,
      props: {
        hostLabel: 'Naspolya',
        account: { kind: 'user', username: 'testuser' },
        canForgetPassword: true,
        shareCount: 3,
        onBack: () => {},
        onSignInAs: () => {},
        onUseGuest: () => {},
        onForgetPassword: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y for `ServersHubStatusBar.svelte`: the bar is one button, labelled, with a shortcut chip inside. */
describe('ServersHubStatusBar a11y', () => {
  it('the status bar has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ServersHubStatusBar, { target, props: { serverCount: 4, onRefresh: () => {} } })
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
      props: { toastId: 'smb-os-mount:smb-archive', volumeId: 'smb-archive', share: 'archive', retryable: true },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the this-Mac-blocked state, with its two buttons, has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SmbOsMountFallbackToastContent, {
      target,
      props: {
        toastId: 'smb-os-mount:smb-archive',
        volumeId: 'smb-archive',
        share: 'archive',
        retryable: true,
        blockedServer: 'Naspolya',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('LocalNetworkBlockedToastContent a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LocalNetworkBlockedToastContent, { target, props: { server: 'Naspolya' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
