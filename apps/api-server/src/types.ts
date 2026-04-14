export type Bindings = {
  // KV namespace for license code -> full key mappings
  LICENSE_CODES: KVNamespace
  // KV namespace for blog post likes
  BLOG_LIKES: KVNamespace
  // Analytics Engine for device count tracking (fair use monitoring)
  DEVICE_COUNTS: AnalyticsEngineDataset
  // D1 database for telemetry persistence (crash reports, downloads, update checks)
  TELEMETRY_DB: D1Database
  // Paddle webhook secrets (both optional to support gradual rollout)
  PADDLE_WEBHOOK_SECRET_LIVE?: string
  PADDLE_WEBHOOK_SECRET_SANDBOX?: string
  // Paddle API keys for validation
  PADDLE_API_KEY_LIVE?: string
  PADDLE_API_KEY_SANDBOX?: string
  // Crypto keys
  ED25519_PRIVATE_KEY: string
  // Email
  RESEND_API_KEY: string
  // Config
  PRODUCT_NAME: string
  SUPPORT_EMAIL: string
  // "sandbox" (default) or "live" — controls which Paddle API to use for /validate
  PADDLE_ENVIRONMENT?: string
  // Price IDs for license type mapping
  PRICE_ID_COMMERCIAL_SUBSCRIPTION?: string
  PRICE_ID_COMMERCIAL_PERPETUAL?: string
  // Dedicated admin API token for /admin/stats (separate from Paddle secrets)
  ADMIN_API_TOKEN?: string
  // Crash notification email recipient (for cron-based crash alerts)
  CRASH_NOTIFICATION_EMAIL?: string
}

export interface PaddleWebhookPayload {
  event_type: string
  data?: {
    id?: string
    customer_id?: string
    items?: Array<{
      price?: {
        id?: string
      }
      quantity?: number
    }>
    custom_data?: {
      // Paddle preserves the key casing from checkout - we use camelCase
      organizationName?: string
    }
  }
}

export const maxOrganizationNameLength = 500

// KV key for the activation counter, read by /admin/stats.
// Starts from zero on deploy — initialize via the CF API if you need historical count.
export const activationCountKey = '_meta:activation_count'
export const maxTransactionIdLength = 200

export function isValidEmail(email: string): boolean {
  const atIndex = email.indexOf('@')
  return atIndex > 0 && email.indexOf('.', atIndex) > atIndex + 1
}

export function isValidLicenseType(type: string): type is LicenseType {
  return (licenseTypes as readonly string[]).includes(type)
}

/** Redact an email for logging: "john@example.com" -> "j***@example.com" */
export function redactEmail(email: string): string {
  const atIndex = email.indexOf('@')
  if (atIndex <= 0) return '***'
  return email[0] + '***' + email.slice(atIndex)
}

/** Determine Paddle API config from PADDLE_ENVIRONMENT var (default: sandbox). */
export function getPaddleConfig(env: Bindings): { apiKey: string; environment: 'sandbox' | 'live' } | null {
  const environment = env.PADDLE_ENVIRONMENT === 'live' ? 'live' : 'sandbox'
  const apiKey = environment === 'live' ? env.PADDLE_API_KEY_LIVE : env.PADDLE_API_KEY_SANDBOX
  if (!apiKey) return null
  return { apiKey, environment }
}

/** Verify admin auth and return error response if unauthorized, or null if authorized. */
export function verifyAdminAuth(c: {
  env: Bindings
  req: { header: (name: string) => string | undefined }
}): Response | null {
  const token = c.env.ADMIN_API_TOKEN
  if (!token) {
    return Response.json({ error: 'Admin API not configured' }, { status: 500 })
  }
  const authHeader = c.req.header('Authorization')
  if (!authHeader || !constantTimeEqual(authHeader, `Bearer ${token}`)) {
    return Response.json({ error: 'Unauthorized' }, { status: 401 })
  }
  return null
}

import { licenseTypes, type LicenseType } from './license'
import { constantTimeEqual } from './paddle'
