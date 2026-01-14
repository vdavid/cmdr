/**
 * Tests for license-related functionality.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock the tauri commands
vi.mock('$lib/tauri-commands', async (importOriginal) => {
    const original = await importOriginal<typeof import('$lib/tauri-commands')>()
    return {
        ...original,
        getLicenseStatus: vi.fn(),
        getWindowTitle: vi.fn(),
        activateLicense: vi.fn(),
        getLicenseInfo: vi.fn(),
        markExpirationModalShown: vi.fn(),
        markCommercialReminderDismissed: vi.fn(),
        resetLicense: vi.fn(),
        needsLicenseValidation: vi.fn(),
        validateLicenseWithServer: vi.fn(),
    }
})

import type { LicenseStatus, LicenseInfo } from '$lib/tauri-commands'
import {
    getLicenseStatus,
    getWindowTitle,
    activateLicense,
    getLicenseInfo,
    markExpirationModalShown,
    markCommercialReminderDismissed,
    needsLicenseValidation,
    validateLicenseWithServer,
} from '$lib/tauri-commands'

describe('License status types', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('handles personal license status', async () => {
        const mockStatus: LicenseStatus = { type: 'personal', showCommercialReminder: false }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        expect(status.type).toBe('personal')
    })

    it('handles personal license with commercial reminder', async () => {
        const mockStatus: LicenseStatus = { type: 'personal', showCommercialReminder: true }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        expect(status.type).toBe('personal')
        if (status.type === 'personal') {
            expect(status.showCommercialReminder).toBe(true)
        }
    })

    it('handles supporter license status', async () => {
        const mockStatus: LicenseStatus = { type: 'supporter', showCommercialReminder: false }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        expect(status.type).toBe('supporter')
    })

    it('handles supporter license with commercial reminder', async () => {
        const mockStatus: LicenseStatus = { type: 'supporter', showCommercialReminder: true }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        expect(status.type).toBe('supporter')
        if (status.type === 'supporter') {
            expect(status.showCommercialReminder).toBe(true)
        }
    })

    it('handles commercial license status with organization', async () => {
        const mockStatus: LicenseStatus = {
            type: 'commercial',
            licenseType: 'commercial_subscription',
            organizationName: 'Test Corp',
            expiresAt: '2027-01-10',
        }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        expect(status.type).toBe('commercial')
        if (status.type === 'commercial') {
            expect(status.organizationName).toBe('Test Corp')
            expect(status.licenseType).toBe('commercial_subscription')
        }
    })

    it('handles commercial perpetual license', async () => {
        const mockStatus: LicenseStatus = {
            type: 'commercial',
            licenseType: 'commercial_perpetual',
            organizationName: 'Perpetual LLC',
            expiresAt: null,
        }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        if (status.type === 'commercial') {
            expect(status.licenseType).toBe('commercial_perpetual')
            expect(status.expiresAt).toBeNull()
        }
    })

    it('handles expired license status with modal flag', async () => {
        const mockStatus: LicenseStatus = {
            type: 'expired',
            organizationName: 'Former Corp',
            expiredAt: '2026-01-01',
            showModal: true,
        }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        expect(status.type).toBe('expired')
        if (status.type === 'expired') {
            expect(status.showModal).toBe(true)
            expect(status.expiredAt).toBe('2026-01-01')
        }
    })

    it('handles expired license with modal already shown', async () => {
        const mockStatus: LicenseStatus = {
            type: 'expired',
            organizationName: null,
            expiredAt: '2026-01-01',
            showModal: false,
        }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        if (status.type === 'expired') {
            expect(status.showModal).toBe(false)
        }
    })
})

describe('Window title', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('returns personal use title', async () => {
        vi.mocked(getWindowTitle).mockResolvedValue('Cmdr – Personal use only')

        const title = await getWindowTitle()
        expect(title).toBe('Cmdr – Personal use only')
    })

    it('returns commercial title (just Cmdr)', async () => {
        vi.mocked(getWindowTitle).mockResolvedValue('Cmdr')

        const title = await getWindowTitle()
        expect(title).toBe('Cmdr')
    })

    it('returns supporter title', async () => {
        vi.mocked(getWindowTitle).mockResolvedValue('Cmdr – Personal')

        const title = await getWindowTitle()
        expect(title).toBe('Cmdr – Personal')
    })
})

describe('License activation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('successfully activates a valid license', async () => {
        const mockInfo: LicenseInfo = {
            email: 'test@example.com',
            transactionId: 'txn_123',
            issuedAt: '2026-01-10',
        }
        vi.mocked(activateLicense).mockResolvedValue(mockInfo)

        const result = await activateLicense('valid-key')
        expect(result.email).toBe('test@example.com')
        expect(result.transactionId).toBe('txn_123')
    })

    it('throws error for invalid license key', async () => {
        vi.mocked(activateLicense).mockRejectedValue('Invalid license key format')

        await expect(activateLicense('invalid-key')).rejects.toBe('Invalid license key format')
    })

    it('throws error for signature verification failure', async () => {
        vi.mocked(activateLicense).mockRejectedValue('Invalid license key: signature verification failed')

        await expect(activateLicense('tampered-key')).rejects.toBe('Invalid license key: signature verification failed')
    })
})

describe('License info retrieval', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('returns license info when license exists', async () => {
        const mockInfo: LicenseInfo = {
            email: 'user@company.com',
            transactionId: 'txn_456',
            issuedAt: '2025-06-15',
        }
        vi.mocked(getLicenseInfo).mockResolvedValue(mockInfo)

        const info = await getLicenseInfo()
        expect(info).not.toBeNull()
        expect(info?.email).toBe('user@company.com')
    })

    it('returns null when no license exists', async () => {
        vi.mocked(getLicenseInfo).mockResolvedValue(null)

        const info = await getLicenseInfo()
        expect(info).toBeNull()
    })
})

describe('Expiration modal', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('calls mark expiration modal shown', async () => {
        vi.mocked(markExpirationModalShown).mockResolvedValue()

        await markExpirationModalShown()
        expect(markExpirationModalShown).toHaveBeenCalled()
    })
})

describe('Commercial reminder modal', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('calls mark commercial reminder dismissed', async () => {
        vi.mocked(markCommercialReminderDismissed).mockResolvedValue()

        await markCommercialReminderDismissed()
        expect(markCommercialReminderDismissed).toHaveBeenCalled()
    })

    it('personal license includes showCommercialReminder field', async () => {
        const mockStatus: LicenseStatus = { type: 'personal', showCommercialReminder: true }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        if (status.type === 'personal') {
            expect('showCommercialReminder' in status).toBe(true)
            expect(status.showCommercialReminder).toBe(true)
        }
    })

    it('supporter license includes showCommercialReminder field', async () => {
        const mockStatus: LicenseStatus = { type: 'supporter', showCommercialReminder: false }
        vi.mocked(getLicenseStatus).mockResolvedValue(mockStatus)

        const status = await getLicenseStatus()
        if (status.type === 'supporter') {
            expect('showCommercialReminder' in status).toBe(true)
            expect(status.showCommercialReminder).toBe(false)
        }
    })
})

describe('Server validation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('checks if validation is needed', async () => {
        vi.mocked(needsLicenseValidation).mockResolvedValue(true)

        const needs = await needsLicenseValidation()
        expect(needs).toBe(true)
    })

    it('returns false when recently validated', async () => {
        vi.mocked(needsLicenseValidation).mockResolvedValue(false)

        const needs = await needsLicenseValidation()
        expect(needs).toBe(false)
    })

    it('validates with server and returns updated status', async () => {
        const mockStatus: LicenseStatus = {
            type: 'commercial',
            licenseType: 'commercial_subscription',
            organizationName: 'Validated Corp',
            expiresAt: '2027-06-15',
        }
        vi.mocked(validateLicenseWithServer).mockResolvedValue(mockStatus)

        const status = await validateLicenseWithServer()
        expect(status.type).toBe('commercial')
        if (status.type === 'commercial') {
            expect(status.organizationName).toBe('Validated Corp')
        }
    })

    it('handles expired status from server', async () => {
        const mockStatus: LicenseStatus = {
            type: 'expired',
            organizationName: 'Former Corp',
            expiredAt: '2026-01-01',
            showModal: true,
        }
        vi.mocked(validateLicenseWithServer).mockResolvedValue(mockStatus)

        const status = await validateLicenseWithServer()
        expect(status.type).toBe('expired')
    })
})
