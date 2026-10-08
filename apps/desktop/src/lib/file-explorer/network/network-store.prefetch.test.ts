/**
 * Which hosts get their shares listed without anyone asking, and when (#324).
 *
 * Listing a host's shares means connecting to it and signing in, as a guest where
 * it lets one in. For a server the person saved, doing that once the Servers view
 * is on screen is the point: its share list is there the moment they open it. At
 * launch, with no Servers view open, nobody is about to open anything. For a host
 * Cmdr merely found on the network it is a sign-in nobody asked for, so that one
 * waits until the person opens it.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { NetworkHost } from '../types'
import type { SavedServer } from '$lib/tauri-commands'

const { ipc, events } = vi.hoisted(() => {
  const events = {
    hostFound: undefined as ((host: unknown) => void) | undefined,
    hostResolved: undefined as ((host: unknown) => void) | undefined,
  }
  return {
    events,
    ipc: {
      listNetworkHosts: vi.fn(),
      getNetworkDiscoveryState: vi.fn(() => Promise.resolve('idle')),
      resolveNetworkHost: vi.fn(),
      listSavedServers: vi.fn(),
      prefetchShares: vi.fn(() => Promise.resolve()),
      listSharesOnHost: vi.fn(() => Promise.resolve({ shares: [], authMode: 'guest_allowed' })),
      setServersViewShown: vi.fn(() => Promise.resolve()),
      onNetworkHostFound: vi.fn((handler: (host: unknown) => void) => {
        events.hostFound = handler
        return Promise.resolve(() => {})
      }),
      onNetworkHostLost: vi.fn(() => Promise.resolve(() => {})),
      onNetworkHostResolved: vi.fn((handler: (host: unknown) => void) => {
        events.hostResolved = handler
        return Promise.resolve(() => {})
      }),
      onNetworkDiscoveryStateChanged: vi.fn(() => Promise.resolve(() => {})),
    },
  }
})
vi.mock('$lib/tauri-commands', () => ipc)
vi.mock('$lib/settings', () => ({ initializeSettings: vi.fn(() => Promise.resolve()) }))
vi.mock('$lib/settings/network-settings', () => ({
  getNetworkTimeoutMs: () => 15000,
  getShareCacheTtlMs: () => 30000,
}))

// Static import of the module under test (satisfies `custom/no-isolated-tests`); the
// `vi.hoisted` mocks above are hoisted over it, so they apply before it loads.
import * as store from './network-store.svelte'

/**
 * The store keeps its share lists at module level, by host id, so each test
 * brings hosts under ids of its own. The names are what the saved server is
 * matched by, so those stay.
 */
let testRun = 0
let nas: NetworkHost
let printer: NetworkHost

/** The person saved the NAS, and nothing else. */
const savedNas = {
  id: 'manual-naspolya',
  protocol: 'smb',
  displayName: 'Naspolya',
  nameSource: 'fallback',
  address: 'naspolya.local',
  places: [],
} as unknown as SavedServer

/** Lets the store's fire-and-forget chains (the saved-list read, the prefetch, the listing) run out. */
async function settle(): Promise<void> {
  for (let round = 0; round < 10; round++) await Promise.resolve()
}

const prefetchedHostIds = () => ipc.prefetchShares.mock.calls.map((call: unknown[]) => call[0])
const listedHostIds = () => ipc.listSharesOnHost.mock.calls.map((call: unknown[]) => call[0])

describe('share prefetch', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.listSavedServers.mockResolvedValue([savedNas])
    ipc.listNetworkHosts.mockResolvedValue([])
    ipc.resolveNetworkHost.mockResolvedValue(null)
    testRun += 1
    nas = { id: `bonjour-nas-${String(testRun)}`, name: 'Naspolya', hostname: 'naspolya.local', port: 445 }
    printer = { id: `bonjour-printer-${String(testRun)}`, name: 'Printer', hostname: 'printer.local', port: 445 }
  })

  afterEach(() => {
    store.cleanupNetworkDiscovery()
    vi.useRealTimers()
  })

  /** Releases every Servers view a test opened, so the next one starts with none on screen. */
  const releases: (() => void)[] = []
  function openServersView(): void {
    releases.push(store.holdDiscoveryForServersView())
  }
  afterEach(() => {
    for (const release of releases.splice(0)) release()
  })

  it('lists nobody’s shares at launch while no Servers view is open, not even a saved server’s', async () => {
    ipc.listNetworkHosts.mockResolvedValue([nas, printer])
    await store.initNetworkDiscovery()
    events.hostResolved?.(nas)
    await settle()

    expect(ipc.prefetchShares).not.toHaveBeenCalled()
    expect(ipc.listSharesOnHost).not.toHaveBeenCalled()
  })

  it('lists a saved server’s shares once the Servers view opens, and leaves a host nobody saved alone', async () => {
    ipc.listNetworkHosts.mockResolvedValue([nas, printer])
    await store.initNetworkDiscovery()
    await settle()

    openServersView()
    await settle()

    expect(prefetchedHostIds()).toEqual([nas.id])
    expect(listedHostIds()).toEqual([nas.id])
    expect(store.getShareState(printer.id)).toBeUndefined()
  })

  it('reads the saved list once for a view’s worth of hosts', async () => {
    const twin: NetworkHost = { id: `bonjour-tv-${String(testRun)}`, name: 'TV', hostname: 'tv.local', port: 445 }
    ipc.listNetworkHosts.mockResolvedValue([nas, printer, twin])
    await store.initNetworkDiscovery()
    openServersView()
    await settle()

    expect(ipc.listSavedServers).toHaveBeenCalledOnce()
  })

  it('holds the same line for a host that turns up, or resolves, while the view is open', async () => {
    await store.initNetworkDiscovery()
    openServersView()

    events.hostFound?.(printer)
    events.hostFound?.({ id: nas.id, name: nas.name, port: 445 })
    events.hostResolved?.(nas)
    await settle()

    expect(prefetchedHostIds()).toEqual([nas.id])
    expect(listedHostIds()).toEqual([nas.id])
  })

  it('lists nothing when the saved list can’t be read: not knowing is no licence to sign in', async () => {
    ipc.listSavedServers.mockRejectedValue(new Error('store unreadable'))
    ipc.listNetworkHosts.mockResolvedValue([nas, printer])
    await store.initNetworkDiscovery()
    openServersView()
    await settle()

    expect(ipc.prefetchShares).not.toHaveBeenCalled()
    expect(ipc.listSharesOnHost).not.toHaveBeenCalled()
  })

  it('on entering the Servers view, re-lists a saved host’s stale shares and only drops a found one’s', async () => {
    vi.useFakeTimers()
    ipc.listNetworkHosts.mockResolvedValue([nas, printer])
    await store.initNetworkDiscovery()
    openServersView()
    await settle()
    // The person opened the printer: that is the one way a found host gets listed.
    await store.fetchShares(printer)
    expect(store.getShareState(printer.id)?.status).toBe('loaded')
    ipc.listSharesOnHost.mockClear()

    vi.advanceTimersByTime(60_000)
    store.refreshAllStaleShares()
    await settle()

    expect(listedHostIds()).toEqual([nas.id])
    // Dropped, so opening it again lists afresh instead of showing the old list.
    expect(store.getShareState(printer.id)).toBeUndefined()
  })
})
