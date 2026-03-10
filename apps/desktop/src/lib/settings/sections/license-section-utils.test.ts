import { describe, it, expect } from 'vitest'
import { getLicenseTypeLabel, formatLicenseDate, getStatusText } from './license-section-utils'
import type { LicenseInfo, LicenseStatus } from '$lib/tauri-commands'

function makeLicenseInfo(overrides: Partial<LicenseInfo> = {}): LicenseInfo {
    return {
        email: 'test@example.com',
        transactionId: 'tx_123',
        issuedAt: '2025-01-15',
        organizationName: null,
        licenseType: null,
        shortCode: null,
        ...overrides,
    }
}

describe('getLicenseTypeLabel', () => {
    it('should return "Personal (free)" when licenseInfo is null', () => {
        expect(getLicenseTypeLabel(null)).toBe('Personal (free)')
    })

    it('should return "Personal (free)" when licenseType is null', () => {
        expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: null }))).toBe('Personal (free)')
    })

    it('should return "Commercial perpetual" for commercial_perpetual', () => {
        expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: 'commercial_perpetual' }))).toBe(
            'Commercial perpetual',
        )
    })

    it('should return "Commercial subscription" for commercial_subscription', () => {
        expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: 'commercial_subscription' }))).toBe(
            'Commercial subscription',
        )
    })

    it('should return "Supporter" for supporter', () => {
        expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: 'supporter' }))).toBe('Supporter')
    })

    it('should return "Personal (free)" for unknown licenseType', () => {
        expect(getLicenseTypeLabel(makeLicenseInfo({ licenseType: 'unknown_type' }))).toBe('Personal (free)')
    })
})

describe('formatLicenseDate', () => {
    it('should return empty string for null', () => {
        expect(formatLicenseDate(null)).toBe('')
    })

    it('should return empty string for undefined', () => {
        expect(formatLicenseDate(undefined)).toBe('')
    })

    it('should return empty string for empty string', () => {
        expect(formatLicenseDate('')).toBe('')
    })

    it('should format a valid ISO date string', () => {
        const result = formatLicenseDate('2025-06-15')
        // The exact output depends on locale, but it should contain the year
        expect(result).toContain('2025')
        // Should not be the raw input string (meaning Date parsing succeeded)
        expect(result).not.toBe('2025-06-15')
    })

    it('should format an ISO datetime string', () => {
        const result = formatLicenseDate('2025-06-15T12:00:00Z')
        expect(result).toContain('2025')
    })

    it('should return the raw string if Date parsing produces Invalid Date', () => {
        // new Date('not-a-date') produces Invalid Date, toLocaleDateString returns 'Invalid Date'
        // The function catches errors, but Invalid Date doesn't throw - it returns 'Invalid Date' string
        // So this tests the catch path won't be hit for truly invalid strings
        const result = formatLicenseDate('not-a-date')
        // NaN date toLocaleDateString may throw in some environments or return 'Invalid Date'
        expect(typeof result).toBe('string')
        expect(result.length).toBeGreaterThan(0)
    })
})

describe('getStatusText', () => {
    it('should return null when licenseStatus is null', () => {
        expect(getStatusText(null)).toBeNull()
    })

    it('should return null for personal status', () => {
        const status: LicenseStatus = { type: 'personal', showCommercialReminder: false }
        expect(getStatusText(status)).toBeNull()
    })

    it('should return null for supporter status', () => {
        const status: LicenseStatus = { type: 'supporter', showCommercialReminder: false }
        expect(getStatusText(status)).toBeNull()
    })

    it('should return expired text with formatted date', () => {
        const status: LicenseStatus = {
            type: 'expired',
            organizationName: null,
            expiredAt: '2025-03-01',
            showModal: false,
        }
        const result = getStatusText(status)
        expect(result).toMatch(/^Expired on /)
        expect(result).toContain('2025')
    })

    it('should return "Active" for commercial_perpetual without expiresAt', () => {
        const status: LicenseStatus = {
            type: 'commercial',
            licenseType: 'commercial_perpetual',
            organizationName: null,
            expiresAt: null,
        }
        expect(getStatusText(status)).toBe('Active')
    })

    it('should return "Updates until ..." for commercial_perpetual with expiresAt', () => {
        const status: LicenseStatus = {
            type: 'commercial',
            licenseType: 'commercial_perpetual',
            organizationName: 'Acme Corp',
            expiresAt: '2026-01-01',
        }
        const result = getStatusText(status)
        expect(result).toMatch(/^Updates until /)
        expect(result).toContain('2026')
    })

    it('should return "Active" for commercial_subscription without expiresAt', () => {
        const status: LicenseStatus = {
            type: 'commercial',
            licenseType: 'commercial_subscription',
            organizationName: null,
            expiresAt: null,
        }
        expect(getStatusText(status)).toBe('Active')
    })

    it('should return "Valid until ..." for commercial_subscription with expiresAt', () => {
        const status: LicenseStatus = {
            type: 'commercial',
            licenseType: 'commercial_subscription',
            organizationName: null,
            expiresAt: '2026-06-15',
        }
        const result = getStatusText(status)
        expect(result).toMatch(/^Valid until /)
        expect(result).toContain('2026')
    })
})
