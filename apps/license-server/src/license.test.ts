import { describe, expect, it } from 'vitest'
import { generateLicenseKey, generateShortCode, isValidShortCode, type LicenseData } from './license'
import * as ed from '@noble/ed25519'

describe('generateShortCode', () => {
    it('generates codes in CMDR-XXXX-XXXX-XXXX format', () => {
        const code = generateShortCode()

        expect(code).toMatch(/^CMDR-[23456789A-HJ-NP-Z]{4}-[23456789A-HJ-NP-Z]{4}-[23456789A-HJ-NP-Z]{4}$/)
    })

    it('generates unique codes', () => {
        const codes = new Set<string>()
        for (let i = 0; i < 100; i++) {
            codes.add(generateShortCode())
        }
        // All 100 codes should be unique
        expect(codes.size).toBe(100)
    })

    it('uses only unambiguous characters', () => {
        // Generate many codes and check none contain ambiguous chars
        for (let i = 0; i < 50; i++) {
            const code = generateShortCode()
            // Should not contain: 0, O, 1, I, L
            expect(code).not.toMatch(/[01OIL]/)
        }
    })
})

describe('isValidShortCode', () => {
    it('accepts valid codes', () => {
        expect(isValidShortCode('CMDR-ABCD-EFGH-2345')).toBe(true)
        expect(isValidShortCode('cmdr-abcd-efgh-2345')).toBe(true) // Case insensitive
        expect(isValidShortCode('CMDR-2345-6789-ABCD')).toBe(true)
    })

    it('rejects invalid codes', () => {
        expect(isValidShortCode('ABCD-EFGH-IJKL-MNOP')).toBe(false) // No CMDR prefix
        expect(isValidShortCode('CMDR-ABC-EFGH-1234')).toBe(false) // Segment too short
        expect(isValidShortCode('CMDR-ABCDE-FGHI-1234')).toBe(false) // Segment too long
        expect(isValidShortCode('CMDR-ABCD-EFGH')).toBe(false) // Missing segment
        expect(isValidShortCode('something.else')).toBe(false) // Full key format
        expect(isValidShortCode('')).toBe(false)
    })

    it('accepts generated codes', () => {
        for (let i = 0; i < 20; i++) {
            const code = generateShortCode()
            expect(isValidShortCode(code)).toBe(true)
        }
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

    it('includes organizationName in payload when provided', async () => {
        const privateKey = ed.utils.randomSecretKey()
        const privateKeyHex = Buffer.from(privateKey).toString('hex')

        const licenseData: LicenseData = {
            email: 'corp@example.com',
            transactionId: 'txn_corp123',
            issuedAt: '2026-01-08T12:00:00Z',
            type: 'commercial_subscription',
            organizationName: 'Acme Corporation',
        }

        const key = await generateLicenseKey(licenseData, privateKeyHex)
        const [payloadBase64] = key.split('.')
        const payloadJson = atob(payloadBase64)
        const decoded = JSON.parse(payloadJson) as LicenseData

        expect(decoded.organizationName).toBe('Acme Corporation')
        expect(decoded.email).toBe('corp@example.com')
        expect(decoded.type).toBe('commercial_subscription')
    })

    it('omits organizationName from payload when not provided', async () => {
        const privateKey = ed.utils.randomSecretKey()
        const privateKeyHex = Buffer.from(privateKey).toString('hex')

        const licenseData: LicenseData = {
            email: 'supporter@example.com',
            transactionId: 'txn_supporter',
            issuedAt: '2026-01-08T12:00:00Z',
            type: 'supporter',
            // organizationName intentionally omitted
        }

        const key = await generateLicenseKey(licenseData, privateKeyHex)
        const [payloadBase64] = key.split('.')
        const payloadJson = atob(payloadBase64)
        const decoded = JSON.parse(payloadJson) as LicenseData

        expect(decoded.organizationName).toBeUndefined()
        expect(decoded.type).toBe('supporter')
    })

    it('protects organizationName from tampering', async () => {
        const privateKey = ed.utils.randomSecretKey()
        const publicKey = await ed.getPublicKeyAsync(privateKey)
        const privateKeyHex = Buffer.from(privateKey).toString('hex')

        const licenseData: LicenseData = {
            email: 'corp@example.com',
            transactionId: 'txn_corp',
            issuedAt: '2026-01-08T12:00:00Z',
            type: 'commercial_perpetual',
            organizationName: 'Small Startup',
        }

        const key = await generateLicenseKey(licenseData, privateKeyHex)
        const [, signatureBase64] = key.split('.')

        // Try to change organization name to something else
        const tamperedData: LicenseData = {
            ...licenseData,
            organizationName: 'Giant Enterprise', // Changed!
        }
        const tamperedPayload = JSON.stringify(tamperedData)
        const tamperedPayloadBytes = new TextEncoder().encode(tamperedPayload)
        const signatureBytes = Uint8Array.from(atob(signatureBase64), (c) => c.charCodeAt(0))

        // Signature should NOT verify for tampered org name
        const isValid = await ed.verifyAsync(signatureBytes, tamperedPayloadBytes, publicKey)
        expect(isValid).toBe(false)
    })
})
