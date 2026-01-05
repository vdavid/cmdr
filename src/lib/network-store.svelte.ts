/**
 * Network discovery store - manages network host discovery at app level.
 * This ensures discovery is active from app startup, not just when viewing the Network volume.
 */

import { SvelteSet } from 'svelte/reactivity'
import { listNetworkHosts, getNetworkDiscoveryState, resolveNetworkHost, listen } from '$lib/tauri-commands'
import type { UnlistenFn } from '$lib/tauri-commands'
import type { NetworkHost, DiscoveryState } from './file-explorer/types'

// Singleton state for network discovery
let hosts = $state<NetworkHost[]>([])
let discoveryState = $state<DiscoveryState>('idle')
const resolvingHosts = new SvelteSet<string>()

// Event listeners
let unlistenHostFound: UnlistenFn | undefined
let unlistenHostLost: UnlistenFn | undefined
let unlistenStateChanged: UnlistenFn | undefined
let initialized = false

/**
 * Start resolution for a host (fire-and-forget, non-blocking).
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
    for (const host of hosts) {
        startResolution(host)
    }

    // Subscribe to events
    unlistenHostFound = await listen<NetworkHost>('network-host-found', (event) => {
        const host = event.payload
        hosts = [...hosts.filter((h) => h.id !== host.id), host]
        // Start resolving the new host immediately
        startResolution(host)
    })

    unlistenHostLost = await listen<{ id: string }>('network-host-lost', (event) => {
        const { id } = event.payload
        hosts = hosts.filter((h) => h.id !== id)
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
    unlistenStateChanged?.()
    initialized = false
}

/**
 * Get reactive network hosts array.
 */
export function getNetworkHosts(): NetworkHost[] {
    return hosts
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
