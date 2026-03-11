/**
 * Licensing store for managing license state across the app.
 */

import {
    getLicenseStatus,
    needsLicenseValidation,
    hasLicenseBeenValidated,
    validateLicenseWithServer,
    type LicenseStatus,
} from '$lib/tauri-commands'

/**
 * Store state - kept in a single object to avoid ESLint unused vars warnings.
 */
const licenseState = {
    cachedStatus: null as LicenseStatus | null,
    shouldShowModal: false,
    /** True when activation succeeded locally but server verification hasn't completed yet (network error). */
    pendingVerification: false,
}

/**
 * Loads the license status from the backend.
 * Should be called once at app startup.
 */
export async function loadLicenseStatus(): Promise<LicenseStatus> {
    const status = await getLicenseStatus()
    licenseState.cachedStatus = status

    // Derive pending verification: license exists and is non-personal, but server has never confirmed it
    if (status.type === 'commercial') {
        const validated = await hasLicenseBeenValidated()
        licenseState.pendingVerification = !validated
    } else {
        licenseState.pendingVerification = false
    }

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
        licenseState.pendingVerification = false

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

/** Directly sets the cached status (used after activation to avoid a redundant backend round trip). */
export function setCachedStatus(status: LicenseStatus): void {
    licenseState.cachedStatus = status
}

/** True when license was activated locally but server verification hasn't completed yet. */
export function isPendingVerification(): boolean {
    return licenseState.pendingVerification
}

/** Mark that activation succeeded locally but server verification is still pending (network error during activation). */
export function setPendingVerification(pending: boolean): void {
    licenseState.pendingVerification = pending
}
