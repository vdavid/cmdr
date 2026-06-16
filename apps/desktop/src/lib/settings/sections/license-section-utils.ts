import type { LicenseInfo, LicenseStatus } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'

export function getLicenseTypeLabel(licenseInfo: LicenseInfo | null): string {
  if (!licenseInfo) return tString('licensing.section.typePersonal')
  if (licenseInfo.licenseType === 'commercial_perpetual') return tString('licensing.section.typeCommercialPerpetual')
  if (licenseInfo.licenseType === 'commercial_subscription')
    return tString('licensing.section.typeCommercialSubscription')
  return tString('licensing.section.typePersonal')
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
  if (licenseStatus.type === 'expired')
    return tString('licensing.section.statusExpiredOn', { date: formatLicenseDate(licenseStatus.expiredAt) })
  if (licenseStatus.type === 'commercial') {
    if (licenseStatus.licenseType === 'commercial_perpetual') {
      return licenseStatus.expiresAt
        ? tString('licensing.section.statusUpdatesUntil', { date: formatLicenseDate(licenseStatus.expiresAt) })
        : tString('licensing.section.statusActive')
    }
    return licenseStatus.expiresAt
      ? tString('licensing.section.statusValidUntil', { date: formatLicenseDate(licenseStatus.expiresAt) })
      : tString('licensing.section.statusActive')
  }
  return null
}
