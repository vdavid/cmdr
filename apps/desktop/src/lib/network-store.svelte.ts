/**
 * Network discovery store - manages network host discovery at app level.
 * This ensures discovery is active from app startup, not just when viewing the Network volume.
 */

import { SvelteSet, SvelteMap } from 'svelte/reactivity'
import {
    listNetworkHosts,
    getNetworkDiscoveryState,
    resolveNetworkHost,
    listen,
    listSharesOnHost,
    prefetchShares as prefetchSharesCmd,
    getSmbCredentials,
} from '$lib/tauri-commands'
import { getNetworkTimeoutMs, getShareCacheTtlMs } from '$lib/settings/network-settings'
import type { UnlistenFn } from '$lib/tauri-commands'
import type { NetworkHost, DiscoveryState, ShareListResult, ShareListError } from './file-explorer/types'

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
const prefetchingHosts = new SvelteSet<string>()

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

/**
 * Start prefetching shares for a host (fire-and-forget).
 * Called automatically after host resolution.
 */
function startPrefetchShares(host: NetworkHost) {
    // Skip if no hostname or already have data
    if (!host.hostname || shareStates.has(host.id)) {
        return
    }

    // Skip if already prefetching
    if (prefetchingHosts.has(host.id)) {
        return
    }

    prefetchingHosts.add(host.id)

    void prefetchSharesCmd(
        host.id,
        host.hostname,
        host.ipAddress,
        host.port,
        getNetworkTimeoutMs(),
        getShareCacheTtlMs(),
    )
        .then(() => {
            // Prefetch succeeded - backend has cached it
            if (!shareStates.has(host.id)) {
                // Trigger a proper fetch to get the cached result and update UI
                void fetchSharesSilent(host)
            }
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
    } catch (error) {
        const shareError = error as ShareListError
        shareStates.set(host.id, { status: 'error', error: shareError, fetchedAt: Date.now() })
    }
}

/**
 * Initialize network discovery - call once at app startup.
 * Subscribes to network events and loads initial hosts.
 */
export async function initNetworkDiscovery(): Promise<void> {
    if (initialized) return
    initialized = true

    // Load initial data
    hosts = await listNetworkHosts()
    discoveryState = await getNetworkDiscoveryState()

    // Start resolving all loaded hosts immediately (non-blocking)
    // Also prefetch shares for already-resolved hosts
    for (const host of hosts) {
        if (host.hostname) {
            // Already resolved - prefetch shares directly
            startPrefetchShares(host)
        } else {
            // Needs resolution first (will prefetch after)
            startResolution(host)
        }
    }

    // Subscribe to events
    unlistenHostFound = await listen<NetworkHost>('network-host-found', (event) => {
        const host = event.payload
        hosts = [...hosts.filter((h) => h.id !== host.id), host]
        // Start resolving the new host immediately (will prefetch after resolution)
        // Or prefetch directly if already resolved
        if (host.hostname) {
            startPrefetchShares(host)
        } else {
            startResolution(host)
        }
    })

    unlistenHostLost = await listen<{ id: string }>('network-host-lost', (event) => {
        const { id } = event.payload
        hosts = hosts.filter((h) => h.id !== id)
        // Clean up share state for lost host
        shareStates.delete(id)
    })

    // Listen for host resolution from mDNS (Bonjour NSNetService.resolve())
    unlistenHostResolved = await listen<NetworkHost>('network-host-resolved', (event) => {
        const resolved = event.payload
        // Update the host with resolved info (hostname and IP from mDNS)
        hosts = hosts.map((h) => (h.id === resolved.id ? { ...h, ...resolved } : h))

        // If we now have hostname and/or IP, prefetch shares
        const updatedHost = hosts.find((h) => h.id === resolved.id)
        if (updatedHost && (updatedHost.hostname || updatedHost.ipAddress)) {
            startPrefetchShares(updatedHost)
        }
    })

    unlistenStateChanged = await listen<{ state: DiscoveryState }>('network-discovery-state-changed', (event) => {
        discoveryState = event.payload.state
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
        throw new Error('Host hostname not resolved')
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
        return result
    } catch (error) {
        const shareError = error as ShareListError
        shareStates.set(host.id, { status: 'error', error: shareError, fetchedAt: Date.now() })
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
 * Refresh all stale shares (call when entering network view).
 */
export function refreshAllStaleShares(): void {
    for (const host of hosts) {
        refreshSharesIfStale(host)
    }
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
