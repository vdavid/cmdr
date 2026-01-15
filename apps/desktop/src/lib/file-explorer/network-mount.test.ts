/**
 * Tests for network mounting functionality.
 * Covers mount types, error handling, and mounting flow.
 */

import { describe, it, expect, vi } from 'vitest'
import type { MountError, MountResult, NetworkHost, ShareInfo } from './types'

// Mock the tauri commands
vi.mock('$lib/tauri-commands', () => {
    return {
        mountNetworkShare: vi.fn(),
        isMountError: (error: unknown): boolean => {
            return (
                typeof error === 'object' &&
                error !== null &&
                'type' in error &&
                typeof (error as { type: string }).type === 'string' &&
                [
                    'host_unreachable',
                    'share_not_found',
                    'auth_required',
                    'auth_failed',
                    'permission_denied',
                    'timeout',
                    'cancelled',
                    'protocol_error',
                    'mount_path_conflict',
                ].includes((error as { type: string }).type)
            )
        },
    }
})

// =============================================================================
// Type tests - ensure types match backend
// =============================================================================

describe('Mount types', () => {
    describe('MountResult', () => {
        it('should have required fields', () => {
            const result: MountResult = {
                mountPath: '/Volumes/Documents',
                alreadyMounted: false,
            }

            expect(result.mountPath).toBe('/Volumes/Documents')
            expect(result.alreadyMounted).toBe(false)
        })

        it('should indicate when already mounted', () => {
            const result: MountResult = {
                mountPath: '/Volumes/Documents',
                alreadyMounted: true,
            }

            expect(result.alreadyMounted).toBe(true)
        })
    })

    describe('MountError', () => {
        it('should support host unreachable error', () => {
            const error: MountError = {
                type: 'host_unreachable',
                message: 'Can\'t connect to "NAS"',
            }
            expect(error.type).toBe('host_unreachable')
            expect(error.message).toContain('NAS')
        })

        it('should support share not found error', () => {
            const error: MountError = {
                type: 'share_not_found',
                message: 'Share "Documents" not found on "NAS"',
            }
            expect(error.type).toBe('share_not_found')
        })

        it('should support auth required error', () => {
            const error: MountError = {
                type: 'auth_required',
                message: 'Authentication required',
            }
            expect(error.type).toBe('auth_required')
        })

        it('should support auth failed error', () => {
            const error: MountError = {
                type: 'auth_failed',
                message: 'Invalid username or password',
            }
            expect(error.type).toBe('auth_failed')
        })

        it('should support permission denied error', () => {
            const error: MountError = {
                type: 'permission_denied',
                message: 'Permission denied',
            }
            expect(error.type).toBe('permission_denied')
        })

        it('should support timeout error', () => {
            const error: MountError = {
                type: 'timeout',
                message: 'Connection to "NAS" timed out',
            }
            expect(error.type).toBe('timeout')
        })

        it('should support cancelled error', () => {
            const error: MountError = {
                type: 'cancelled',
                message: 'Mount operation was cancelled',
            }
            expect(error.type).toBe('cancelled')
        })

        it('should support protocol error', () => {
            const error: MountError = {
                type: 'protocol_error',
                message: 'Incompatible SMB protocol version',
            }
            expect(error.type).toBe('protocol_error')
        })

        it('should support mount path conflict error', () => {
            const error: MountError = {
                type: 'mount_path_conflict',
                message: 'Mount path already exists',
            }
            expect(error.type).toBe('mount_path_conflict')
        })
    })
})

// =============================================================================
// Mount flow logic tests
// =============================================================================

describe('Mount flow logic', () => {
    describe('Setting server address', () => {
        it('should prefer IP address over hostname', () => {
            const host: NetworkHost = {
                id: 'test-host',
                name: 'NAS',
                hostname: 'nas.local',
                ipAddress: '192.168.1.100',
                port: 445,
            }

            const server = host.ipAddress ?? host.hostname ?? host.name
            expect(server).toBe('192.168.1.100')
        })

        it('should fall back to hostname when IP not available', () => {
            const host: NetworkHost = {
                id: 'test-host',
                name: 'NAS',
                hostname: 'nas.local',
                port: 445,
            }

            const server = host.ipAddress ?? host.hostname ?? host.name
            expect(server).toBe('nas.local')
        })

        it('should fall back to name when neither IP nor hostname available', () => {
            const host: NetworkHost = {
                id: 'test-host',
                name: 'NAS',
                port: 445,
            }

            const server = host.ipAddress ?? host.hostname ?? host.name
            expect(server).toBe('NAS')
        })
    })

    describe('Share info for mounting', () => {
        it('should have share name for mounting', () => {
            const share: ShareInfo = {
                name: 'Documents',
                isDisk: true,
                comment: 'Shared documents',
            }

            expect(share.name).toBe('Documents')
            expect(share.isDisk).toBe(true)
        })

        it('should handle shares without comments', () => {
            const share: ShareInfo = {
                name: 'Media',
                isDisk: true,
            }

            expect(share.name).toBe('Media')
            expect(share.comment).toBeUndefined()
        })
    })

    describe('Credentials handling', () => {
        // Helper to extract username from credentials, handling null
        function getUsername(creds: { username: string; password: string } | null): string | null {
            return creds?.username ?? null
        }

        // Helper to extract password from credentials, handling null
        function getPassword(creds: { username: string; password: string } | null): string | null {
            return creds?.password ?? null
        }

        it('should pass credentials when available', () => {
            const credentials: { username: string; password: string } | null = {
                username: 'testuser',
                password: 'testpass',
            }

            expect(getUsername(credentials)).toBe('testuser')
            expect(getPassword(credentials)).toBe('testpass')
        })

        it('should pass null when no credentials (guest mode)', () => {
            const credentials: { username: string; password: string } | null = null

            expect(getUsername(credentials)).toBeNull()
            expect(getPassword(credentials)).toBeNull()
        })
    })
})

// =============================================================================
// Error type guard tests
// =============================================================================

describe('isMountError type guard', () => {
    it('should identify valid mount errors', async () => {
        const { isMountError } = await import('$lib/tauri-commands')

        const errors: MountError[] = [
            { type: 'host_unreachable', message: 'Host unreachable' },
            { type: 'share_not_found', message: 'Share not found' },
            { type: 'auth_required', message: 'Auth required' },
            { type: 'auth_failed', message: 'Auth failed' },
            { type: 'timeout', message: 'Timeout' },
            { type: 'cancelled', message: 'Cancelled' },
            { type: 'protocol_error', message: 'Protocol error' },
        ]

        for (const error of errors) {
            expect(isMountError(error)).toBe(true)
        }
    })

    it('should reject non-mount errors', async () => {
        const { isMountError } = await import('$lib/tauri-commands')

        expect(isMountError(null)).toBe(false)
        expect(isMountError(undefined)).toBe(false)
        expect(isMountError('error')).toBe(false)
        expect(isMountError(123)).toBe(false)
        expect(isMountError({ type: 'unknown_error', message: 'test' })).toBe(false)
    })
})

// =============================================================================
// Volume ID generation tests
// =============================================================================

describe('Volume ID generation for mounted shares', () => {
    it('should generate SMB volume ID', () => {
        const server = '192.168.1.100'
        const shareName = 'Documents'
        const volumeId = `smb://${server}/${shareName}`

        expect(volumeId).toBe('smb://192.168.1.100/Documents')
    })

    it('should handle hostname in volume ID', () => {
        const server = 'nas.local'
        const shareName = 'Media'
        const volumeId = `smb://${server}/${shareName}`

        expect(volumeId).toBe('smb://nas.local/Media')
    })

    it('should handle special characters in share name', () => {
        const server = '192.168.1.100'
        const shareName = 'My Documents'
        const volumeId = `smb://${server}/${shareName}`

        expect(volumeId).toBe('smb://192.168.1.100/My Documents')
    })
})

// =============================================================================
// Mock mount command tests
// =============================================================================

describe('mountNetworkShare command', () => {
    it('should return mount result on success', async () => {
        const { mountNetworkShare } = await import('$lib/tauri-commands')
        vi.mocked(mountNetworkShare).mockResolvedValueOnce({
            mountPath: '/Volumes/Documents',
            alreadyMounted: false,
        })

        const result = await mountNetworkShare('192.168.1.100', 'Documents', null, null)

        expect(result.mountPath).toBe('/Volumes/Documents')
        expect(result.alreadyMounted).toBe(false)
    })

    it('should indicate when share was already mounted', async () => {
        const { mountNetworkShare } = await import('$lib/tauri-commands')
        vi.mocked(mountNetworkShare).mockResolvedValueOnce({
            mountPath: '/Volumes/Documents',
            alreadyMounted: true,
        })

        const result = await mountNetworkShare('192.168.1.100', 'Documents', null, null)

        expect(result.alreadyMounted).toBe(true)
    })

    it('should throw mount error on failure', async () => {
        const { mountNetworkShare } = await import('$lib/tauri-commands')
        vi.mocked(mountNetworkShare).mockRejectedValueOnce({
            type: 'host_unreachable',
            message: 'Can\'t connect to "192.168.1.100"',
        })

        await expect(mountNetworkShare('192.168.1.100', 'Documents', null, null)).rejects.toEqual({
            type: 'host_unreachable',
            message: 'Can\'t connect to "192.168.1.100"',
        })
    })

    it('should pass credentials when provided', async () => {
        const { mountNetworkShare } = await import('$lib/tauri-commands')
        vi.mocked(mountNetworkShare).mockResolvedValueOnce({
            mountPath: '/Volumes/Documents',
            alreadyMounted: false,
        })

        await mountNetworkShare('192.168.1.100', 'Documents', 'testuser', 'testpass')

        expect(mountNetworkShare).toHaveBeenCalledWith('192.168.1.100', 'Documents', 'testuser', 'testpass')
    })
})
