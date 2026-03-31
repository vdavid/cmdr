import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/tauri-commands', () => ({
  getLicenseStatus: vi.fn(),
  needsLicenseValidation: vi.fn(),
  hasLicenseBeenValidated: vi.fn(),
  validateLicenseWithServer: vi.fn(),
}))

import type { LicenseStatus } from '$lib/tauri-commands'
import { getLicenseStatus, needsLicenseValidation, validateLicenseWithServer } from '$lib/tauri-commands'
import {
  getCachedStatus,
  loadLicenseStatus,
  triggerValidationIfNeeded,
  resetForTesting,
} from './licensing-store.svelte'

const personalStatus: LicenseStatus = { type: 'personal', showCommercialReminder: false }
const commercialStatus: LicenseStatus = {
  type: 'commercial',
  licenseType: 'commercial_subscription',
  organizationName: 'Test Corp',
  expiresAt: '2027-01-10',
}
const expiredStatus: LicenseStatus = {
  type: 'expired',
  organizationName: 'Former Corp',
  expiredAt: '2026-01-01',
  showModal: true,
}

describe('licensing-store', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetForTesting()
  })

  describe('getCachedStatus', () => {
    it('returns null before status is loaded', () => {
      expect(getCachedStatus()).toBeNull()
    })
  })

  describe('loadLicenseStatus', () => {
    it('fetches status from backend and caches it', async () => {
      vi.mocked(getLicenseStatus).mockResolvedValue(personalStatus)

      const result = await loadLicenseStatus()

      expect(result).toEqual(personalStatus)
      expect(getCachedStatus()).toEqual(personalStatus)
    })
  })

  describe('triggerValidationIfNeeded', () => {
    it('skips validation when not needed', async () => {
      vi.mocked(needsLicenseValidation).mockResolvedValue(false)

      const result = await triggerValidationIfNeeded()

      expect(result).toBeNull()
      expect(validateLicenseWithServer).not.toHaveBeenCalled()
    })

    it('validates with server and updates cache when needed', async () => {
      vi.mocked(needsLicenseValidation).mockResolvedValue(true)
      vi.mocked(validateLicenseWithServer).mockResolvedValue(commercialStatus)

      const result = await triggerValidationIfNeeded()

      expect(result).toEqual(commercialStatus)
      expect(getCachedStatus()).toEqual(commercialStatus)
    })

    it('returns null and keeps cached status on network error', async () => {
      vi.mocked(getLicenseStatus).mockResolvedValue(personalStatus)
      vi.mocked(needsLicenseValidation).mockResolvedValue(true)
      vi.mocked(validateLicenseWithServer).mockRejectedValue(new Error('Network error'))

      await loadLicenseStatus()
      const result = await triggerValidationIfNeeded()

      expect(result).toBeNull()
      expect(getCachedStatus()).toEqual(personalStatus)
    })

    it('caches expired status from server validation', async () => {
      vi.mocked(needsLicenseValidation).mockResolvedValue(true)
      vi.mocked(validateLicenseWithServer).mockResolvedValue(expiredStatus)

      const result = await triggerValidationIfNeeded()

      expect(result).toEqual(expiredStatus)
      expect(getCachedStatus()).toEqual(expiredStatus)
    })
  })
})
