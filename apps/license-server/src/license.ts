import * as ed from '@noble/ed25519'

export const licenseTypes = ['supporter', 'commercial_subscription', 'commercial_perpetual'] as const
export type LicenseType = (typeof licenseTypes)[number]

export interface LicenseData {
    email: string
    transactionId: string
    issuedAt: string
    type: LicenseType
    organizationName?: string // For commercial licenses
}

/**
 * Generate a short, readable license code.
 * Format: CMDR-XXXX-XXXX-XXXX (16 chars + prefix, using unambiguous characters)
 */
export function generateShortCode(): string {
    // Use unambiguous characters (no 0/O, 1/I/L confusion)
    const chars = '23456789ABCDEFGHJKMNPQRSTUVWXYZ'
    const segments: string[] = []

    // Rejection sampling: discard bytes that would cause modulo bias (256 % 29 != 0)
    const maxUnbiased = 256 - (256 % chars.length) // 232
    let filled = 0
    while (filled < 12) {
        const batch = crypto.getRandomValues(new Uint8Array(12 - filled))
        for (const byte of batch) {
            if (byte < maxUnbiased && filled < 12) {
                segments[Math.floor(filled / 4)] = (segments[Math.floor(filled / 4)] ?? '') + chars[byte % chars.length]
                filled++
            }
        }
    }

    return `CMDR-${segments.join('-')}`
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
    return btoa(String.fromCharCode(...bytes))
}
