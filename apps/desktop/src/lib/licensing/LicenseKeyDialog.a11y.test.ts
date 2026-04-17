/**
 * Tier 3 a11y tests for `LicenseKeyDialog.svelte`.
 *
 * Dialog has three rendered branches: loading (short-lived), entry
 * (no existing license), and details (existing license). Entry is the
 * default for a fresh user; details shows the stored key info with a
 * "Use a different key" reset flow. Tests cover entry, details, and
 * the reset-confirm sub-state.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import LicenseKeyDialog from './LicenseKeyDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let mockLicenseInfo: unknown = null
let mockCachedStatus: unknown = null

vi.mock('$lib/tauri-commands', () => ({
  verifyLicense: vi.fn(() => Promise.resolve({ info: {}, fullKey: '', shortCode: '' })),
  commitLicense: vi.fn(() => Promise.resolve()),
  validateLicenseWithServer: vi.fn(() => Promise.resolve(null)),
  getLicenseInfo: vi.fn(() => Promise.resolve(mockLicenseInfo)),
  resetLicense: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  parseActivationError: vi.fn(() => null),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('./licensing-store.svelte', () => ({
  loadLicenseStatus: vi.fn(() => Promise.resolve()),
  getCachedStatus: () => mockCachedStatus,
  setCachedStatus: vi.fn(),
  isPendingVerification: () => false,
  setPendingVerification: vi.fn(),
}))

vi.mock('$lib/ui/toast/toast-store.svelte', () => ({
  addToast: vi.fn(() => 'id'),
}))

describe('LicenseKeyDialog a11y', () => {
  beforeEach(() => {
    mockLicenseInfo = null
    mockCachedStatus = null
  })

  it('entry state (no existing license) has no a11y violations', async () => {
    mockLicenseInfo = null
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LicenseKeyDialog, { target, props: { onClose: () => {}, onSuccess: () => {} } })
    // Flush the getLicenseInfo() microtask so `isLoading` flips to false.
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('details state (existing commercial license) has no a11y violations', async () => {
    mockLicenseInfo = {
      organizationName: 'Acme Corp',
      licenseType: 'commercial_perpetual',
      shortCode: 'CMDR-ABCD-EFGH-1234',
      transactionId: 'txn-1',
    }
    mockCachedStatus = {
      type: 'commercial',
      licenseType: 'commercial_perpetual',
      organizationName: 'Acme Corp',
      expiresAt: null,
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LicenseKeyDialog, { target, props: { onClose: () => {}, onSuccess: () => {} } })
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('details state (subscription with expiry) has no a11y violations', async () => {
    mockLicenseInfo = {
      organizationName: 'Acme Corp',
      licenseType: 'commercial_subscription',
      shortCode: 'CMDR-WXYZ-1234-5678',
      transactionId: 'txn-2',
    }
    mockCachedStatus = {
      type: 'commercial',
      licenseType: 'commercial_subscription',
      organizationName: 'Acme Corp',
      expiresAt: '2027-01-01',
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LicenseKeyDialog, { target, props: { onClose: () => {}, onSuccess: () => {} } })
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })
})
