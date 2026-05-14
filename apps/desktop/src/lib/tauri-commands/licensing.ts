// License commands

import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

/** License types */
export type LicenseType = 'commercial_subscription' | 'commercial_perpetual'

/** Application license status */
export type LicenseStatus =
  | { type: 'personal'; showCommercialReminder: boolean }
  | { type: 'commercial'; licenseType: LicenseType; organizationName: string | null; expiresAt: string | null }
  | { type: 'expired'; organizationName: string | null; expiredAt: string; showModal: boolean }

/** License information from activation */
export interface LicenseInfo {
  email: string
  transactionId: string
  issuedAt: string
  organizationName: string | null
  licenseType: string | null
  shortCode: string | null
}

/** Result of verifying a license key without persisting it. */
export interface VerifyResult {
  info: LicenseInfo
  fullKey: string
  shortCode: string | null
}

/** Error codes from the license activation flow (matches Rust `LicenseActivationError` enum). */
export type LicenseActivationErrorCode =
  | 'invalidFormat'
  | 'badEncoding'
  | 'badSignature'
  | 'badPayload'
  | 'shortCodeNotFound'
  | 'networkError'
  | 'serverError'

/** Typed activation error returned by the backend. */
export interface LicenseActivationError {
  code: LicenseActivationErrorCode
  detail?: string
}

const validActivationErrorCodes = new Set<string>([
  'invalidFormat',
  'badEncoding',
  'badSignature',
  'badPayload',
  'shortCodeNotFound',
  'networkError',
  'serverError',
])

/**
 * Parses a Tauri invoke error into a typed activation error.
 *
 * Tauri 2 serializes `Serialize`-implementing error types directly as the rejection value.
 * The value may arrive as a parsed object or as a JSON string (depending on Tauri version/config).
 */
export function parseActivationError(e: unknown): LicenseActivationError | null {
  // Case 1: already a parsed object with a `code` field
  if (typeof e === 'object' && e !== null && 'code' in e) {
    const obj = e
    if (typeof obj.code === 'string' && validActivationErrorCodes.has(obj.code)) {
      return e as LicenseActivationError
    }
  }

  // Case 2: JSON string (Tauri may wrap the serialized error in a string)
  if (typeof e === 'string') {
    try {
      const parsed: unknown = JSON.parse(e)
      if (typeof parsed === 'object' && parsed !== null && 'code' in parsed) {
        const obj = parsed
        if (typeof obj.code === 'string' && validActivationErrorCodes.has(obj.code)) {
          return parsed as LicenseActivationError
        }
      }
    } catch {
      // Not JSON, fall through
    }
  }

  return null
}

/**
 * Gets the current application license status.
 * @returns Current license status (personal, commercial, or expired)
 */
export async function getLicenseStatus(): Promise<LicenseStatus> {
  return commands.getLicenseStatus()
}

/**
 * Gets the window title based on current license status.
 * @returns Window title string (like "Cmdr – Personal use only")
 */
export async function getWindowTitle(): Promise<string> {
  return commands.getWindowTitle()
}

/**
 * Activates a license key (verify + commit in one call).
 * Kept for backward compatibility. New code should use verifyLicense + commitLicense.
 */
export async function activateLicense(licenseKey: string): Promise<LicenseInfo> {
  const res = await commands.activateLicense(licenseKey)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Verifies a license key offline without writing anything to disk. */
export async function verifyLicense(licenseKey: string): Promise<VerifyResult> {
  const res = await commands.verifyLicense(licenseKey)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Persists a verified license key to disk and updates caches. */
export async function commitLicense(licenseKey: string, shortCode: string | null): Promise<LicenseInfo> {
  const res = await commands.commitLicense(licenseKey, shortCode)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Gets information about the current stored license.
 * @returns License info if a valid license is stored, null otherwise
 */
export async function getLicenseInfo(): Promise<LicenseInfo | null> {
  return commands.getLicenseInfo()
}

/**
 * Marks the expiration modal as shown to prevent showing it again.
 */
export async function markExpirationModalShown(): Promise<void> {
  await commands.markExpirationModalShown()
}

/**
 * Marks the commercial reminder as dismissed (resets the 30-day timer).
 */
export async function markCommercialReminderDismissed(): Promise<void> {
  await commands.markCommercialReminderDismissed()
}

/**
 * Resets all license data (debug builds only).
 */
export async function resetLicense(): Promise<void> {
  await commands.resetLicense()
}

/**
 * Checks if the license needs re-validation with the server.
 * Should be called on app startup to determine if validateLicenseWithServer should be invoked.
 * @returns True if validation is needed (7+ days since last validation)
 */
export async function needsLicenseValidation(): Promise<boolean> {
  return commands.needsLicenseValidation()
}

/**
 * Checks if a server validation has ever completed for the current license.
 * Returns false if the license was committed locally but never verified with the server.
 */
export async function hasLicenseBeenValidated(): Promise<boolean> {
  return commands.hasLicenseBeenValidated()
}

/**
 * Validates the license with the license server.
 * If transactionId is provided, uses it directly (for pre-commit validation).
 * If omitted, reads from the stored license (for periodic re-validation).
 */
export async function validateLicenseWithServer(transactionId?: string): Promise<LicenseStatus> {
  const res = await commands.validateLicenseWithServer(transactionId ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}
