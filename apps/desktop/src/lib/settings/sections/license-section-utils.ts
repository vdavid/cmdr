import type { LicenseInfo, LicenseStatus } from '$lib/tauri-commands'

export function getLicenseTypeLabel(licenseInfo: LicenseInfo | null): string {
  if (!licenseInfo) return 'Personal (free)'
  if (licenseInfo.licenseType === 'commercial_perpetual') return 'Commercial perpetual'
  if (licenseInfo.licenseType === 'commercial_subscription') return 'Commercial subscription'
  return 'Personal (free)'
}

export function formatLicenseDate(dateStr: string | null | undefined): string {
  if (!dateStr) return ''
  try {
    return new Date(dateStr).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
    })
  } catch {
    return dateStr
  }
}

export function getStatusText(licenseStatus: LicenseStatus | null): string | null {
  if (!licenseStatus) return null
  if (licenseStatus.type === 'expired') return `Expired on ${formatLicenseDate(licenseStatus.expiredAt)}`
  if (licenseStatus.type === 'commercial') {
    if (licenseStatus.licenseType === 'commercial_perpetual') {
      return licenseStatus.expiresAt ? `Updates until ${formatLicenseDate(licenseStatus.expiresAt)}` : 'Active'
    }
    return licenseStatus.expiresAt ? `Valid until ${formatLicenseDate(licenseStatus.expiresAt)}` : 'Active'
  }
  return null
}
