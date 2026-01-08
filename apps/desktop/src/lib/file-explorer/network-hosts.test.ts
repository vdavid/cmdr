/**
 * Tests for network host discovery display in the NetworkBrowser component.
 * Tests the display and interaction logic for discovered network hosts.
 */

import { describe, it, expect, vi } from 'vitest'
import type { NetworkHost, DiscoveryState, VolumeInfo } from './types'

// Mock the tauri-commands module
vi.mock('$lib/tauri-commands', () => ({
    listVolumes: vi.fn(),
    findContainingVolume: vi.fn(),
    listNetworkHosts: vi.fn(),
    getNetworkDiscoveryState: vi.fn(),
    resolveNetworkHost: vi.fn(),
    listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Helper to create test data
function createMockHost(overrides: Partial<NetworkHost> = {}): NetworkHost {
    return {
        id: 'test-host',
        name: 'Test Host',
        hostname: undefined,
        ipAddress: undefined,
        port: 445,
        ...overrides,
    }
}

describe('Network host discovery types', () => {
    describe('NetworkHost interface', () => {
        it('should have required fields', () => {
            const host = createMockHost()
            expect(host.id).toBeDefined()
            expect(host.name).toBeDefined()
            expect(host.port).toBe(445)
        })

        it('should support optional hostname and ipAddress', () => {
            const hostWithoutResolution = createMockHost()
            expect(hostWithoutResolution.hostname).toBeUndefined()
            expect(hostWithoutResolution.ipAddress).toBeUndefined()

            const hostWithResolution = createMockHost({
                hostname: 'test.local',
                ipAddress: '192.168.1.100',
            })
            expect(hostWithResolution.hostname).toBe('test.local')
            expect(hostWithResolution.ipAddress).toBe('192.168.1.100')
        })
    })

    describe('DiscoveryState type', () => {
        it('should accept valid states', () => {
            const states: DiscoveryState[] = ['idle', 'searching', 'active']
            states.forEach((state) => {
                expect(['idle', 'searching', 'active']).toContain(state)
            })
        })
    })
})

describe('Volume selector network entry', () => {
    // Tests that the volume selector includes a single "Network" entry

    function createNetworkVolumeEntry(): VolumeInfo {
        return {
            id: 'network',
            name: 'Network',
            path: 'smb://',
            category: 'network',
            icon: undefined,
            isEjectable: false,
        }
    }

    it('should create network volume with correct ID', () => {
        const networkEntry = createNetworkVolumeEntry()
        expect(networkEntry.id).toBe('network')
    })

    it('should create network volume with Network name', () => {
        const networkEntry = createNetworkVolumeEntry()
        expect(networkEntry.name).toBe('Network')
    })

    it('should create network volume with smb:// path', () => {
        const networkEntry = createNetworkVolumeEntry()
        expect(networkEntry.path).toBe('smb://')
    })

    it('should set category to network', () => {
        const networkEntry = createNetworkVolumeEntry()
        expect(networkEntry.category).toBe('network')
    })

    it('should not be ejectable', () => {
        const networkEntry = createNetworkVolumeEntry()
        expect(networkEntry.isEjectable).toBe(false)
    })
})

describe('NetworkBrowser host display', () => {
    // Tests for how hosts are displayed in NetworkBrowser

    it('should format host with all fields', () => {
        const host = createMockHost({
            id: 'my-nas',
            name: 'My NAS',
            hostname: 'my-nas.local',
            ipAddress: '192.168.1.100',
        })

        expect(host.name).toBe('My NAS')
        expect(host.hostname).toBe('my-nas.local')
        expect(host.ipAddress).toBe('192.168.1.100')
    })

    it('should handle unresolved host', () => {
        const host = createMockHost({
            id: 'unresolved',
            name: 'Unresolved Host',
        })

        expect(host.name).toBe('Unresolved Host')
        expect(host.hostname).toBeUndefined()
        expect(host.ipAddress).toBeUndefined()
    })
})

describe('Network host event handling', () => {
    it('should add new host on network-host-found event', () => {
        let hosts: NetworkHost[] = []

        // Simulate event handler
        const handleHostFound = (host: NetworkHost) => {
            hosts = [...hosts.filter((h) => h.id !== host.id), host]
        }

        handleHostFound(createMockHost({ id: 'host1', name: 'Host 1' }))
        expect(hosts).toHaveLength(1)

        handleHostFound(createMockHost({ id: 'host2', name: 'Host 2' }))
        expect(hosts).toHaveLength(2)

        // Update existing host
        handleHostFound(createMockHost({ id: 'host1', name: 'Updated Host 1' }))
        expect(hosts).toHaveLength(2)
        expect(hosts.find((h) => h.id === 'host1')?.name).toBe('Updated Host 1')
    })

    it('should remove host on network-host-lost event', () => {
        let hosts: NetworkHost[] = [
            createMockHost({ id: 'host1', name: 'Host 1' }),
            createMockHost({ id: 'host2', name: 'Host 2' }),
        ]

        // Simulate event handler
        const handleHostLost = (hostId: string) => {
            hosts = hosts.filter((h) => h.id !== hostId)
        }

        handleHostLost('host1')
        expect(hosts).toHaveLength(1)
        expect(hosts[0]?.id).toBe('host2')
    })

    it('should update discovery state on state change event', () => {
        let state: DiscoveryState = 'idle'

        // Simulate event handler
        const handleStateChange = (newState: DiscoveryState) => {
            state = newState
        }

        handleStateChange('searching')
        expect(state).toBe('searching')

        handleStateChange('active')
        expect(state).toBe('active')

        handleStateChange('idle')
        expect(state).toBe('idle')
    })
})

describe('NetworkBrowser keyboard navigation', () => {
    it('should track selected index', () => {
        let selectedIndex = 0
        const hosts = [
            createMockHost({ id: 'host1', name: 'Host 1' }),
            createMockHost({ id: 'host2', name: 'Host 2' }),
            createMockHost({ id: 'host3', name: 'Host 3' }),
        ]

        // Simulate ArrowDown
        const handleArrowDown = () => {
            selectedIndex = Math.min(selectedIndex + 1, hosts.length - 1)
        }

        // Simulate ArrowUp
        const handleArrowUp = () => {
            selectedIndex = Math.max(selectedIndex - 1, 0)
        }

        handleArrowDown()
        expect(selectedIndex).toBe(1)

        handleArrowDown()
        expect(selectedIndex).toBe(2)

        handleArrowDown() // Should stay at last
        expect(selectedIndex).toBe(2)

        handleArrowUp()
        expect(selectedIndex).toBe(1)

        handleArrowUp()
        expect(selectedIndex).toBe(0)

        handleArrowUp() // Should stay at first
        expect(selectedIndex).toBe(0)
    })

    it('should handle Home/End navigation', () => {
        let selectedIndex = 1
        const hosts = [
            createMockHost({ id: 'host1', name: 'Host 1' }),
            createMockHost({ id: 'host2', name: 'Host 2' }),
            createMockHost({ id: 'host3', name: 'Host 3' }),
        ]

        // Simulate Home
        const handleHome = () => {
            selectedIndex = 0
        }

        // Simulate End
        const handleEnd = () => {
            selectedIndex = hosts.length - 1
        }

        handleEnd()
        expect(selectedIndex).toBe(2)

        handleHome()
        expect(selectedIndex).toBe(0)
    })
})

// ============================================================================
// Share listing tests (2.19)
// ============================================================================

import type {
    ShareInfo,
    ShareListResult,
    AuthMode,
    ShareListError,
    KnownNetworkShare,
    ConnectionMode,
    AuthOptions,
} from './types'

describe('Share listing types', () => {
    function createMockShare(overrides: Partial<ShareInfo> = {}): ShareInfo {
        return {
            name: 'Documents',
            isDisk: true,
            comment: undefined,
            ...overrides,
        }
    }

    function createMockShareListResult(overrides: Partial<ShareListResult> = {}): ShareListResult {
        return {
            shares: [],
            authMode: 'guest_allowed',
            fromCache: false,
            ...overrides,
        }
    }

    describe('ShareInfo interface', () => {
        it('should have required fields', () => {
            const share = createMockShare()
            expect(share.name).toBe('Documents')
            expect(share.isDisk).toBe(true)
        })

        it('should support optional comment', () => {
            const shareWithComment = createMockShare({ comment: 'My documents folder' })
            expect(shareWithComment.comment).toBe('My documents folder')

            const shareWithoutComment = createMockShare()
            expect(shareWithoutComment.comment).toBeUndefined()
        })
    })

    describe('AuthMode type', () => {
        it('should accept valid auth modes', () => {
            const modes: AuthMode[] = ['guest_allowed', 'creds_required', 'unknown']
            expect(modes).toHaveLength(3)
        })
    })

    describe('ShareListResult interface', () => {
        it('should contain shares array', () => {
            const result = createMockShareListResult({
                shares: [createMockShare({ name: 'Documents' }), createMockShare({ name: 'Media' })],
            })
            expect(result.shares).toHaveLength(2)
            expect(result.shares[0].name).toBe('Documents')
            expect(result.shares[1].name).toBe('Media')
        })

        it('should track auth mode', () => {
            const guestResult = createMockShareListResult({ authMode: 'guest_allowed' })
            expect(guestResult.authMode).toBe('guest_allowed')

            const credsResult = createMockShareListResult({ authMode: 'creds_required' })
            expect(credsResult.authMode).toBe('creds_required')
        })

        it('should track cache status', () => {
            const freshResult = createMockShareListResult({ fromCache: false })
            expect(freshResult.fromCache).toBe(false)

            const cachedResult = createMockShareListResult({ fromCache: true })
            expect(cachedResult.fromCache).toBe(true)
        })
    })

    describe('ShareListError type', () => {
        it('should represent different error types', () => {
            const errors: ShareListError[] = [
                { type: 'host_unreachable', message: 'Cannot connect to host' },
                { type: 'timeout', message: 'Connection timed out after 15s' },
                { type: 'auth_required', message: 'Authentication required' },
                { type: 'signing_required', message: 'SMB signing required' },
                { type: 'auth_failed', message: 'Invalid credentials' },
                { type: 'protocol_error', message: 'SMB protocol error' },
                { type: 'resolution_failed', message: 'DNS resolution failed' },
            ]
            expect(errors).toHaveLength(7)
        })
    })
})

describe('Share caching behavior', () => {
    it('should reuse cached results within TTL', () => {
        let cacheHits = 0
        let cacheMisses = 0

        // Simulate cache behavior
        const cache: Map<string, { result: ShareListResult; expiresAt: number }> = new Map()
        const ttl = 30000 // 30 seconds

        const getShares = (hostId: string, now: number): ShareListResult => {
            const cached = cache.get(hostId)
            if (cached && now < cached.expiresAt) {
                cacheHits++
                return { ...cached.result, fromCache: true }
            }
            cacheMisses++
            const result: ShareListResult = {
                shares: [{ name: 'Documents', isDisk: true }],
                authMode: 'guest_allowed',
                fromCache: false,
            }
            cache.set(hostId, { result, expiresAt: now + ttl })
            return result
        }

        const now = Date.now()

        // First call - cache miss
        const result1 = getShares('host1', now)
        expect(result1.fromCache).toBe(false)
        expect(cacheMisses).toBe(1)
        expect(cacheHits).toBe(0)

        // Second call within TTL - cache hit
        const result2 = getShares('host1', now + 10000)
        expect(result2.fromCache).toBe(true)
        expect(cacheHits).toBe(1)

        // Call after TTL expires - cache miss
        const result3 = getShares('host1', now + 35000)
        expect(result3.fromCache).toBe(false)
        expect(cacheMisses).toBe(2)
    })

    it('should cache auth mode with shares', () => {
        let cachedAuthMode: AuthMode | undefined

        // Simulate storing auth mode from successful share list
        const handleShareListSuccess = (result: ShareListResult) => {
            cachedAuthMode = result.authMode
        }

        handleShareListSuccess({
            shares: [],
            authMode: 'creds_required',
            fromCache: false,
        })

        expect(cachedAuthMode).toBe('creds_required')
    })
})

describe('Share error handling', () => {
    // Simulate error classification like in network-store
    const getErrorDisplayText = (error: ShareListError): string => {
        switch (error.type) {
            case 'host_unreachable':
                return 'Unable to connect'
            case 'timeout':
                return 'Connection timed out'
            case 'auth_required':
                return 'Sign-in required'
            case 'signing_required':
                return 'SMB signing is required by this server'
            case 'auth_failed':
                return 'Authentication failed'
            case 'protocol_error':
                return 'Protocol error'
            case 'resolution_failed':
                return 'Unable to resolve hostname'
            default:
                return 'Unknown error'
        }
    }

    it('should provide user-friendly error messages', () => {
        expect(getErrorDisplayText({ type: 'auth_required', message: '' })).toBe('Sign-in required')
        expect(getErrorDisplayText({ type: 'timeout', message: '' })).toBe('Connection timed out')
        expect(getErrorDisplayText({ type: 'host_unreachable', message: '' })).toBe('Unable to connect')
    })

    it('should distinguish auth errors', () => {
        const authRequired: ShareListError = { type: 'auth_required', message: 'Need credentials' }
        const authFailed: ShareListError = { type: 'auth_failed', message: 'Bad password' }

        expect(authRequired.type).toBe('auth_required')
        expect(authFailed.type).toBe('auth_failed')
        expect(authRequired.type).not.toBe(authFailed.type)
    })
})

// ============================================================================
// Known shares store tests (5.10)
// ============================================================================

describe('Known shares store types', () => {
    function createMockKnownShare(overrides: Partial<KnownNetworkShare> = {}): KnownNetworkShare {
        return {
            serverName: 'Alpha',
            shareName: 'Documents',
            protocol: 'smb',
            lastConnectedAt: '2026-01-06T12:00:00Z',
            lastConnectionMode: 'credentials',
            lastKnownAuthOptions: 'guest_or_credentials',
            username: 'david',
            ...overrides,
        }
    }

    describe('ConnectionMode type', () => {
        it('should accept valid connection modes', () => {
            const modes: ConnectionMode[] = ['guest', 'credentials']
            expect(modes).toHaveLength(2)
        })
    })

    describe('AuthOptions type', () => {
        it('should accept valid auth options', () => {
            const options: AuthOptions[] = ['guest_only', 'credentials_only', 'guest_or_credentials']
            expect(options).toHaveLength(3)
        })
    })

    describe('KnownNetworkShare interface', () => {
        it('should have all required fields', () => {
            const share = createMockKnownShare()
            expect(share.serverName).toBe('Alpha')
            expect(share.shareName).toBe('Documents')
            expect(share.protocol).toBe('smb')
            expect(share.lastConnectedAt).toBeDefined()
            expect(share.lastConnectionMode).toBe('credentials')
            expect(share.lastKnownAuthOptions).toBe('guest_or_credentials')
            expect(share.username).toBe('david')
        })

        it('should allow null username for guest connections', () => {
            const guestShare = createMockKnownShare({
                lastConnectionMode: 'guest',
                lastKnownAuthOptions: 'guest_only',
                username: null,
            })
            expect(guestShare.username).toBeNull()
            expect(guestShare.lastConnectionMode).toBe('guest')
        })
    })
})

describe('Known shares username pre-fill logic', () => {
    // Simulate the frontend logic for username pre-fill

    function findUsernameForServer(knownShares: KnownNetworkShare[], serverName: string): string | undefined {
        const serverLower = serverName.toLowerCase()
        const shareForServer = knownShares.find((s) => s.serverName.toLowerCase() === serverLower)
        return shareForServer?.username ?? undefined
    }

    it('should return username from known share', () => {
        const knownShares: KnownNetworkShare[] = [
            {
                serverName: 'MyNAS',
                shareName: 'Documents',
                protocol: 'smb',
                lastConnectedAt: '2026-01-06T12:00:00Z',
                lastConnectionMode: 'credentials',
                lastKnownAuthOptions: 'credentials_only',
                username: 'admin',
            },
        ]

        expect(findUsernameForServer(knownShares, 'MyNAS')).toBe('admin')
        expect(findUsernameForServer(knownShares, 'mynas')).toBe('admin') // Case-insensitive
    })

    it('should return undefined for unknown server', () => {
        const knownShares: KnownNetworkShare[] = []
        expect(findUsernameForServer(knownShares, 'UnknownServer')).toBeUndefined()
    })

    it('should return undefined for guest-only shares', () => {
        const knownShares: KnownNetworkShare[] = [
            {
                serverName: 'GuestNAS',
                shareName: 'Public',
                protocol: 'smb',
                lastConnectedAt: '2026-01-06T12:00:00Z',
                lastConnectionMode: 'guest',
                lastKnownAuthOptions: 'guest_only',
                username: null,
            },
        ]

        expect(findUsernameForServer(knownShares, 'GuestNAS')).toBeUndefined()
    })
})

describe('Auth options change detection', () => {
    // Simulate the frontend logic for detecting auth changes

    function detectAuthChange(
        previousAuthOptions: AuthOptions | undefined,
        currentAuthOptions: AuthOptions,
    ): { changed: boolean; message?: string } {
        if (!previousAuthOptions) {
            return { changed: false }
        }

        if (previousAuthOptions === currentAuthOptions) {
            return { changed: false }
        }

        // Guest-only changed to allow credentials
        if (previousAuthOptions === 'guest_only' && currentAuthOptions !== 'guest_only') {
            return {
                changed: true,
                message: 'You connected as guest before. You can now sign in for more access.',
            }
        }

        // Credentials changed to guest-only
        if (previousAuthOptions !== 'guest_only' && currentAuthOptions === 'guest_only') {
            return {
                changed: true,
                message: "This share now only allows guest access. Your previous credentials won't be used.",
            }
        }

        return { changed: true }
    }

    it('should detect no change when options are the same', () => {
        const result = detectAuthChange('guest_only', 'guest_only')
        expect(result.changed).toBe(false)
    })

    it('should detect change from guest-only to credentials available', () => {
        const result = detectAuthChange('guest_only', 'guest_or_credentials')
        expect(result.changed).toBe(true)
        expect(result.message).toContain('sign in for more access')
    })

    it('should detect change from credentials to guest-only', () => {
        const result = detectAuthChange('guest_or_credentials', 'guest_only')
        expect(result.changed).toBe(true)
        expect(result.message).toContain('only allows guest access')
    })

    it('should not flag change for first connection', () => {
        const result = detectAuthChange(undefined, 'credentials_only')
        expect(result.changed).toBe(false)
    })
})

describe('Known shares store operations', () => {
    // Simulate in-memory store operations

    it('should add new share', () => {
        const shares: KnownNetworkShare[] = []

        const addOrUpdateShare = (share: KnownNetworkShare) => {
            const key = `${share.serverName.toLowerCase()}/${share.shareName.toLowerCase()}`
            const index = shares.findIndex((s) => `${s.serverName.toLowerCase()}/${s.shareName.toLowerCase()}` === key)
            if (index >= 0) {
                shares[index] = share
            } else {
                shares.push(share)
            }
        }

        addOrUpdateShare({
            serverName: 'NewServer',
            shareName: 'NewShare',
            protocol: 'smb',
            lastConnectedAt: '2026-01-06T12:00:00Z',
            lastConnectionMode: 'guest',
            lastKnownAuthOptions: 'guest_only',
            username: null,
        })

        expect(shares).toHaveLength(1)
        expect(shares[0].serverName).toBe('NewServer')
    })

    it('should update existing share', () => {
        const shares: KnownNetworkShare[] = [
            {
                serverName: 'Server',
                shareName: 'Share',
                protocol: 'smb',
                lastConnectedAt: '2026-01-05T12:00:00Z',
                lastConnectionMode: 'guest',
                lastKnownAuthOptions: 'guest_only',
                username: null,
            },
        ]

        const addOrUpdateShare = (share: KnownNetworkShare) => {
            const key = `${share.serverName.toLowerCase()}/${share.shareName.toLowerCase()}`
            const index = shares.findIndex((s) => `${s.serverName.toLowerCase()}/${s.shareName.toLowerCase()}` === key)
            if (index >= 0) {
                shares[index] = share
            } else {
                shares.push(share)
            }
        }

        addOrUpdateShare({
            serverName: 'Server',
            shareName: 'Share',
            protocol: 'smb',
            lastConnectedAt: '2026-01-06T12:00:00Z',
            lastConnectionMode: 'credentials',
            lastKnownAuthOptions: 'guest_or_credentials',
            username: 'admin',
        })

        expect(shares).toHaveLength(1)
        expect(shares[0].lastConnectionMode).toBe('credentials')
        expect(shares[0].username).toBe('admin')
    })

    it('should lookup share case-insensitively', () => {
        const shares: KnownNetworkShare[] = [
            {
                serverName: 'MyNAS',
                shareName: 'Documents',
                protocol: 'smb',
                lastConnectedAt: '2026-01-06T12:00:00Z',
                lastConnectionMode: 'credentials',
                lastKnownAuthOptions: 'credentials_only',
                username: 'admin',
            },
        ]

        const findShare = (serverName: string, shareName: string) => {
            const key = `${serverName.toLowerCase()}/${shareName.toLowerCase()}`
            return shares.find((s) => `${s.serverName.toLowerCase()}/${s.shareName.toLowerCase()}` === key)
        }

        expect(findShare('MyNAS', 'Documents')).toBeDefined()
        expect(findShare('mynas', 'documents')).toBeDefined()
        expect(findShare('MYNAS', 'DOCUMENTS')).toBeDefined()
        expect(findShare('OtherNAS', 'Documents')).toBeUndefined()
    })
})
