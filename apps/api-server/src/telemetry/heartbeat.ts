import { Hono, type Context } from 'hono'
import { enforceIpRateLimit, readCappedBody, type Bindings } from '../types'
import { versionPattern } from './telemetry'

const heartbeat = new Hono<{ Bindings: Bindings }>()

// Heartbeat ingestion: one row per beat for true daily-active tracking, plus the feature events the
// app accumulated since its last successful beat. Identity is the random `anal_<uuid>` analytics id;
// no IP is stored (the id is the dedup key). The whole request body is capped, and the config blob
// is capped again on its own so a single fat config can't dominate the budget. The body cap is sized
// for a full batch of events: 500 events at a few hundred bytes each, plus the config.
const maxHeartbeatBytes = 256 * 1024
const maxConfigJsonBytes = 16 * 1024
/** Events stored per beat. The client never sends more; past this the server keeps the first ones. */
const maxEventsPerBeat = 500
/** A beat can't account for more runtime than this. Anything longer is a client bug, clamped rather than stored. */
const maxUptimeSeconds = 7 * 86_400
const heartbeatRequiredFields = ['analId', 'appVersion', 'osVersion', 'arch'] as const
// `anal_` + a v4 UUID (36 chars: 32 hex digits plus the 4 dashes).
const analIdPattern = /^anal_[0-9a-f-]{36}$/
// What the app's event names look like (`search_used`), plus `$` for PostHog's reserved names.
const eventNamePattern = /^[a-z0-9_$]{1,100}$/
// The client's per-event id: a lowercase hyphenated v4 UUID.
const eventIdPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/
// RFC 3339 date-time: a `Z` or a numeric offset is required, so a zone-less time can't be misread.
const rfc3339Pattern = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/
/** No Cmdr build predates this, so an earlier event timestamp is a broken clock. */
const earliestEventMs = Date.parse('2025-01-01T00:00:00Z')
/** How far ahead of our clock an event may claim to be, for clock skew. Past this it's a broken clock. */
const maxEventClockSkewMs = 86_400_000

interface Heartbeat {
  analId: string
  appVersion: string
  osVersion: string
  arch: string
  /** Optional. `"release"` or `"debug"`; older clients don't set it (stored as NULL). */
  buildMode?: 'release' | 'debug' | null
  /** Optional. The allowlisted config-shape snapshot, stored verbatim as `config_json`. */
  config?: Record<string, unknown> | null
  /** Optional. Runtime this beat accounts for; older clients don't set it (stored as NULL). */
  uptimeSeconds?: number | null
  /** Optional. Feature events since the last successful beat; validated item by item. */
  events?: unknown[] | null
  /** Optional. `"homebrew"` or `"direct"`, how the app was installed. Anything else is stored as NULL. */
  installChannel?: unknown
}

/** The per-install facts one beat contributes, each NULL when this beat doesn't know it. */
export interface BeatInstallFacts {
  country: string | null
  installChannel: 'homebrew' | 'direct' | null
}

/** One feature event that passed validation, with its timestamp normalized to UTC. */
interface RelayedEvent {
  event: string
  timestamp: string
  properties: Record<string, unknown>
  /** The build that produced the event: its own `appVersion` when valid, else the beat's. */
  appVersion: string
  /** The client's per-event UUID, or null when it sent none (or a malformed one). */
  id: string | null
}

/**
 * `created_at` is bound, never left to the column default: the same value lands in
 * `install_summary.first_beat_at` on an install's first beat, so the two can't straddle a midnight.
 */
const insertHeartbeatSql = `INSERT INTO heartbeat (anal_id, app_version, os_version, arch, build_mode, config_json, uptime_seconds, country, install_channel, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`

/**
 * Keeps `install_summary` in step with the beat, in the beat's own batch. The first beat inserts the
 * row; a later one only fills a still-NULL country or channel (the first non-NULL value wins). The
 * `WHERE` makes a beat that changes nothing a no-op rather than a row write, which is almost every
 * beat. `src/admin/DETAILS.md` § Install summary.
 */
const upsertInstallSummarySql = `INSERT INTO install_summary (anal_id, first_beat_at, country, install_channel)
     VALUES (?1, ?2, ?3, ?4)
     ON CONFLICT(anal_id) DO UPDATE SET
       country = COALESCE(install_summary.country, excluded.country),
       install_channel = COALESCE(install_summary.install_channel, excluded.install_channel)
     WHERE (install_summary.country IS NULL AND excluded.country IS NOT NULL)
        OR (install_summary.install_channel IS NULL AND excluded.install_channel IS NOT NULL)`

/** Cloudflare's codes that aren't countries: `XX` (no country) and `T1` (Tor). */
const nonCountryCodes = new Set(['XX', 'T1'])
const countryPattern = /^[A-Z]{2}$/

/**
 * Inserts a whole beat's events in ONE statement: they travel as a single JSON parameter
 * (`[[event, occurredAt, propertiesJson, appVersion, id], ...]`) and `json_each` unpacks them. One
 * statement per event would put up to 500 statements in the batch, against D1's per-invocation query
 * limit, and a multi-row VALUES list would hit its 100-bound-parameter cap at 16 events. `OR IGNORE`
 * skips an event whose `event_id` is already stored: a retried beat whose first 204 got lost.
 */
const insertEventsSql = `INSERT OR IGNORE INTO analytics_event (anal_id, event, occurred_at, properties_json, app_version, event_id)
     SELECT ?1, json_extract(value, '$[0]'), json_extract(value, '$[1]'), json_extract(value, '$[2]'),
            json_extract(value, '$[3]'), json_extract(value, '$[4]')
     FROM json_each(?2)`

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/**
 * Validate the optional `buildMode` field. `null` and `undefined` both mean "absent": Rust
 * serializes `Option::None` as `null` (specta's unified mode rejects `skip_serializing_if`), so
 * tolerate both rather than rejecting upgrade-window beats.
 */
function validateBuildMode(buildMode: unknown): string | null {
  if (buildMode !== undefined && buildMode !== null && buildMode !== 'release' && buildMode !== 'debug') {
    return 'Invalid buildMode'
  }
  return null
}

/** Validate the optional `config` blob: must be a plain object, capped at `maxConfigJsonBytes`. */
function validateConfig(config: unknown): string | null {
  if (config === undefined || config === null) return null
  if (!isPlainObject(config)) return 'Invalid config'
  if (JSON.stringify(config).length > maxConfigJsonBytes) return 'Config too large'
  return null
}

/** Validate the optional `uptimeSeconds`: a non-negative integer. Too large is clamped later, not rejected. */
function validateUptime(uptime: unknown): string | null {
  if (uptime === undefined || uptime === null) return null
  if (typeof uptime !== 'number' || !Number.isInteger(uptime) || uptime < 0) return 'Invalid uptimeSeconds'
  return null
}

/**
 * Validate the optional `events` container. Only the container can fail the beat; a bad ITEM is
 * dropped by `parseEvents` instead, because one malformed event from any call site would otherwise
 * fail every beat that carries it, and the client would resend it forever.
 */
function validateEvents(events: unknown): string | null {
  if (events === undefined || events === null) return null
  if (!Array.isArray(events)) return 'Invalid events'
  return null
}

/**
 * Validate the runtime shape of a `POST /heartbeat` body. Returns `null` if the body is
 * well-formed; otherwise the error message to surface as 400. Input is typed as
 * `Record<string, unknown>` (not `Heartbeat`) so the optional-field checks aren't statically
 * narrowed away: values arrive from `JSON.parse` and can be any shape a client sends.
 */
function validateHeartbeatShape(beat: Record<string, unknown>): string | null {
  for (const field of heartbeatRequiredFields) {
    const value = beat[field]
    if (typeof value !== 'string' || value.length === 0) {
      return `Missing required field: ${field}`
    }
  }
  if (!analIdPattern.test(beat.analId as string)) return 'Invalid analId'
  if (!versionPattern.test(beat.appVersion as string)) return 'Invalid appVersion'
  return (
    validateBuildMode(beat.buildMode) ??
    validateConfig(beat.config) ??
    validateUptime(beat.uptimeSeconds) ??
    validateEvents(beat.events)
  )
}

/** A string matching `pattern`, or null for anything else. For the optional fields a bad value only unsets. */
function matching(value: unknown, pattern: RegExp): string | null {
  return typeof value === 'string' && pattern.test(value) ? value : null
}

/** The event's timestamp as UTC ISO, or null when it's malformed or outside what a working clock says. */
function parseOccurredAt(timestamp: unknown, nowMs: number): string | null {
  const text = matching(timestamp, rfc3339Pattern)
  if (text === null) return null
  const occurredMs = Date.parse(text)
  if (Number.isNaN(occurredMs) || occurredMs < earliestEventMs || occurredMs > nowMs + maxEventClockSkewMs) return null
  return new Date(occurredMs).toISOString()
}

/**
 * One event, validated and normalized, or null to drop it. `properties` may be absent (it means `{}`).
 * A bad `appVersion` or `id` doesn't drop the event, since both are extras: the version falls back to
 * the beat's, and the id to none.
 */
function parseEvent(raw: unknown, beatAppVersion: string, nowMs: number): RelayedEvent | null {
  if (!isPlainObject(raw)) return null
  const event = matching(raw.event, eventNamePattern)
  const timestamp = parseOccurredAt(raw.timestamp, nowMs)
  const properties = raw.properties ?? {}
  if (event === null || timestamp === null || !isPlainObject(properties)) return null
  return {
    event,
    timestamp,
    properties,
    appVersion: matching(raw.appVersion, versionPattern) ?? beatAppVersion,
    id: matching(raw.id, eventIdPattern),
  }
}

/**
 * Keep the first `maxEventsPerBeat` events and the valid ones among them. Both kinds of drop are
 * logged with a count: past the cap means a client ignored its own limit, and an invalid item means
 * a call site in the app sends something the contract doesn't allow.
 */
function parseEvents(raw: unknown[] | null | undefined, beatAppVersion: string): RelayedEvent[] {
  if (!raw) return []
  const nowMs = Date.now()
  const overCap = Math.max(0, raw.length - maxEventsPerBeat)
  const kept: RelayedEvent[] = []
  for (const item of raw.slice(0, maxEventsPerBeat)) {
    const parsed = parseEvent(item, beatAppVersion, nowMs)
    if (parsed) kept.push(parsed)
  }
  const invalid = Math.min(raw.length, maxEventsPerBeat) - kept.length
  if (overCap > 0) {
    console.warn(`Heartbeat: dropped ${String(overCap)} events past the ${String(maxEventsPerBeat)}-event cap`)
  }
  if (invalid > 0) {
    console.warn(`Heartbeat: dropped ${String(invalid)} malformed events (bad name, timestamp, or properties)`)
  }
  return kept
}

/** The request's ISO-2 country as Cloudflare derived it, or null when absent or not a country. */
export function parseCountry(cfCountry: unknown): string | null {
  if (typeof cfCountry !== 'string' || !countryPattern.test(cfCountry) || nonCountryCodes.has(cfCountry)) return null
  return cfCountry
}

/**
 * `"homebrew"` or `"direct"`, or null for absent and for any value this server doesn't know yet. An
 * unknown value only unsets the field: it's an enrichment, and failing the beat would make the client
 * resend it forever.
 */
export function parseInstallChannel(value: unknown): 'homebrew' | 'direct' | null {
  return value === 'homebrew' || value === 'direct' ? value : null
}

/** `YYYY-MM-DD HH:MM:SS`, the format `datetime('now')` writes, so bound and defaulted rows compare alike. */
function sqliteTimestamp(now: Date): string {
  return now.toISOString().slice(0, 19).replace('T', ' ')
}

/** Read and parse the request body, enforcing the size cap. Returns the parsed object or an error. */
async function readHeartbeatBody(c: Context<{ Bindings: Bindings }>): Promise<Record<string, unknown> | Response> {
  // A cheap fast-fail for an honest client; `readCappedBody` is the actual cap.
  const contentLength = c.req.header('content-length')
  if (contentLength && parseInt(contentLength, 10) > maxHeartbeatBytes) {
    return c.json({ error: 'Heartbeat too large' }, 400)
  }

  const body = c.req.raw.body
  if (!body) return c.json({ error: 'Invalid JSON' }, 400)
  let bytes: ArrayBuffer | null
  try {
    bytes = await readCappedBody(body, maxHeartbeatBytes)
  } catch {
    return c.json({ error: 'Could not read request body' }, 400)
  }
  if (!bytes) {
    return c.json({ error: 'Heartbeat too large' }, 400)
  }

  let parsed: unknown
  try {
    parsed = JSON.parse(new TextDecoder().decode(bytes))
  } catch {
    return c.json({ error: 'Invalid JSON' }, 400)
  }
  if (!parsed || typeof parsed !== 'object') {
    return c.json({ error: 'Invalid JSON' }, 400)
  }
  return parsed as Record<string, unknown>
}

heartbeat.post('/heartbeat', async (c) => {
  // Rate-limit by the caller IP before doing any work. The IP keys the limiter's sliding window
  // only and is never stored.
  const limited = await enforceIpRateLimit(c.env.HEARTBEAT_LIMITER, c.req)
  if (limited) return limited

  const parsed = await readHeartbeatBody(c)
  if (parsed instanceof Response) return parsed

  const validationError = validateHeartbeatShape(parsed)
  if (validationError) {
    return c.json({ error: validationError }, 400)
  }
  const beat = parsed as unknown as Heartbeat
  const events = parseEvents(beat.events, beat.appVersion)
  const config = beat.config ?? null

  // The config blob is stored verbatim as a single JSON column (not per-field columns), so new
  // settings auto-absorb without a migration. We re-serialize to a canonical string for storage.
  const configJson = config ? JSON.stringify(config) : null
  const uptimeSeconds =
    beat.uptimeSeconds !== undefined && beat.uptimeSeconds !== null
      ? Math.min(beat.uptimeSeconds, maxUptimeSeconds)
      : null

  // The country comes from Cloudflare's view of the request, never from the IP itself.
  const createdAt = sqliteTimestamp(new Date())
  const facts: BeatInstallFacts = {
    country: parseCountry((c.req.raw.cf as { country?: unknown } | undefined)?.country),
    installChannel: parseInstallChannel(beat.installChannel),
  }

  const db = c.env.TELEMETRY_DB
  const statements = [
    db
      .prepare(insertHeartbeatSql)
      .bind(
        beat.analId,
        beat.appVersion,
        beat.osVersion,
        beat.arch,
        beat.buildMode ?? null,
        configJson,
        uptimeSeconds,
        facts.country,
        facts.installChannel,
        createdAt,
      ),
    db.prepare(upsertInstallSummarySql).bind(beat.analId, createdAt, facts.country, facts.installChannel),
  ]
  if (events.length > 0) {
    const rows = events.map((e) => [e.event, e.timestamp, JSON.stringify(e.properties), e.appVersion, e.id])
    statements.push(db.prepare(insertEventsSql).bind(beat.analId, JSON.stringify(rows)))
  }

  // AWAITED, and one batch (D1 runs a batch as a transaction): a 2xx tells the client both the beat
  // and its events are stored, which is what lets it clear them from its spool. A failure answers a
  // soft 502 so the client keeps them for the next try.
  try {
    await db.batch(statements)
  } catch (e) {
    console.error('Heartbeat: D1 write failed', e)
    return c.json({ error: 'Could not store the heartbeat right now' }, 502)
  }

  return c.body(null, 204)
})

export { heartbeat, insertEventsSql, insertHeartbeatSql, maxEventsPerBeat, upsertInstallSummarySql }
