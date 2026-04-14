import { Hono } from 'hono'
import type { Bindings } from './types'

const telemetry = new Hono<{ Bindings: Bindings }>()

// Crash report ingestion — writes to D1 for crash analysis
const maxCrashReportBytes = 64 * 1024
const crashReportRequiredFields = ['appVersion', 'osVersion', 'arch', 'signal'] as const
const maxBacktraceBytes = 5_000

interface CrashReport {
  appVersion: string
  osVersion: string
  arch: string
  signal: string
  backtraceFrames?: string[]
  [key: string]: unknown
}

/** Extract the first app-code frame from a backtrace (contains `cmdr` or `cmdr_lib`). */
function extractTopFunction(frames: string[] | undefined): string {
  if (!frames || !Array.isArray(frames)) return 'unknown'
  for (const frame of frames) {
    if (typeof frame === 'string' && (frame.includes('cmdr') || frame.includes('cmdr_lib'))) {
      return frame
    }
  }
  return 'unknown'
}

telemetry.post('/crash-report', async (c) => {
  // Reject oversized payloads before parsing
  const contentLength = c.req.header('content-length')
  if (contentLength && parseInt(contentLength, 10) > maxCrashReportBytes) {
    return c.json({ error: 'Report too large' }, 400)
  }

  let rawBody: string
  try {
    rawBody = await c.req.text()
  } catch {
    return c.json({ error: 'Could not read request body' }, 400)
  }

  if (rawBody.length > maxCrashReportBytes) {
    return c.json({ error: 'Report too large' }, 400)
  }

  let report: CrashReport
  try {
    report = JSON.parse(rawBody) as CrashReport
  } catch {
    return c.json({ error: 'Invalid JSON' }, 400)
  }

  // Validate required fields
  for (const field of crashReportRequiredFields) {
    if (typeof report[field] !== 'string' || report[field].length === 0) {
      return c.json({ error: `Missing required field: ${field}` }, 400)
    }
  }

  // Hash IP with daily salt for deduplication (same pattern as update-check)
  const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
  const dailySalt = new Date().toISOString().slice(0, 10) // YYYY-MM-DD
  const hashBuffer = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(ip + dailySalt))
  const hashedIp = [...new Uint8Array(hashBuffer)].map((b) => b.toString(16).padStart(2, '0')).join('')

  const topFunction = extractTopFunction(report.backtraceFrames)
  const backtraceTruncated = JSON.stringify(report.backtraceFrames ?? []).slice(0, maxBacktraceBytes)

  // Write to D1 (fire-and-forget)
  const dbWrite = c.env.TELEMETRY_DB.prepare(
    `INSERT INTO crash_reports (hashed_ip, app_version, os_version, arch, signal, top_function, backtrace)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
  )
    .bind(hashedIp, report.appVersion, report.osVersion, report.arch, report.signal, topFunction, backtraceTruncated)
    .run()
    .catch(() => {}) // Don't let D1 failure block the response

  try {
    c.executionCtx.waitUntil(dbWrite)
  } catch {
    // executionCtx unavailable (for example, in tests) — await inline as fallback
    await dbWrite
  }

  return c.body(null, 204)
})

const versionPattern = /^\d+\.\d+\.\d+$/

// Update check proxy — tracks version and arch for active user counting, then redirects to latest.json
telemetry.get('/update-check/:version', async (c) => {
  const { version } = c.req.param()

  if (!versionPattern.test(version)) {
    return c.json({ error: 'Invalid version' }, 400)
  }

  const arch = c.req.query('arch') ?? 'unknown'

  // Hash IP with daily salt for deduplication without storing PII
  const ip = c.req.header('cf-connecting-ip') ?? c.req.header('x-forwarded-for') ?? 'unknown'
  const dailySalt = new Date().toISOString().slice(0, 10) // YYYY-MM-DD
  const hashBuffer = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(ip + dailySalt))
  const hashedIp = [...new Uint8Array(hashBuffer)].map((b) => b.toString(16).padStart(2, '0')).join('')

  // Write to D1 (fire-and-forget). INSERT OR IGNORE deduplicates via UNIQUE constraint.
  const dbWrite = c.env.TELEMETRY_DB.prepare(
    `INSERT OR IGNORE INTO update_checks (date, hashed_ip, app_version, arch) VALUES (?, ?, ?, ?)`,
  )
    .bind(dailySalt, hashedIp, version, arch)
    .run()
    .catch(() => {})

  try {
    c.executionCtx.waitUntil(dbWrite)
  } catch {
    // executionCtx unavailable (for example, in tests) — await inline as fallback
    await dbWrite
  }

  return c.redirect('https://getcmdr.com/latest.json', 302)
})

// Download redirect — tracks version, arch, and country, then redirects to GitHub Releases
const validArchitectures = new Set(['aarch64', 'x86_64', 'universal'])

telemetry.get('/download/:version/:arch', async (c) => {
  const { version, arch } = c.req.param()

  if (!versionPattern.test(version) || !validArchitectures.has(arch)) {
    return c.json({ error: 'Invalid version or architecture' }, 400)
  }

  const cf = c.req.raw.cf as { country?: string; continent?: string } | undefined
  const country = cf?.country ?? 'unknown'
  const continent = cf?.continent ?? 'unknown'

  // Write to D1 (fire-and-forget)
  const dbWrite = c.env.TELEMETRY_DB.prepare(
    `INSERT INTO downloads (app_version, arch, country, continent) VALUES (?, ?, ?, ?)`,
  )
    .bind(version, arch, country, continent)
    .run()
    .catch(() => {})

  try {
    c.executionCtx.waitUntil(dbWrite)
  } catch {
    // executionCtx unavailable (for example, in tests) — await inline as fallback
    await dbWrite
  }

  return c.redirect(`https://github.com/vdavid/cmdr/releases/download/v${version}/Cmdr_${version}_${arch}.dmg`, 302)
})

export { telemetry }
