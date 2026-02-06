/** XOR-accumulate comparison that always inspects every byte, preventing timing attacks. */
function constantTimeEqual(a: string, b: string): boolean {
    if (a.length !== b.length) return false
    let mismatch = 0
    for (let i = 0; i < a.length; i++) {
        mismatch |= a.charCodeAt(i) ^ b.charCodeAt(i)
    }
    return mismatch === 0
}

/**
 * Verify Paddle webhook signature.
 * See: https://developer.paddle.com/webhooks/signature-verification
 */
export async function verifyPaddleWebhook(body: string, signatureHeader: string, secret: string): Promise<boolean> {
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
