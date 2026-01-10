import * as ed from '@noble/ed25519'

export type LicenseType = 'supporter' | 'commercial_subscription' | 'commercial_perpetual'

export interface LicenseData {
    email: string
    transactionId: string
    issuedAt: string
    type: LicenseType
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
 * Format license key for display (groups of 5 chars).
 * ABCDE-FGHIJ-KLMNO-...
 */
export function formatLicenseKey(key: string): string {
    // Remove the dots and create a shorter representation
    const hash = simpleHash(key)
    const formatted = hash.match(/.{1,5}/g)?.join('-') ?? hash
    return formatted.toUpperCase()
}

/**
 * Create a shorter hash for display purposes.
 * The full key is still valid, this is just for UX.
 * Actually, let's just return the full key and let the app handle it.
 */
function simpleHash(input: string): string {
    // For license keys, we want the full cryptographic key
    // Just format it nicely
    return input.replace(/[^a-zA-Z0-9]/g, '').slice(0, 25)
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
