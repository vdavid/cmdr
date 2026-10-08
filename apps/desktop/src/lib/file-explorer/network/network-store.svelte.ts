/**
 * Network discovery store: the hosts the backend knows, kept in sync at app level.
 *
 * The backend browses mDNS only while something needs it; this store's part is to say
 * when a Servers view is on screen (`holdDiscoveryForServersView`). Between browses the
 * backend keeps the hosts it found, so a view opens on known servers straight away.
 */

import { SvelteSet, SvelteMap } from 'svelte/reactivity'
import {
  listNetworkHosts,
  getNetworkDiscoveryState,
  resolveNetworkHost,
  onNetworkHostFound,
  onNetworkHostLost,
  onNetworkHostResolved,
  onNetworkDiscoveryStateChanged,
  listSharesOnHost,
  prefetchShares as prefetchSharesCmd,
  getSmbCredentials,
  hasCachedSmbCredentials,
  deleteSmbCredentials,
  setServersViewShown,
  listSavedServers,
  type SavedServer,
} from '$lib/tauri-commands'
import { getNetworkTimeoutMs, getShareCacheTtlMs } from '$lib/settings/network-settings'
import { initializeSettings } from '$lib/settings'
import type { UnlistenFn } from '$lib/tauri-commands'
import type { NetworkHost, DiscoveryState, ShareListResult, ShareListError } from '../types'
import { ShareListFailure, shareListErrorOf } from './share-list-error'
import type { SignedInAs } from './signed-in-as'
import { savedSmbHostIds } from './servers-hub-rows'

// Singleton state for network discovery
let hosts = $state<NetworkHost[]>([])
let discoveryState = $state<DiscoveryState>('idle')
const resolvingHosts = new SvelteSet<string>()

// Share listing state - includes fetchedAt for staleness tracking
type ShareState =
  | { status: 'loading' }
  | { status: 'loaded'; result: ShareListResult; fetchedAt: number }
  | { status: 'error'; error: ShareListError; fetchedAt: number }
const shareStates = new SvelteMap<string, ShareState>()
/**
 * The account each host's share list last signed in as, by host id. ❗ Off the
 * listings only: a guest listing says guest, a sign-in says its account
 * (`setListedAccount`), and an account listing answered from the backend's cache
 * keeps what the sign-in recorded, since it IS that listing.
 */
const listedAccounts = new SvelteMap<string, SignedInAs>()
/** Hosts a guest listing worked on this session: where "Use guest" is a real way back. */
const guestListingHosts = new SvelteSet<string>()
const prefetchingHosts = new SvelteSet<string>()
/** How many Servers views are on screen (two panes can each show one). */
let serversViewsShown = 0

// Credential status tracking - 'unknown' | 'has_creds' | 'no_creds' | 'failed'
type CredentialStatus = 'unknown' | 'has_creds' | 'no_creds' | 'failed'
const credentialStatuses = new SvelteMap<string, CredentialStatus>()

// Event listeners
let unlistenHostFound: UnlistenFn | undefined
let unlistenHostLost: UnlistenFn | undefined
let unlistenHostResolved: UnlistenFn | undefined
let unlistenStateChanged: UnlistenFn | undefined
let initialized = false

/**
 * Start resolution for a host (fire-and-forget, non-blocking).
 * After resolution completes, automatically prefetches shares.
 */
function startResolution(host: NetworkHost) {
  // Skip if already resolved or already resolving
  if (host.hostname || resolvingHosts.has(host.id)) {
    return
  }

  // Mark as resolving
  resolvingHosts.add(host.id)

  // Fire and forget - don't await, don't block UI
  resolveNetworkHost(host.id)
    .then((resolved) => {
      if (resolved) {
        hosts = hosts.map((h) => (h.id === host.id ? resolved : h))
        // After resolution, prefetch shares automatically
        startPrefetchShares(resolved)
      }
    })
    .catch(() => {
      // Resolution failed, just leave as unresolved
    })
    .finally(() => {
      resolvingHosts.delete(host.id)
    })
}

/** The saved-list read in flight, shared by every host that asks while it's out. */
let savedServersRead: Promise<SavedServer[]> | undefined

/**
 * The servers the person saved, read fresh: a host becomes theirs the moment they
 * add it or mount one of its shares, and nothing tells this store when.
 *
 * One read serves every caller that asks while it's out, so a launch's worth of
 * resolved hosts costs one. ❗ A read that breaks answers "none saved": not
 * knowing whose a host is must never be what signs Cmdr in to it.
 */
function readSavedServers(): Promise<SavedServer[]> {
  savedServersRead ??= listSavedServers()
    // `Array.isArray` because this is an IPC boundary, as in the hub's own read.
    .then((answer: unknown) => (Array.isArray(answer) ? (answer as SavedServer[]) : []))
    .catch(() => [])
    .finally(() => {
      savedServersRead = undefined
    })
  return savedServersRead
}

/**
 * Lists a host's shares ahead of time (fire-and-forget), so its share list is
 * there the moment the person opens it. Called when a Servers view opens, and
 * when a host resolves while one is open.
 *
 * ❗ **Only while a Servers view is on screen, and only for a server the person
 * SAVED** (`savedSmbHostIds`, the hub's own match). Listing means connecting to
 * the host and signing in, as a guest where it lets one in: every launch used to
 * do it to every SMB machine on the network (#324). With no Servers view open,
 * nobody is about to pick a share. A found host is listed when the person opens
 * it (`fetchShares`), and at no other time: see `refreshAllStaleShares` and the
 * hub's refresh, which hold the same line.
 */
function startPrefetchShares(host: NetworkHost) {
  const { hostname } = host
  // Skip if no Servers view is up, no hostname, or we already have data
  if (serversViewsShown === 0 || !hostname || shareStates.has(host.id)) {
    return
  }

  // Skip if already prefetching
  if (prefetchingHosts.has(host.id)) {
    return
  }

  prefetchingHosts.add(host.id)

  void readSavedServers()
    .then((saved) => {
      if (!savedSmbHostIds(saved, [host]).has(host.id)) return
      return prefetchSharesCmd(
        host.id,
        hostname,
        host.ipAddress,
        host.port,
        getNetworkTimeoutMs(),
        getShareCacheTtlMs(),
      ).then(() => {
        // Prefetch succeeded - backend has cached it
        if (!shareStates.has(host.id)) {
          // Trigger a proper fetch to get the cached result and update UI
          void fetchSharesSilent(host)
        }
      })
    })
    .catch(() => {
      // Silently ignore prefetch errors
    })
    .finally(() => {
      prefetchingHosts.delete(host.id)
    })
}

/**
 * Fetch shares silently (for background refresh after prefetch).
 */
async function fetchSharesSilent(host: NetworkHost): Promise<void> {
  if (!host.hostname) return

  try {
    const result = await listSharesOnHost(
      host.id,
      host.hostname,
      host.ipAddress,
      host.port,
      getNetworkTimeoutMs(),
      getShareCacheTtlMs(),
    )
    shareStates.set(host.id, { status: 'loaded', result, fetchedAt: Date.now() })
    noteListing(host.id, result)
  } catch (error) {
    shareStates.set(host.id, { status: 'error', error: shareListErrorOf(error), fetchedAt: Date.now() })
    listedAccounts.delete(host.id)
  }
}

/**
 * Keeps the backend's mDNS browse running while a Servers view is on screen, so hosts
 * arrive and leave live there, and lists the saved servers' shares the first time one
 * appears (`startPrefetchShares`). Call when the view appears; call the returned release
 * when it goes. The backend hears only the first appearance and the last departure.
 */
export function holdDiscoveryForServersView(): () => void {
  serversViewsShown += 1
  if (serversViewsShown === 1) {
    void setServersViewShown(true)
    for (const host of hosts) startPrefetchShares(host)
  }
  let released = false
  return () => {
    if (released) return
    released = true
    serversViewsShown -= 1
    if (serversViewsShown === 0) void setServersViewShown(false)
  }
}

/**
 * Initialize network discovery - call once at app startup.
 * Subscribes to network events and loads initial hosts.
 */
export async function initNetworkDiscovery(): Promise<void> {
  if (initialized) return
  initialized = true

  // A reloaded page starts from zero views, while the backend may still hold the browse
  // for the page before it: send this page's truth.
  void setServersViewShown(serversViewsShown > 0)

  // Ensure settings are loaded before reading network timeout/cache values
  await initializeSettings()

  // Load initial data
  hosts = await listNetworkHosts()
  discoveryState = await getNetworkDiscoveryState()

  // Start resolving all loaded hosts immediately (non-blocking). Resolved ones go
  // to the prefetch, which lists nothing unless a Servers view is already open.
  for (const host of hosts) {
    if (host.hostname) {
      startPrefetchShares(host)
    } else {
      // Needs resolution first (will prefetch after)
      startResolution(host)
    }
  }

  // Subscribe to events
  unlistenHostFound = await onNetworkHostFound((host) => {
    hosts = [...hosts.filter((h) => h.id !== host.id), host]
    // Start resolving the new host immediately (will prefetch after resolution)
    // Or prefetch directly if already resolved
    if (host.hostname) {
      startPrefetchShares(host)
    } else {
      startResolution(host)
    }
  })

  unlistenHostLost = await onNetworkHostLost((id) => {
    hosts = hosts.filter((h) => h.id !== id)
    // Clean up share state for lost host
    shareStates.delete(id)
    listedAccounts.delete(id)
  })

  // Listen for host resolution from mDNS (Bonjour NSNetService.resolve())
  unlistenHostResolved = await onNetworkHostResolved((resolved) => {
    // Update the host with resolved info (hostname and IP from mDNS)
    hosts = hosts.map((h) => (h.id === resolved.id ? { ...h, ...resolved } : h))

    // If we now have hostname and/or IP, prefetch shares
    const updatedHost = hosts.find((h) => h.id === resolved.id)
    if (updatedHost && (updatedHost.hostname || updatedHost.ipAddress)) {
      startPrefetchShares(updatedHost)
    }
  })

  unlistenStateChanged = await onNetworkDiscoveryStateChanged((state) => {
    discoveryState = state
  })
}

/**
 * Cleanup network discovery - call on app shutdown.
 */
export function cleanupNetworkDiscovery(): void {
  unlistenHostFound?.()
  unlistenHostLost?.()
  unlistenHostResolved?.()
  unlistenStateChanged?.()
  initialized = false
}

/**
 * Get reactive network hosts array, sorted alphabetically by name.
 */
export function getNetworkHosts(): NetworkHost[] {
  return [...hosts].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
}

/**
 * Get reactive discovery state.
 */
export function getDiscoveryState(): DiscoveryState {
  return discoveryState
}

/**
 * Check if a host is currently being resolved.
 */
export function isHostResolving(hostId: string): boolean {
  return resolvingHosts.has(hostId)
}

// ============================================================================
// Share listing functions
// ============================================================================

/**
 * Get share state for a host.
 */
export function getShareState(hostId: string): ShareState | undefined {
  return shareStates.get(hostId)
}

/**
 * Get share count for a host (for display in network browser).
 * Returns undefined if not yet loaded, or the count.
 */
export function getShareCount(hostId: string): number | undefined {
  const state = shareStates.get(hostId)
  if (state?.status === 'loaded') {
    return state.result.shares.length
  }
  return undefined
}

/**
 * Check if share listing is in progress for a host.
 */
export function isListingShares(hostId: string): boolean {
  return shareStates.get(hostId)?.status === 'loading'
}

/**
 * Check if share data is stale (older than the configured cache TTL).
 */
export function isShareDataStale(hostId: string): boolean {
  const state = shareStates.get(hostId)
  if (!state || state.status === 'loading') return false
  return Date.now() - state.fetchedAt > getShareCacheTtlMs()
}

/**
 * Fetch shares for a host. Updates the share state reactively.
 * Returns the result or throws an error.
 */
export async function fetchShares(host: NetworkHost): Promise<ShareListResult> {
  if (!host.hostname) {
    throw new ShareListFailure({ type: 'resolution_failed', message: 'the host has no hostname yet' })
  }

  // Mark as loading
  shareStates.set(host.id, { status: 'loading' })

  try {
    const result = await listSharesOnHost(
      host.id,
      host.hostname,
      host.ipAddress,
      host.port,
      getNetworkTimeoutMs(),
      getShareCacheTtlMs(),
    )
    shareStates.set(host.id, { status: 'loaded', result, fetchedAt: Date.now() })
    noteListing(host.id, result)
    return result
  } catch (error) {
    shareStates.set(host.id, { status: 'error', error: shareListErrorOf(error), fetchedAt: Date.now() })
    listedAccounts.delete(host.id)
    throw error
  }
}

/**
 * Clear share state for a host (for example, to force refresh).
 */
export function clearShareState(hostId: string): void {
  shareStates.delete(hostId)
}

/**
 * Drops the share lists of every host that is the same machine as `host`: the
 * manual host an add just injected, plus a discovered twin at the same IP or
 * hostname.
 *
 * ❗ After an add that typed an account, a guest listing fetched earlier for the
 * discovered twin would otherwise still open, which is the listing the person
 * just said they don't want. The backend skips guest from then on; this is the
 * frontend's copy of the same answer.
 */
export function forgetShareListsOfMachine(host: NetworkHost): void {
  for (const other of hosts) {
    const sameIp = host.ipAddress !== undefined && other.ipAddress === host.ipAddress
    const sameHostname = host.hostname !== undefined && other.hostname === host.hostname
    if (other.id === host.id || sameIp || sameHostname) {
      shareStates.delete(other.id)
      listedAccounts.delete(other.id)
    }
  }
  shareStates.delete(host.id)
  listedAccounts.delete(host.id)
}

/**
 * A plain listing's account: a guest one says guest, and an account one keeps what its
 * sign-in recorded. ❗ A guest listing never takes a KNOWN account back to guest: the
 * person chose it, and a background listing ("Sign in as testuser" read "as guest" three
 * seconds later, from the prefetch on the way back) is no one choosing anything.
 * "Use guest" goes through `setListedAccount`.
 */
function noteListing(hostId: string, result: ShareListResult): void {
  if (result.authMode !== 'guest_allowed') return
  guestListingHosts.add(hostId)
  if (listedAccounts.get(hostId)?.kind !== 'user') listedAccounts.set(hostId, { kind: 'guest' })
}

/** Whether a guest listing worked on `hostId` this session, so "Use guest" can offer a way back. */
export function guestListingWorked(hostId: string): boolean {
  return guestListingHosts.has(hostId)
}

/** The account `hostId`'s share list last signed in as, or `undefined` when no listing said. */
export function getListedAccount(hostId: string): SignedInAs | undefined {
  return listedAccounts.get(hostId)
}

/** Records the account a listing with explicit credentials (or the guest choice) signed in as. */
export function setListedAccount(hostId: string, account: SignedInAs): void {
  listedAccounts.set(hostId, account)
  if (account.kind === 'guest') guestListingHosts.add(hostId)
}

/**
 * Set share state for a host directly.
 * Use this when you have the result from a successful connection.
 */
export function setShareState(hostId: string, result: ShareListResult): void {
  shareStates.set(hostId, { status: 'loaded', result, fetchedAt: Date.now() })
}

/**
 * Refresh shares if data is stale.
 * Returns true if refresh was triggered.
 */
export function refreshSharesIfStale(host: NetworkHost): boolean {
  if (!isShareDataStale(host.id)) return false
  if (isListingShares(host.id)) return false // Already loading

  // Trigger background refresh
  void fetchSharesSilent(host)
  return true
}

/**
 * Brings the share lists up to date (call when entering the Servers view): a saved
 * host's stale list is re-read in the background, and a found host's is dropped.
 *
 * ❗ Dropped, ❌ not re-read: a host nobody saved is listed only when the person
 * opens it (see `startPrefetchShares`), and with no list cached, opening it reads
 * a fresh one instead of showing the old.
 */
export function refreshAllStaleShares(): void {
  const stale = hosts.filter((host) => isShareDataStale(host.id))
  if (stale.length === 0) return
  void readSavedServers().then((saved) => {
    const savedHosts = savedSmbHostIds(saved, stale)
    for (const host of stale) {
      if (savedHosts.has(host.id)) refreshSharesIfStale(host)
      else if (isShareDataStale(host.id)) shareStates.delete(host.id)
    }
  })
}

// ============================================================================
// Credential status functions
// ============================================================================

/**
 * Get credential status for a host (by server name).
 */
export function getCredentialStatus(serverName: string): CredentialStatus {
  const key = serverName.toLowerCase()
  return credentialStatuses.get(key) ?? 'unknown'
}

/**
 * Set credential status for a host.
 * Call this when credentials succeed or fail.
 */
export function setCredentialStatus(serverName: string, status: CredentialStatus): void {
  const key = serverName.toLowerCase()
  credentialStatuses.set(key, status)
}

/**
 * Check if credentials exist for a host (async, updates status).
 * Call this on mount to populate credential status.
 */
export async function checkCredentialsForHost(serverName: string): Promise<void> {
  const key = serverName.toLowerCase()

  // Don't re-check if already known
  if (credentialStatuses.has(key)) return

  try {
    await getSmbCredentials(serverName, null)
    credentialStatuses.set(key, 'has_creds')
  } catch {
    credentialStatuses.set(key, 'no_creds')
  }
}

/**
 * Marks a host `has_creds` when this session already READ its server-level password (a
 * listing's own lookup, a mount), from the backend's in-memory cache.
 *
 * ❗ Never a Keychain access: each can raise a system prompt, and the question (should
 * the share list offer "Forget saved password"?) is not worth one. So an unread
 * password leaves the status as it was, and the button waits for something to read it.
 */
export async function noteCachedCredentials(serverName: string): Promise<void> {
  if (await hasCachedSmbCredentials(serverName)) credentialStatuses.set(serverName.toLowerCase(), 'has_creds')
}

/**
 * Delete stored credentials for a host and update status.
 * Removes server-level credentials from the Keychain/credential store.
 */
export async function forgetCredentials(serverName: string): Promise<void> {
  await deleteSmbCredentials(serverName, null)
  credentialStatuses.set(serverName.toLowerCase(), 'no_creds')
}
