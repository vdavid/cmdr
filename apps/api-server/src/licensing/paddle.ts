/** XOR-accumulate comparison that always inspects every byte, preventing timing attacks. */
export function constantTimeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false
  let mismatch = 0
  for (let i = 0; i < a.length; i++) {
    mismatch |= a.charCodeAt(i) ^ b.charCodeAt(i)
  }
  return mismatch === 0
}

/**
 * How far a webhook's signed `ts` may sit from now, either way, in seconds. Paddle's SDKs default to
 * five seconds; this allows for clock skew and a slow hop while still killing the replay of a
 * captured webhook. Paddle stamps `ts` when it SENDS, retries included (their SDKs enforce the window
 * on every delivery of a three-day retry schedule).
 */
export const PADDLE_TIMESTAMP_TOLERANCE_SECONDS = 300

/**
 * Verify Paddle webhook signature, and that it was sent within the tolerance of `nowMs`.
 * See: https://developer.paddle.com/webhooks/signature-verification
 */
export async function verifyPaddleWebhook(
  body: string,
  signatureHeader: string,
  secret: string,
  nowMs: number = Date.now(),
): Promise<boolean> {
  if (!signatureHeader) return false

  // Parse signature header: ts=123;h1=abc
  const parts = signatureHeader.split(';').reduce<Record<string, string>>((acc, part) => {
    const [key, value] = part.split('=')
    if (key && value) acc[key] = value
    return acc
  }, {})

  const timestamp = parts['ts']
  const signature = parts['h1']

  if (!timestamp || !signature) return false

  // The timestamp is inside the signed payload, so a replay can't freshen it: refuse one sent too
  // long ago (or claiming the future) before spending the HMAC.
  const sentAtSeconds = Number(timestamp)
  if (!Number.isFinite(sentAtSeconds)) return false
  if (Math.abs(nowMs / 1000 - sentAtSeconds) > PADDLE_TIMESTAMP_TOLERANCE_SECONDS) return false

  // Build signed payload: timestamp:body
  const signedPayload = `${timestamp}:${body}`

  // Compute HMAC-SHA256
  const encoder = new TextEncoder()
  const key = await crypto.subtle.importKey('raw', encoder.encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, [
    'sign',
  ])

  const signatureBytes = await crypto.subtle.sign('HMAC', key, encoder.encode(signedPayload))

  const expectedSignature = Array.from(new Uint8Array(signatureBytes))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')

  return constantTimeEqual(signature, expectedSignature)
}

/**
 * Verify webhook against multiple secrets (live + sandbox).
 * Returns true if any secret matches.
 */
export async function verifyPaddleWebhookMulti(
  body: string,
  signatureHeader: string,
  secrets: (string | undefined)[],
): Promise<boolean> {
  for (const secret of secrets) {
    if (secret && (await verifyPaddleWebhook(body, signatureHeader, secret))) {
      return true
    }
  }
  return false
}
