/**
 * Tests for network authentication functionality.
 * Covers login form logic, credential management, and auth scenarios.
 */

import { describe, it, expect, vi } from 'vitest'

// Mock the tauri commands
vi.mock('$lib/tauri-commands', () => {
    return {
        getUsernameHints: vi.fn().mockResolvedValue({}),
        getKnownShareByName: vi.fn().mockResolvedValue(null),
        saveSmbCredentials: vi.fn().mockResolvedValue(undefined),
        getSmbCredentials: vi.fn().mockRejectedValue({ type: 'not_found', message: 'Not found' }),
        hasSmbCredentials: vi.fn().mockResolvedValue(false),
        deleteSmbCredentials: vi.fn().mockResolvedValue(undefined),
        listSharesWithCredentials: vi.fn().mockResolvedValue({
            shares: [{ name: 'TestShare', isDisk: true }],
            authMode: 'guest_allowed',
            fromCache: false,
        }),
        isKeychainError: (error: unknown): boolean => {
            return (
                typeof error === 'object' &&
                error !== null &&
                'type' in error &&
                ['not_found', 'access_denied', 'other'].includes((error as { type: string }).type)
            )
        },
    }
})

// Mock the network store
vi.mock('$lib/network-store.svelte', () => {
    return {
        getShareState: vi.fn().mockReturnValue(null),
        fetchShares: vi.fn().mockResolvedValue({
            shares: [{ name: 'TestShare', isDisk: true }],
            authMode: 'guest_allowed',
            fromCache: false,
        }),
        clearShareState: vi.fn(),
    }
})

import type { AuthMode, AuthOptions, ConnectionMode, KnownNetworkShare, NetworkHost, KeychainError } from './types'

// =============================================================================
// Type tests - ensure types match backend
// =============================================================================

describe('Network authentication types', () => {
    describe('AuthMode', () => {
        it('should support all auth modes', () => {
            const modes: AuthMode[] = ['guest_allowed', 'creds_required', 'unknown']
            expect(modes).toHaveLength(3)
        })
    })

    describe('ConnectionMode', () => {
        it('should support guest and credentials modes', () => {
            const modes: ConnectionMode[] = ['guest', 'credentials']
            expect(modes).toHaveLength(2)
        })
    })

    describe('AuthOptions', () => {
        it('should support all auth option combinations', () => {
            const options: AuthOptions[] = ['guest_only', 'credentials_only', 'guest_or_credentials']
            expect(options).toHaveLength(3)
        })
    })

    describe('KnownNetworkShare', () => {
        it('should have all required fields', () => {
            const share: KnownNetworkShare = {
                serverName: 'TestServer',
                shareName: 'TestShare',
                protocol: 'smb',
                lastConnectedAt: '2026-01-06T12:00:00Z',
                lastConnectionMode: 'credentials',
                lastKnownAuthOptions: 'guest_or_credentials',
                username: 'testuser',
            }

            expect(share.serverName).toBe('TestServer')
            expect(share.shareName).toBe('TestShare')
            expect(share.protocol).toBe('smb')
            expect(share.username).toBe('testuser')
        })

        it('should allow null username for guest connections', () => {
            const share: KnownNetworkShare = {
                serverName: 'TestServer',
                shareName: 'TestShare',
                protocol: 'smb',
                lastConnectedAt: '2026-01-06T12:00:00Z',
                lastConnectionMode: 'guest',
                lastKnownAuthOptions: 'guest_only',
                username: null,
            }

            expect(share.username).toBeNull()
        })
    })

    describe('KeychainError', () => {
        it('should support all error types', () => {
            const notFound: KeychainError = { type: 'not_found', message: 'Credentials not found' }
            const accessDenied: KeychainError = { type: 'access_denied', message: 'User cancelled' }
            const other: KeychainError = { type: 'other', message: 'Unknown error' }

            expect(notFound.type).toBe('not_found')
            expect(accessDenied.type).toBe('access_denied')
            expect(other.type).toBe('other')
        })
    })
})

// =============================================================================
// Authentication flow tests
// =============================================================================

// Helper functions to avoid TypeScript narrowing false positives in tests
function isGuestAllowed(mode: AuthMode): boolean {
    return mode === 'guest_allowed'
}

function getDefaultConnectionMode(authMode: AuthMode): ConnectionMode {
    return authMode === 'guest_allowed' ? 'guest' : 'credentials'
}

function canSubmitCredentials(mode: ConnectionMode, username: string, password: string): boolean {
    return mode === 'guest' || (username.trim() !== '' && password !== '')
}

describe('Authentication flow logic', () => {
    describe('Auth mode detection', () => {
        it('should identify guest-allowed hosts', () => {
            const authMode: AuthMode = 'guest_allowed'
            expect(isGuestAllowed(authMode)).toBe(true)
        })

        it('should identify credentials-required hosts', () => {
            const authMode: AuthMode = 'creds_required'
            expect(isGuestAllowed(authMode)).toBe(false)
        })

        it('should handle unknown auth mode', () => {
            const authMode: AuthMode = 'unknown'
            expect(isGuestAllowed(authMode)).toBe(false)
        })
    })

    describe('Username pre-fill logic', () => {
        it('should derive username from username hints', async () => {
            const { getUsernameHints } = await import('$lib/tauri-commands')
            vi.mocked(getUsernameHints).mockResolvedValueOnce({
                testserver: 'david',
                otherserver: 'john',
            })

            const hints = await getUsernameHints()
            const serverKey = 'TestServer'.toLowerCase()
            const username = hints[serverKey] ?? ''

            expect(username).toBe('david')
        })

        it('should fallback to known share username', async () => {
            const { getKnownShareByName } = await import('$lib/tauri-commands')
            vi.mocked(getKnownShareByName).mockResolvedValueOnce({
                serverName: 'TestServer',
                shareName: 'Documents',
                protocol: 'smb',
                lastConnectedAt: '2026-01-06T12:00:00Z',
                lastConnectionMode: 'credentials',
                lastKnownAuthOptions: 'guest_or_credentials',
                username: 'storeduser',
            })

            const known = await getKnownShareByName('TestServer', 'Documents')
            expect(known?.username).toBe('storeduser')
        })
    })

    describe('Auth options change detection', () => {
        it('should detect when guest can now auth', () => {
            const knownShare: KnownNetworkShare = {
                serverName: 'TestServer',
                shareName: 'TestShare',
                protocol: 'smb',
                lastConnectedAt: '2026-01-06T12:00:00Z',
                lastConnectionMode: 'guest',
                lastKnownAuthOptions: 'guest_only',
                username: null,
            }
            const currentAuthMode: AuthMode = 'guest_allowed'

            // Was guest-only, now can use credentials (detected as guest_allowed)
            const authChanged = knownShare.lastKnownAuthOptions === 'guest_only' && isGuestAllowed(currentAuthMode)
            expect(authChanged).toBe(true)
        })

        it('should not flag change when options same', () => {
            const knownShare: KnownNetworkShare = {
                serverName: 'TestServer',
                shareName: 'TestShare',
                protocol: 'smb',
                lastConnectedAt: '2026-01-06T12:00:00Z',
                lastConnectionMode: 'credentials',
                lastKnownAuthOptions: 'credentials_only',
                username: 'testuser',
            }

            // Still credentials required, no change
            const wasCredsRequired =
                knownShare.lastKnownAuthOptions === 'credentials_only' ||
                knownShare.lastKnownAuthOptions === 'guest_or_credentials'

            expect(wasCredsRequired).toBe(true)
        })
    })

    describe('Connection mode determination', () => {
        it('should default to guest when allowed', () => {
            const authMode: AuthMode = 'guest_allowed'
            expect(getDefaultConnectionMode(authMode)).toBe('guest')
        })

        it('should default to credentials when required', () => {
            const authMode: AuthMode = 'creds_required'
            expect(getDefaultConnectionMode(authMode)).toBe('credentials')
        })
    })
})

// =============================================================================
// Credential validation tests
// =============================================================================

describe('Credential validation', () => {
    it('should require username and password for credentials mode', () => {
        const connectionMode: ConnectionMode = 'credentials'
        expect(canSubmitCredentials(connectionMode, 'testuser', 'testpass')).toBe(true)
    })

    it('should fail validation with empty username', () => {
        const connectionMode: ConnectionMode = 'credentials'
        expect(canSubmitCredentials(connectionMode, '', 'testpass')).toBe(false)
    })

    it('should fail validation with empty password', () => {
        const connectionMode: ConnectionMode = 'credentials'
        expect(canSubmitCredentials(connectionMode, 'testuser', '')).toBe(false)
    })

    it('should allow submission in guest mode without credentials', () => {
        const connectionMode: ConnectionMode = 'guest'
        expect(canSubmitCredentials(connectionMode, '', '')).toBe(true)
    })

    it('should trim whitespace from username', () => {
        const username = '  testuser  '
        expect(username.trim()).toBe('testuser')
    })
})

// =============================================================================
// NetworkHost type tests
// =============================================================================

describe('NetworkHost for authentication', () => {
    it('should have required fields for authentication', () => {
        const host: NetworkHost = {
            id: 'test-host',
            name: 'TestServer',
            hostname: 'testserver.local',
            ipAddress: '192.168.1.100',
            port: 445,
        }

        expect(host.name).toBe('TestServer')
        expect(host.hostname).toBe('testserver.local')
        expect(host.ipAddress).toBe('192.168.1.100')
        expect(host.port).toBe(445)
    })

    it('should work with optional hostname and IP', () => {
        const host: NetworkHost = {
            id: 'unresolved-host',
            name: 'UnresolvedServer',
            port: 445,
        }

        expect(host.hostname).toBeUndefined()
        expect(host.ipAddress).toBeUndefined()
    })
})
