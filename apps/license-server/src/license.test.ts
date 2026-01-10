import { describe, expect, it } from 'vitest'
import { formatLicenseKey, generateLicenseKey, type LicenseData } from './license'
import * as ed from '@noble/ed25519'

describe('formatLicenseKey', () => {
    it('formats a key into uppercase groups', () => {
        const key = 'eyJlbWFpbCI6InRlc3RAZXhhbXBsZS5jb20ifQ.c2lnbmF0dXJl'
        const formatted = formatLicenseKey(key)

        // Should be uppercase
        expect(formatted).toBe(formatted.toUpperCase())

        // Should contain dashes separating groups
        expect(formatted).toContain('-')
    })

    it('produces consistent output for the same input', () => {
        const key = 'someTestKey.signature'
        const result1 = formatLicenseKey(key)
        const result2 = formatLicenseKey(key)

        expect(result1).toBe(result2)
    })

    it('removes non-alphanumeric characters', () => {
        const key = 'abc.def/ghi+jkl='
        const formatted = formatLicenseKey(key)

        expect(formatted).not.toContain('.')
        expect(formatted).not.toContain('/')
        expect(formatted).not.toContain('+')
        expect(formatted).not.toContain('=')
    })
})

describe('generateLicenseKey', () => {
    it('generates a key in payload.signature format', async () => {
        // Generate a test key pair
        const privateKey = ed.utils.randomSecretKey()
        const privateKeyHex = Buffer.from(privateKey).toString('hex')

        const licenseData: LicenseData = {
            email: 'test@example.com',
            transactionId: 'txn_123',
            issuedAt: '2026-01-08T12:00:00Z',
            type: 'commercial_subscription',
        }

        const key = await generateLicenseKey(licenseData, privateKeyHex)

        // Should have two parts separated by dot
        const parts = key.split('.')
        expect(parts).toHaveLength(2)

        // Both parts should be base64 encoded
        expect(() => atob(parts[0])).not.toThrow()
        expect(() => atob(parts[1])).not.toThrow()
    })

    it('embeds license data in the payload', async () => {
        const privateKey = ed.utils.randomSecretKey()
        const privateKeyHex = Buffer.from(privateKey).toString('hex')

        const licenseData: LicenseData = {
            email: 'user@domain.com',
            transactionId: 'txn_abc123',
            issuedAt: '2026-01-08T12:00:00Z',
            type: 'commercial_subscription',
        }

        const key = await generateLicenseKey(licenseData, privateKeyHex)
        const [payloadBase64] = key.split('.')
        const payloadJson = atob(payloadBase64)
        const decoded = JSON.parse(payloadJson) as LicenseData

        expect(decoded.email).toBe(licenseData.email)
        expect(decoded.transactionId).toBe(licenseData.transactionId)
        expect(decoded.issuedAt).toBe(licenseData.issuedAt)
    })

    it('produces verifiable signatures', async () => {
        const privateKey = ed.utils.randomSecretKey()
        const publicKey = await ed.getPublicKeyAsync(privateKey)
        const privateKeyHex = Buffer.from(privateKey).toString('hex')

        const licenseData: LicenseData = {
            email: 'test@test.com',
            transactionId: 'txn_verify',
            issuedAt: '2026-01-08T12:00:00Z',
            type: 'commercial_perpetual',
        }

        const key = await generateLicenseKey(licenseData, privateKeyHex)
        const [payloadBase64, signatureBase64] = key.split('.')

        // Decode payload and signature
        const payloadBytes = Uint8Array.from(atob(payloadBase64), (c) => c.charCodeAt(0))
        const signatureBytes = Uint8Array.from(atob(signatureBase64), (c) => c.charCodeAt(0))

        // Verify signature
        const isValid = await ed.verifyAsync(signatureBytes, payloadBytes, publicKey)
        expect(isValid).toBe(true)
    })

    it('rejects tampered payloads', async () => {
        const privateKey = ed.utils.randomSecretKey()
        const publicKey = await ed.getPublicKeyAsync(privateKey)
        const privateKeyHex = Buffer.from(privateKey).toString('hex')

        const licenseData: LicenseData = {
            email: 'original@test.com',
            transactionId: 'txn_original',
            issuedAt: '2026-01-08T12:00:00Z',
            type: 'supporter',
        }

        const key = await generateLicenseKey(licenseData, privateKeyHex)
        const [, signatureBase64] = key.split('.')

        // Create tampered payload
        const tamperedData: LicenseData = {
            email: 'hacker@evil.com',
            transactionId: 'txn_original',
            issuedAt: '2026-01-08T12:00:00Z',
            type: 'supporter',
        }
        const tamperedPayload = JSON.stringify(tamperedData)
        const tamperedPayloadBytes = new TextEncoder().encode(tamperedPayload)
        const signatureBytes = Uint8Array.from(atob(signatureBase64), (c) => c.charCodeAt(0))

        // Signature should NOT verify for tampered payload
        const isValid = await ed.verifyAsync(signatureBytes, tamperedPayloadBytes, publicKey)
        expect(isValid).toBe(false)
    })
})
