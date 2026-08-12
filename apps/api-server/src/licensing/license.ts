import * as ed from '@noble/ed25519'

export const licenseTypes = ['commercial_subscription', 'commercial_perpetual'] as const
export type LicenseType = (typeof licenseTypes)[number]

export interface LicenseData {
  email: string
  transactionId: string
  issuedAt: string
  type: LicenseType
  organizationName?: string // For commercial licenses
  shortCode?: string // Embedded so the app can display it even when activated via full key
}

/** Unambiguous alphabet (no 0/O, 1/I/L). Shared by short license codes and error report IDs. */
export const unambiguousAlphabet = '23456789ABCDEFGHJKMNPQRSTUVWXYZ'

/**
 * Generate `len` random chars from `unambiguousAlphabet` using rejection sampling
 * to avoid modulo bias.
 */
export function generateRandomChars(len: number): string {
  const chars = unambiguousAlphabet
  // 256 - (256 % 31) = 232; bytes >= this would skew the distribution
  const maxUnbiased = 256 - (256 % chars.length)
  let out = ''
  while (out.length < len) {
    const batch = crypto.getRandomValues(new Uint8Array(len - out.length))
    for (const byte of batch) {
      if (byte < maxUnbiased && out.length < len) {
        out += chars[byte % chars.length]
      }
    }
  }
  return out
}

/**
 * Generate a short, readable license code.
 * Format: CMDR-XXXX-XXXX-XXXX (16 chars + prefix, using unambiguous characters)
 */
export function generateShortCode(): string {
  const raw = generateRandomChars(12)
  return `CMDR-${raw.slice(0, 4)}-${raw.slice(4, 8)}-${raw.slice(8, 12)}`
}

/**
 * Generate a short ID with a prefix (e.g. `ERR-XXXXX`).
 * Uses the same unambiguous alphabet as license short codes.
 */
export function generateShortId(prefix: string, len: number): string {
  return `${prefix}-${generateRandomChars(len)}`
}

/**
 * Validate that a string looks like a short license code.
 */
export function isValidShortCode(code: string): boolean {
  return /^CMDR-[23456789A-HJ-NP-Z]{4}-[23456789A-HJ-NP-Z]{4}-[23456789A-HJ-NP-Z]{4}$/i.test(code)
}

/**
 * Generate a signed license key.
 * Format: base64(payload).base64(signature)
 */
export async function generateLicenseKey(data: LicenseData, privateKeyHex: string): Promise<string> {
  const payload = JSON.stringify(data)
  const payloadBytes = new TextEncoder().encode(payload)

  // Sign with Ed25519
  const privateKey = hexToBytes(privateKeyHex)
  const signature = await ed.signAsync(payloadBytes, privateKey)

  // Encode as base64
  const payloadBase64 = bytesToBase64(payloadBytes)
  const signatureBase64 = bytesToBase64(signature)

  return `${payloadBase64}.${signatureBase64}`
}

// Helper functions
function hexToBytes(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2)
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16)
  }
  return bytes
}

function bytesToBase64(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString('base64')
}
