/**
 * Cloudflare Access JWT validation: the dashboard's own authentication gate.
 *
 * Access binds to a *hostname*, not to a Pages project, so the project's default
 * `cmdr-analytics-dashboard.pages.dev` alias serves the very same deployment with nothing in front
 * of it. Edge config alone therefore can't be the only gate. Every request here must carry an
 * Access JWT that we verify cryptographically; a request that didn't pass through Access has no
 * token and is refused, whichever hostname it arrived on.
 *
 * ❌ Never gate on the `cf-access-authenticated-user-email` header instead: any client can send it.
 * Only the signature over the JWT proves Access actually vouched for the caller.
 *
 * People and machines both arrive this way. A browser login carries a user token; a script sending
 * the Access service-token headers gets a `type: 'app'` token that has no `email` claim at all. Both
 * are signed by the same keys and checked identically; only the identity they map to differs.
 *
 * Both constants below are public (the audience tag appears in Access's own login-redirect URL), so
 * they live in code rather than in env vars: a missing env var would fail *open* on a deploy slip,
 * which is the one direction an auth gate must never fail.
 */

const TEAM_DOMAIN = 'getcmdr.cloudflareaccess.com'
const ISSUER = `https://${TEAM_DOMAIN}`
const CERTS_URL = `${ISSUER}/cdn-cgi/access/certs`

/** Audience tag of the "Cmdr analytics dashboard" Access application (`analdash.getcmdr.com`). */
const AUDIENCE = '5559a3ae99609154d1df6f95b4c4a946dfedd44a90d870bc5340237c1421e9d8'

/** Access sends the assertion in this header, and as a cookie on browser navigations. */
export const ACCESS_JWT_HEADER = 'cf-access-jwt-assertion'
export const ACCESS_JWT_COOKIE = 'CF_Authorization'

/** Tolerated clock skew between Cloudflare's signer and this Worker, in seconds. */
const CLOCK_SKEW_S = 60

/** How long a fetched key set is reused before a refresh, in milliseconds. */
const KEY_CACHE_TTL_MS = 60 * 60 * 1000

/** The only signature algorithm Access issues, and the only one we accept. */
const ALGORITHM = { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' } as const

/**
 * Who Access vouched for. A discriminated union rather than a bare email, because Access mints two
 * different kinds of token and a machine must never read as a person downstream: a login gives a
 * `email`-carrying user token, while a service token gives `type: 'app'` with an empty `sub`, no
 * `email` at all, and the token named by `common_name`.
 */
export type AccessIdentity = { kind: 'user'; email: string; sub: string } | { kind: 'service'; commonName: string }

interface CachedKeys {
  keys: Map<string, CryptoKey>
  fetchedAt: number
}

let cache: CachedKeys | null = null
let inFlight: Promise<CachedKeys | null> | null = null

/** Drops the cached Access key set. For tests; production relies on the TTL and kid-miss refresh. */
export function resetKeyCache(): void {
  cache = null
  inFlight = null
}

/** Backed by a plain `ArrayBuffer` so the result satisfies WebCrypto's `BufferSource`. */
function base64UrlToBytes(input: string): Uint8Array<ArrayBuffer> {
  const padded = input
    .replace(/-/g, '+')
    .replace(/_/g, '/')
    .padEnd(Math.ceil(input.length / 4) * 4, '=')
  const binary = atob(padded)
  const bytes = new Uint8Array(new ArrayBuffer(binary.length))
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return bytes
}

function decodeJsonSegment(segment: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(new TextDecoder().decode(base64UrlToBytes(segment)))
    return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : null
  } catch {
    return null
  }
}

/**
 * Fetches and imports Cloudflare's published signing keys. Concurrent callers share one request, and
 * any failure returns `null` so the caller rejects the request rather than falling back to a
 * trust-everything path.
 */
async function fetchKeys(): Promise<CachedKeys | null> {
  if (inFlight) return inFlight

  inFlight = (async (): Promise<CachedKeys | null> => {
    try {
      const response = await fetch(CERTS_URL)
      if (!response.ok) {
        console.error(`Access certs fetch returned ${String(response.status)}`)
        return null
      }
      const body = (await response.json()) as { keys?: Array<Record<string, unknown>> }
      const keys = new Map<string, CryptoKey>()
      for (const jwk of body.keys ?? []) {
        // Import only RS256 signing keys; anything else can't have signed a token we'd accept.
        if (jwk.kty !== 'RSA' || jwk.alg !== 'RS256' || typeof jwk.kid !== 'string') continue
        try {
          const key = await crypto.subtle.importKey(
            'jwk',
            { kty: 'RSA', alg: 'RS256', use: 'sig', n: String(jwk.n), e: String(jwk.e), ext: true },
            ALGORITHM,
            false,
            ['verify'],
          )
          keys.set(jwk.kid, key)
        } catch (e) {
          console.error(`Skipping unimportable Access key ${jwk.kid}:`, e)
        }
      }
      if (keys.size === 0) return null
      const fresh = { keys, fetchedAt: Date.now() }
      cache = fresh
      return fresh
    } catch (e) {
      console.error('Access certs fetch failed:', e)
      return null
    } finally {
      inFlight = null
    }
  })()

  return inFlight
}

/**
 * Resolves the signing key for `kid`, refreshing the cached set once on a miss so a key rotation
 * doesn't lock everyone out until the TTL expires.
 */
async function resolveKey(kid: string): Promise<CryptoKey | null> {
  const cached = cache && Date.now() - cache.fetchedAt < KEY_CACHE_TTL_MS ? cache : null
  const hit = cached?.keys.get(kid)
  if (hit) return hit

  // Either nothing usable is cached, or the cached set predates a rotation. One refresh, then give
  // up: an unknown kid must not let a caller drive unbounded fetches to the certs endpoint.
  const fetched = await fetchKeys()
  return fetched?.keys.get(kid) ?? null
}

function readToken(request: Request): string | null {
  const header = request.headers.get(ACCESS_JWT_HEADER)
  if (header) return header.trim()

  const cookies = request.headers.get('cookie')
  if (!cookies) return null
  for (const part of cookies.split(';')) {
    const [name, ...rest] = part.trim().split('=')
    if (name === ACCESS_JWT_COOKIE && rest.length > 0) return rest.join('=').trim()
  }
  return null
}

function audienceMatches(aud: unknown): boolean {
  if (typeof aud === 'string') return aud === AUDIENCE
  if (Array.isArray(aud)) return aud.some((a) => a === AUDIENCE)
  return false
}

/**
 * Checks the registered claims on an already signature-verified payload and returns the caller's
 * identity, or `null` to reject. Only reached once the signature checks out.
 *
 * The two token kinds are told apart by `type`, before either is asked for its own claims: a
 * machine token is a machine token whatever else it carries, so `type: 'app'` can never come back
 * as a person. Each branch then needs its own identifying claim (`common_name` for a service token,
 * `email` for a user), and a payload that has neither is refused: this is the mapping step, and it
 * is the last place a payload can still be rejected.
 */
function identityFromPayload(payload: Record<string, unknown>): AccessIdentity | null {
  if (payload.iss !== ISSUER) return null
  if (!audienceMatches(payload.aud)) return null

  const now = Math.floor(Date.now() / 1000)
  if (typeof payload.exp !== 'number' || payload.exp + CLOCK_SKEW_S < now) return null
  if (typeof payload.nbf === 'number' && payload.nbf - CLOCK_SKEW_S > now) return null

  if (payload.type === 'app') {
    const commonName = typeof payload.common_name === 'string' ? payload.common_name.trim() : ''
    return commonName ? { kind: 'service', commonName } : null
  }

  const email = typeof payload.email === 'string' ? payload.email.trim() : ''
  if (!email) return null
  const sub = typeof payload.sub === 'string' ? payload.sub : ''

  return { kind: 'user', email, sub }
}

/**
 * Verifies the Access JWT on `request` and returns the caller's identity, or `null` if the request
 * isn't provably from an Access-authenticated user. Never throws: every failure path is a rejection.
 */
export async function verifyAccessJwt(request: Request): Promise<AccessIdentity | null> {
  const token = readToken(request)
  if (!token) return null

  const parts = token.split('.')
  if (parts.length !== 3) return null
  const [headerSegment, payloadSegment, signatureSegment] = parts

  const header = decodeJsonSegment(headerSegment)
  // Pin the algorithm before touching the signature: this is what kills `alg: none` and the
  // HS256-over-the-public-key confusion attack.
  if (!header || header.alg !== 'RS256' || typeof header.kid !== 'string') return null

  const key = await resolveKey(header.kid)
  if (!key) return null

  let verified: boolean
  try {
    verified = await crypto.subtle.verify(
      ALGORITHM,
      key,
      base64UrlToBytes(signatureSegment),
      new TextEncoder().encode(`${headerSegment}.${payloadSegment}`),
    )
  } catch {
    return null
  }
  if (!verified) return null

  const payload = decodeJsonSegment(payloadSegment)
  if (!payload) return null

  return identityFromPayload(payload)
}
