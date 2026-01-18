import * as ed from '@noble/ed25519'

export type LicenseType = 'supporter' | 'commercial_subscription' | 'commercial_perpetual'

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

    for (let s = 0; s < 3; s++) {
        let segment = ''
        for (let i = 0; i < 4; i++) {
            const randomIndex = Math.floor(Math.random() * chars.length)
            segment += chars[randomIndex]
        }
        segments.push(segment)
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

/**
 * Format license key for display.
 * Returns the full key as-is since it must contain the `.` separator
 * for the client-side Ed25519 signature verification to work.
 */
export function formatLicenseKey(key: string): string {
    // Return the full key - clients need the complete payload.signature format
    return key
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
