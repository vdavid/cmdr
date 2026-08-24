import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { verifyAccessJwt, resetKeyCache, ACCESS_JWT_HEADER, ACCESS_JWT_COOKIE } from './access-jwt.js'

const ISSUER = 'https://getcmdr.cloudflareaccess.com'
const AUDIENCE = '5559a3ae99609154d1df6f95b4c4a946dfedd44a90d870bc5340237c1421e9d8'
const KID = 'test-key-1'

function b64url(bytes: Uint8Array | string): string {
  const bin = typeof bytes === 'string' ? bytes : String.fromCharCode(...bytes)
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

async function generateKeyPair() {
  return crypto.subtle.generateKey(
    {
      name: 'RSASSA-PKCS1-v1_5',
      modulusLength: 2048,
      publicExponent: new Uint8Array([1, 0, 1]),
      hash: 'SHA-256',
    },
    true,
    ['sign', 'verify'],
  )
}

async function toJwks(publicKey: CryptoKey, kid: string) {
  const jwk = await crypto.subtle.exportKey('jwk', publicKey)
  return { keys: [{ kty: 'RSA', alg: 'RS256', use: 'sig', kid, n: jwk.n, e: jwk.e }] }
}

/** Signs a JWT with the given header/payload overrides. `alg: 'none'` produces an unsigned token. */
async function makeToken(
  privateKey: CryptoKey,
  { header = {}, payload = {} }: { header?: Record<string, unknown>; payload?: Record<string, unknown> } = {},
): Promise<string> {
  const now = Math.floor(Date.now() / 1000)
  const fullHeader = { alg: 'RS256', kid: KID, typ: 'JWT', ...header }
  const fullPayload = {
    iss: ISSUER,
    aud: [AUDIENCE],
    exp: now + 3600,
    iat: now,
    email: 'veszelovszki@gmail.com',
    sub: 'user-123',
    ...payload,
  }
  const signingInput = `${b64url(JSON.stringify(fullHeader))}.${b64url(JSON.stringify(fullPayload))}`
  if (fullHeader.alg === 'none') return `${signingInput}.`
  const sig = await crypto.subtle.sign(
    { name: 'RSASSA-PKCS1-v1_5' },
    privateKey,
    new TextEncoder().encode(signingInput),
  )
  return `${signingInput}.${b64url(new Uint8Array(sig))}`
}

function requestWithHeader(token: string): Request {
  return new Request('https://cmdr-analytics-dashboard.pages.dev/product', {
    headers: { [ACCESS_JWT_HEADER]: token },
  })
}

describe('verifyAccessJwt', () => {
  let keys: CryptoKeyPair
  let fetchMock: ReturnType<typeof vi.fn>

  beforeEach(async () => {
    keys = await generateKeyPair()
    const jwks = await toJwks(keys.publicKey, KID)
    fetchMock = vi.fn(() => Promise.resolve(new Response(JSON.stringify(jwks), { status: 200 })))
    vi.stubGlobal('fetch', fetchMock)
    resetKeyCache()
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('accepts a token signed by a published Access key', async () => {
    const token = await makeToken(keys.privateKey)
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toEqual({
      kind: 'user',
      email: 'veszelovszki@gmail.com',
      sub: 'user-123',
    })
  })

  it('accepts the token from the CF_Authorization cookie', async () => {
    const token = await makeToken(keys.privateKey)
    const request = new Request('https://cmdr-analytics-dashboard.pages.dev/', {
      headers: { cookie: `other=x; ${ACCESS_JWT_COOKIE}=${token}; trailing=y` },
    })
    await expect(verifyAccessJwt(request)).resolves.toMatchObject({
      kind: 'user',
      email: 'veszelovszki@gmail.com',
    })
  })

  it('accepts a service token, the machine caller Access mints for `type: app`', async () => {
    // A real service-token payload: no `email`, an empty `sub`, and the token named by `common_name`.
    const token = await makeToken(keys.privateKey, {
      payload: {
        type: 'app',
        email: undefined,
        sub: '',
        common_name: 'f3e5a7332fc14564d58faf13d5ead798.access',
      },
    })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toEqual({
      kind: 'service',
      commonName: 'f3e5a7332fc14564d58faf13d5ead798.access',
    })
  })

  it('rejects a payload with neither an email nor a service-token identity', async () => {
    const token = await makeToken(keys.privateKey, { payload: { email: undefined } })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects a machine token that names no service token', async () => {
    const token = await makeToken(keys.privateKey, {
      payload: { type: 'app', email: undefined, sub: '', common_name: undefined },
    })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects a service token minted for a different Access application', async () => {
    const token = await makeToken(keys.privateKey, {
      payload: { type: 'app', email: undefined, common_name: 'other.access', aud: ['some-other-app-aud'] },
    })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects a self-minted service-token payload spliced onto a real signature', async () => {
    const token = await makeToken(keys.privateKey)
    const [header, , sig] = token.split('.')
    const forged = {
      iss: ISSUER,
      aud: [AUDIENCE],
      exp: Math.floor(Date.now() / 1000) + 3600,
      type: 'app',
      common_name: 'attacker.access',
    }
    await expect(
      verifyAccessJwt(requestWithHeader(`${header}.${b64url(JSON.stringify(forged))}.${sig}`)),
    ).resolves.toBeNull()
  })

  it('rejects an unsigned service token (alg: none)', async () => {
    const token = await makeToken(keys.privateKey, {
      header: { alg: 'none' },
      payload: { type: 'app', email: undefined, common_name: 'attacker.access' },
    })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects a request with no token at all', async () => {
    const request = new Request('https://cmdr-analytics-dashboard.pages.dev/api/report')
    await expect(verifyAccessJwt(request)).resolves.toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('rejects a token minted for a different Access application', async () => {
    const token = await makeToken(keys.privateKey, { payload: { aud: ['some-other-app-aud'] } })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects a token from a different team domain', async () => {
    const token = await makeToken(keys.privateKey, { payload: { iss: 'https://evil.cloudflareaccess.com' } })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects an expired token', async () => {
    const past = Math.floor(Date.now() / 1000) - 7200
    const token = await makeToken(keys.privateKey, { payload: { exp: past, iat: past - 3600 } })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects a token that is not valid yet', async () => {
    const token = await makeToken(keys.privateKey, { payload: { nbf: Math.floor(Date.now() / 1000) + 3600 } })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects a token whose payload was tampered with after signing', async () => {
    const token = await makeToken(keys.privateKey)
    const [header, , sig] = token.split('.')
    const forged = {
      iss: ISSUER,
      aud: [AUDIENCE],
      exp: Math.floor(Date.now() / 1000) + 3600,
      email: 'attacker@evil.com',
    }
    await expect(
      verifyAccessJwt(requestWithHeader(`${header}.${b64url(JSON.stringify(forged))}.${sig}`)),
    ).resolves.toBeNull()
  })

  it('rejects an unsigned token (alg: none)', async () => {
    const token = await makeToken(keys.privateKey, { header: { alg: 'none' } })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects an algorithm-confusion token (HS256 over the public key)', async () => {
    const jwk = await crypto.subtle.exportKey('jwk', keys.publicKey)
    const hmacKey = await crypto.subtle.importKey(
      'raw',
      new TextEncoder().encode(jwk.n ?? ''),
      { name: 'HMAC', hash: 'SHA-256' },
      false,
      ['sign'],
    )
    const now = Math.floor(Date.now() / 1000)
    const signingInput = `${b64url(JSON.stringify({ alg: 'HS256', kid: KID, typ: 'JWT' }))}.${b64url(
      JSON.stringify({ iss: ISSUER, aud: [AUDIENCE], exp: now + 3600, email: 'attacker@evil.com' }),
    )}`
    const sig = await crypto.subtle.sign('HMAC', hmacKey, new TextEncoder().encode(signingInput))
    await expect(
      verifyAccessJwt(requestWithHeader(`${signingInput}.${b64url(new Uint8Array(sig))}`)),
    ).resolves.toBeNull()
  })

  it('rejects a token signed by a key Cloudflare never published', async () => {
    const attacker = await generateKeyPair()
    const token = await makeToken(attacker.privateKey)
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects malformed tokens without throwing', async () => {
    for (const junk of ['', 'not-a-jwt', 'a.b', 'a.b.c.d', '...', '%%%.%%%.%%%']) {
      await expect(verifyAccessJwt(requestWithHeader(junk))).resolves.toBeNull()
    }
  })

  it('caches the key set across requests', async () => {
    const token = await makeToken(keys.privateKey)
    await verifyAccessJwt(requestWithHeader(token))
    await verifyAccessJwt(requestWithHeader(token))
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('refetches the key set once when it sees an unknown kid (key rotation)', async () => {
    const oldPair = await generateKeyPair()
    fetchMock
      .mockResolvedValueOnce(new Response(JSON.stringify(await toJwks(oldPair.publicKey, 'old-kid')), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(await toJwks(keys.publicKey, KID)), { status: 200 }))

    // Warm the cache with the pre-rotation key set.
    const oldToken = await makeToken(oldPair.privateKey, { header: { kid: 'old-kid' } })
    await expect(verifyAccessJwt(requestWithHeader(oldToken))).resolves.toMatchObject({
      kind: 'user',
      email: 'veszelovszki@gmail.com',
    })
    expect(fetchMock).toHaveBeenCalledTimes(1)

    // Cloudflare rotates: the unseen kid triggers exactly one refresh, and then verifies.
    await expect(verifyAccessJwt(requestWithHeader(await makeToken(keys.privateKey)))).resolves.toMatchObject({
      kind: 'user',
      email: 'veszelovszki@gmail.com',
    })
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('gives up after one refresh so an unknown kid cannot drive unbounded certs fetches', async () => {
    const attacker = await generateKeyPair()
    const token = await makeToken(attacker.privateKey, { header: { kid: 'kid-that-never-existed' } })
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
    expect(fetchMock).toHaveBeenCalledTimes(1)

    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('rejects rather than fails open when the certs endpoint is down', async () => {
    fetchMock.mockResolvedValue(new Response('upstream boom', { status: 500 }))
    const token = await makeToken(keys.privateKey)
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })

  it('rejects rather than fails open when the certs fetch throws', async () => {
    fetchMock.mockRejectedValue(new Error('network down'))
    const token = await makeToken(keys.privateKey)
    await expect(verifyAccessJwt(requestWithHeader(token))).resolves.toBeNull()
  })
})
