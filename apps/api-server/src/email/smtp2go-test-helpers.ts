/**
 * SMTP2Go at the socket, for the `createTestHarness` lanes (`email-runtime.test.ts`,
 * `../licensing/webhook-runtime.test.ts`, `../licensing/sale-notice-runtime.test.ts`). Each file's
 * `globalThis.fetch` stub hands SMTP2Go's URLs here, so the Worker's real `sendEmail` runs against
 * answers shaped like SMTP2Go's own, including the 200 that reports a failed send.
 */
import { SMTP2GO_API_BASE } from './send'

/** One message as it left the Worker, decoded from SMTP2Go's fields back into ours. */
export interface WireEmail {
  from: string
  to: string
  subject: string
  html: string
  text?: string
  replyTo?: string
}

interface Smtp2GoSendBody {
  sender: string
  to: string[]
  subject: string
  html_body: string
  text_body?: string
  custom_headers?: { header: string; value: string }[]
  fastaccept?: boolean
}

export interface Smtp2GoStub {
  sent: WireEmail[]
  /** How many times the daily probe read the email cycle. */
  cycleChecks: number
  /** Fail every send, the way SMTP2Go reports it: a 200 whose body says `failed: 1`. */
  failAll: boolean
  /** Fail sends to these recipients only, same shape. */
  failTo: Set<string>
  /** Set to hold a send open, which is the widest window a redelivery can land in. */
  held: { promise: Promise<void>; release: () => void } | null
  reset: () => void
  /** Whether `url` is SMTP2Go's, so the caller's stub routes it to {@link Smtp2GoStub.answer}. */
  handles: (url: string) => boolean
  answer: (url: string, input: RequestInfo | URL, init?: RequestInit) => Promise<Response>
}

/**
 * The body and headers of an outbound request. The harness hands it over as `fetch(url, request)`,
 * so both ride on the init rather than the input.
 *
 * ❌ No `instanceof Request` here: the init is undici's internal `Request` class, which is NOT the
 * `Request` this module's global resolves to, so the check silently reads false and the body comes
 * back empty. Duck-typing on `text()` is what survives that.
 */
export async function requestBody(input: RequestInfo | URL, init?: RequestInit): Promise<string> {
  for (const candidate of [init, input]) {
    const body = candidate as { text?: () => Promise<string> } | undefined
    if (typeof body?.text === 'function') return await body.text()
  }
  return typeof init?.body === 'string' ? init.body : ''
}

function apiKeyOf(input: RequestInfo | URL, init?: RequestInit): string | null {
  for (const candidate of [init, input]) {
    const headers = (candidate as { headers?: HeadersInit } | undefined)?.headers
    if (headers) return new Headers(headers).get('X-Smtp2go-Api-Key')
  }
  return null
}

function decode(body: Smtp2GoSendBody): WireEmail {
  // We only ever send to one person; a second address would mean the mapping changed shape.
  if (body.to.length !== 1) throw new Error(`Expected one recipient, got ${JSON.stringify(body.to)}`)
  // `fastaccept: true` answers before the send is judged, so a failure would read as success.
  if (body.fastaccept !== false) throw new Error('A send went out without `fastaccept: false`')
  const replyTo = body.custom_headers?.find((h) => h.header === 'Reply-To')?.value
  return {
    from: body.sender,
    to: body.to[0],
    subject: body.subject,
    html: body.html_body,
    ...(body.text_body !== undefined ? { text: body.text_body } : {}),
    ...(replyTo ? { replyTo } : {}),
  }
}

export function createSmtp2GoStub(): Smtp2GoStub {
  const stub: Smtp2GoStub = {
    sent: [],
    cycleChecks: 0,
    failAll: false,
    failTo: new Set(),
    held: null,
    reset: () => {
      stub.sent.length = 0
      stub.cycleChecks = 0
      stub.failAll = false
      stub.failTo.clear()
      stub.held = null
    },
    handles: (url) => url.startsWith(`${SMTP2GO_API_BASE}/`),
    answer: async (url, input, init) => {
      if (!apiKeyOf(input, init)) {
        return Response.json({ data: { error: 'No API key was sent' } }, { status: 401 })
      }
      if (url === `${SMTP2GO_API_BASE}/stats/email_cycle`) {
        stub.cycleChecks++
        return Response.json({ data: { cycle_used: 100, cycle_remaining: 900, cycle_max: 1000 } })
      }
      if (url !== `${SMTP2GO_API_BASE}/email/send`) {
        return Response.json({ data: { error: `Endpoint not stubbed: ${url}` } }, { status: 404 })
      }
      const email = decode(JSON.parse(await requestBody(input, init)) as Smtp2GoSendBody)
      stub.sent.push(email)
      if (stub.held) await stub.held.promise
      if (stub.failAll || stub.failTo.has(email.to)) {
        return Response.json({ data: { succeeded: 0, failed: 1, failures: ['Rate limit exceeded'], email_id: null } })
      }
      return Response.json({
        data: { succeeded: 1, failed: 0, failures: [], email_id: `email_${String(stub.sent.length)}` },
      })
    },
  }
  return stub
}
