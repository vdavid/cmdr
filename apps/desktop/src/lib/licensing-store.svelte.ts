/**
 * Licensing store for managing license state across the app.
 */

import {
    getLicenseStatus,
    needsLicenseValidation,
    validateLicenseWithServer,
    type LicenseStatus,
} from './tauri-commands'

/**
 * Store state - kept in a single object to avoid ESLint unused vars warnings.
 */
const licenseState = {
    cachedStatus: null as LicenseStatus | null,
    shouldShowModal: false,
}

/**
 * Loads the license status from the backend.
 * Should be called once at app startup.
 */
export async function loadLicenseStatus(): Promise<LicenseStatus> {
    const status = await getLicenseStatus()
    licenseState.cachedStatus = status

    // Show expiration modal if license expired and not shown before
    if (status.type === 'expired' && status.showModal) {
        licenseState.shouldShowModal = true
    }

    return status
}

/**
 * Triggers server validation if needed (7+ days since last validation).
 * Returns the updated status if validation occurred, or null if no validation was needed.
 */
export async function triggerValidationIfNeeded(): Promise<LicenseStatus | null> {
    try {
        const needsValidation = await needsLicenseValidation()
        if (!needsValidation) {
            return null
        }

        // Perform server validation
        const status = await validateLicenseWithServer()
        licenseState.cachedStatus = status

        // Update modal state if license expired
        if (status.type === 'expired' && status.showModal) {
            licenseState.shouldShowModal = true
        }

        return status
    } catch {
        // Validation failed (network error, etc.) - return null and use cached status
        return null
    }
}

/**
 * Hides the expiration modal.
 */
export function hideExpirationModal(): void {
    licenseState.shouldShowModal = false
}

/**
 * Gets the current cached license status.
 * Returns null if status hasn't been loaded yet.
 */
export function getCachedStatus(): LicenseStatus | null {
    return licenseState.cachedStatus
}

/**
 * Returns the organization name from the current license (if commercial or expired).
 */
export function getOrganizationName(): string | null {
    const status = licenseState.cachedStatus
    if (!status) return null

    if (status.type === 'commercial') {
        return status.organizationName
    }
    if (status.type === 'expired') {
        return status.organizationName
    }
    return null
}
