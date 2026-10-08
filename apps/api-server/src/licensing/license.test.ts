import { describe, expect, it } from 'vitest'
import {
  generateLicenseKey,
  generateShortCode,
  isValidNonce,
  isValidShortCode,
  signValidationAnswer,
  validationAnswerSignaturePrefix,
  type LicenseData,
} from './license'
import * as ed from '@noble/ed25519'

// This file runs in workerd (see `vitest.workerd.config.ts`), so hex and base64 go through the
// web-standard primitives the Worker itself has. The pool's runner force-enables `nodejs_compat_v2`,
// so `Buffer` would actually resolve here; the `no-restricted-globals` ban in `eslint.config.js` is
// what keeps that from turning into false confidence.

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

/** Throws on input that isn't base64, which is what the format assertions below check for. */
function base64ToBytes(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0))
}

function base64ToUtf8(base64: string): string {
  return new TextDecoder().decode(base64ToBytes(base64))
}

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
    const privateKeyHex = bytesToHex(privateKey)

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
    expect(() => base64ToBytes(parts[0])).not.toThrow()
    expect(() => base64ToBytes(parts[1])).not.toThrow()
  })

  it('encodes a payload longer than one base64 chunk', async () => {
    const privateKey = ed.utils.randomSecretKey()
    const privateKeyHex = bytesToHex(privateKey)

    // Several times the encoder's chunk size, so a wrong chunk boundary corrupts the payload.
    const organizationName = 'Ácme Corporation '.repeat(3000)
    const licenseData: LicenseData = {
      email: 'corp@example.com',
      transactionId: 'txn_long',
      issuedAt: '2026-01-08T12:00:00Z',
      type: 'commercial_subscription',
      organizationName,
    }

    const key = await generateLicenseKey(licenseData, privateKeyHex)
    const [payloadBase64] = key.split('.')
    const decoded = JSON.parse(base64ToUtf8(payloadBase64)) as LicenseData

    expect(decoded.organizationName).toBe(organizationName)
  })

  it('embeds license data in the payload', async () => {
    const privateKey = ed.utils.randomSecretKey()
    const privateKeyHex = bytesToHex(privateKey)

    const licenseData: LicenseData = {
      email: 'user@domain.com',
      transactionId: 'txn_abc123',
      issuedAt: '2026-01-08T12:00:00Z',
      type: 'commercial_subscription',
    }

    const key = await generateLicenseKey(licenseData, privateKeyHex)
    const [payloadBase64] = key.split('.')
    const payloadJson = base64ToUtf8(payloadBase64)
    const decoded = JSON.parse(payloadJson) as LicenseData

    expect(decoded.email).toBe(licenseData.email)
    expect(decoded.transactionId).toBe(licenseData.transactionId)
    expect(decoded.issuedAt).toBe(licenseData.issuedAt)
  })

  it('produces verifiable signatures', async () => {
    const privateKey = ed.utils.randomSecretKey()
    const publicKey = await ed.getPublicKeyAsync(privateKey)
    const privateKeyHex = bytesToHex(privateKey)

    const licenseData: LicenseData = {
      email: 'test@test.com',
      transactionId: 'txn_verify',
      issuedAt: '2026-01-08T12:00:00Z',
      type: 'commercial_perpetual',
    }

    const key = await generateLicenseKey(licenseData, privateKeyHex)
    const [payloadBase64, signatureBase64] = key.split('.')

    // Decode payload and signature
    const payloadBytes = base64ToBytes(payloadBase64)
    const signatureBytes = base64ToBytes(signatureBase64)

    // Verify signature
    const isValid = await ed.verifyAsync(signatureBytes, payloadBytes, publicKey)
    expect(isValid).toBe(true)
  })

  it('rejects tampered payloads', async () => {
    const privateKey = ed.utils.randomSecretKey()
    const publicKey = await ed.getPublicKeyAsync(privateKey)
    const privateKeyHex = bytesToHex(privateKey)

    const licenseData: LicenseData = {
      email: 'original@test.com',
      transactionId: 'txn_original',
      issuedAt: '2026-01-08T12:00:00Z',
      type: 'commercial_subscription',
    }

    const key = await generateLicenseKey(licenseData, privateKeyHex)
    const [, signatureBase64] = key.split('.')

    // Create tampered payload
    const tamperedData: LicenseData = {
      email: 'hacker@evil.com',
      transactionId: 'txn_original',
      issuedAt: '2026-01-08T12:00:00Z',
      type: 'commercial_subscription',
    }
    const tamperedPayload = JSON.stringify(tamperedData)
    const tamperedPayloadBytes = new TextEncoder().encode(tamperedPayload)
    const signatureBytes = base64ToBytes(signatureBase64)

    // Signature should NOT verify for tampered payload
    const isValid = await ed.verifyAsync(signatureBytes, tamperedPayloadBytes, publicKey)
    expect(isValid).toBe(false)
  })

  it('includes organizationName in payload when provided', async () => {
    const privateKey = ed.utils.randomSecretKey()
    const privateKeyHex = bytesToHex(privateKey)

    const licenseData: LicenseData = {
      email: 'corp@example.com',
      transactionId: 'txn_corp123',
      issuedAt: '2026-01-08T12:00:00Z',
      type: 'commercial_subscription',
      organizationName: 'Acme Corporation',
    }

    const key = await generateLicenseKey(licenseData, privateKeyHex)
    const [payloadBase64] = key.split('.')
    const payloadJson = base64ToUtf8(payloadBase64)
    const decoded = JSON.parse(payloadJson) as LicenseData

    expect(decoded.organizationName).toBe('Acme Corporation')
    expect(decoded.email).toBe('corp@example.com')
    expect(decoded.type).toBe('commercial_subscription')
  })

  it('omits organizationName from payload when not provided', async () => {
    const privateKey = ed.utils.randomSecretKey()
    const privateKeyHex = bytesToHex(privateKey)

    const licenseData: LicenseData = {
      email: 'user@example.com',
      transactionId: 'txn_no_org',
      issuedAt: '2026-01-08T12:00:00Z',
      type: 'commercial_subscription',
      // organizationName intentionally omitted
    }

    const key = await generateLicenseKey(licenseData, privateKeyHex)
    const [payloadBase64] = key.split('.')
    const payloadJson = base64ToUtf8(payloadBase64)
    const decoded = JSON.parse(payloadJson) as LicenseData

    expect(decoded.organizationName).toBeUndefined()
    expect(decoded.type).toBe('commercial_subscription')
  })

  it('protects organizationName from tampering', async () => {
    const privateKey = ed.utils.randomSecretKey()
    const publicKey = await ed.getPublicKeyAsync(privateKey)
    const privateKeyHex = bytesToHex(privateKey)

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
    const signatureBytes = base64ToBytes(signatureBase64)

    // Signature should NOT verify for tampered org name
    const isValid = await ed.verifyAsync(signatureBytes, tamperedPayloadBytes, publicKey)
    expect(isValid).toBe(false)
  })

  it('signs a fixed end date into a dated license, so the app can enforce it offline', async () => {
    const privateKey = ed.utils.randomSecretKey()

    const key = await generateLicenseKey(
      {
        email: 'trial@example.com',
        transactionId: 'manual-ABCDEFGHJKMN',
        issuedAt: '2026-10-05T12:00:00Z',
        type: 'commercial_subscription',
        expiresAt: '2027-01-31T23:59:59.999Z',
      },
      bytesToHex(privateKey),
    )

    const decoded = JSON.parse(base64ToUtf8(key.split('.')[0])) as LicenseData
    expect(decoded.expiresAt).toBe('2027-01-31T23:59:59.999Z')
  })
})

/**
 * The `/validate` answer the app trusts. A perpetual license drops to Personal only on an
 * authenticated "invalid", so the answer is signed with the license key and bound to the app's
 * nonce: a squatter on a lapsed domain, or a TLS-intercepting proxy, can't forge one, and a captured
 * one can't be replayed into another request.
 */
describe('signValidationAnswer', () => {
  const answer = {
    transactionId: 'txn_abc',
    nonce: '0123456789abcdef0123456789abcdef',
    status: 'invalid' as const,
    type: null,
    organizationName: null,
    expiresAt: null,
  }

  it('signs the payload under a domain prefix, so the signature never verifies as a license key', async () => {
    const privateKey = ed.utils.randomSecretKey()
    const publicKey = await ed.getPublicKeyAsync(privateKey)

    const signed = await signValidationAnswer(answer, bytesToHex(privateKey), new Date('2026-10-05T12:00:00Z'))

    const payloadBytes = base64ToBytes(signed.payload)
    const signatureBytes = base64ToBytes(signed.signature)
    const prefixed = new Uint8Array([...new TextEncoder().encode(validationAnswerSignaturePrefix), ...payloadBytes])
    expect(await ed.verifyAsync(signatureBytes, prefixed, publicKey)).toBe(true)
    expect(await ed.verifyAsync(signatureBytes, payloadBytes, publicKey)).toBe(false)
  })

  it('carries the nonce, the transaction id, the verdict, and the server time', async () => {
    const privateKey = ed.utils.randomSecretKey()

    const signed = await signValidationAnswer(answer, bytesToHex(privateKey), new Date('2026-10-05T12:00:00Z'))

    expect(JSON.parse(base64ToUtf8(signed.payload))).toEqual({ ...answer, signedAt: '2026-10-05T12:00:00.000Z' })
  })
})

describe('isValidNonce', () => {
  it('takes 32 hex characters, the shape the app sends', () => {
    expect(isValidNonce('0123456789abcdef0123456789abcdef')).toBe(true)
  })

  it('refuses anything else, so a caller can’t make us sign arbitrary text', () => {
    expect(isValidNonce(undefined)).toBe(false)
    expect(isValidNonce('short')).toBe(false)
    expect(isValidNonce('0123456789abcdef0123456789abcdeg')).toBe(false)
    expect(isValidNonce('0123456789abcdef0123456789abcdef0')).toBe(false)
    expect(isValidNonce(42)).toBe(false)
  })
})
