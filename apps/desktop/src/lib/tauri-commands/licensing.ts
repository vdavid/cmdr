// License commands

import { invoke } from '@tauri-apps/api/core'

/** License types */
export type LicenseType = 'supporter' | 'commercial_subscription' | 'commercial_perpetual'

/** Application license status */
export type LicenseStatus =
    | { type: 'personal'; showCommercialReminder: boolean }
    | { type: 'supporter'; showCommercialReminder: boolean }
    | { type: 'commercial'; licenseType: LicenseType; organizationName: string | null; expiresAt: string | null }
    | { type: 'expired'; organizationName: string | null; expiredAt: string; showModal: boolean }

/** License information from activation */
export interface LicenseInfo {
    email: string
    transactionId: string
    issuedAt: string
    organizationName: string | null
    shortCode: string | null
}

/**
 * Gets the current application license status.
 * @returns Current license status (personal, supporter, commercial, or expired)
 */
export async function getLicenseStatus(): Promise<LicenseStatus> {
    return invoke<LicenseStatus>('get_license_status')
}

/**
 * Gets the window title based on current license status.
 * @returns Window title string (like "Cmdr – Personal use only")
 */
export async function getWindowTitle(): Promise<string> {
    return invoke<string>('get_window_title')
}

/**
 * Activates a license key.
 * @param licenseKey The license key to activate
 * @returns License info on success
 * @throws Error message on failure
 */
export async function activateLicense(licenseKey: string): Promise<LicenseInfo> {
    return invoke<LicenseInfo>('activate_license', { licenseKey })
}

/**
 * Gets information about the current stored license.
 * @returns License info if a valid license is stored, null otherwise
 */
export async function getLicenseInfo(): Promise<LicenseInfo | null> {
    return invoke<LicenseInfo | null>('get_license_info')
}

/**
 * Marks the expiration modal as shown to prevent showing it again.
 */
export async function markExpirationModalShown(): Promise<void> {
    await invoke('mark_expiration_modal_shown')
}

/**
 * Marks the commercial reminder as dismissed (resets the 30-day timer).
 */
export async function markCommercialReminderDismissed(): Promise<void> {
    await invoke('mark_commercial_reminder_dismissed')
}

/**
 * Resets all license data (debug builds only).
 */
export async function resetLicense(): Promise<void> {
    await invoke('reset_license')
}

/**
 * Checks if the license needs re-validation with the server.
 * Should be called on app startup to determine if validateLicenseWithServer should be invoked.
 * @returns True if validation is needed (7+ days since last validation)
 */
export async function needsLicenseValidation(): Promise<boolean> {
    return invoke<boolean>('needs_license_validation')
}

/**
 * Validates the license with the license server.
 * Call this when needsLicenseValidation returns true, or after activating a new license.
 * @returns Updated license status from server
 */
export async function validateLicenseWithServer(): Promise<LicenseStatus> {
    return invoke<LicenseStatus>('validate_license_with_server')
}
