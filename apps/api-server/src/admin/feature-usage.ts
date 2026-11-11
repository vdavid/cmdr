import { Hono } from 'hono'
import { verifyAdminAuth } from '../admin-auth'
import type { Bindings } from '../types'

/**
 * `/admin/feature-usage?range=7d|30d|90d`: per feature (the ids in the repo-root `feature-status.json`),
 * how many installs used it in the window and how many events it produced, against the window's
 * active installs. The per-release feature-status review reads it through the dashboard's report.
 * `DETAILS.md` § Feature usage.
 */

const featureUsage = new Hono<{ Bindings: Bindings }>()

/** One event that counts toward a feature, optionally only when a property holds one of `values`. */
export interface FeatureEventMatch {
  event: string
  prop?: string
  values?: readonly string[]
}

export interface FeatureUsageDefinition {
  /** A `feature-status.json` id. `feature-usage.test.ts` pins it, and `name`, against that file. */
  id: string
  name: string
  matches: readonly FeatureEventMatch[]
  /**
   * An optional breakdown of the feature's events: `'event'` splits by event name, any other value by
   * that property (`outcome`, say). Each part counts its own installs and events.
   */
  split?: string
}

/** `pane_navigated` into one of these volume kinds (`VolumeKind` in the desktop app). */
function navigated(...kinds: string[]): FeatureEventMatch {
  return { event: 'pane_navigated', prop: 'volume_kind', values: kinds }
}

const suggestionEvents = ['suggestion_group_proposed', 'suggestion_group_approved', 'suggestion_group_rejected']

function suggestions(...verbs: string[]): FeatureEventMatch[] {
  return suggestionEvents.map((event) => ({ event, prop: 'verb', values: verbs }))
}

/**
 * THE feature → event mapping. A new event is one line here; the SQL, the dashboard, and the report
 * follow. Event names and props are the catalog's (`apps/desktop/src-tauri/src/analytics/DETAILS.md` §
 * Starter event set). An event the app has never sent reads as "no data yet" on the dashboard, so a
 * mapping can land before the release that ships its event.
 *
 * - `ai-rename` vs `ai-batch` split the `suggestion_group_*` events on `verb` (`ProposalVerb`): `rename`
 *   is AI rename, the other file verbs are AI file organization. The server verbs (`add_server`,
 *   `edit_server`) belong to neither.
 */
export const featureUsageDefinitions: readonly FeatureUsageDefinition[] = [
  { id: 'search', name: 'Search', matches: [{ event: 'search_used' }], split: 'mode' },
  { id: 'select-files', name: 'Select files dialog', matches: [{ event: 'select_files_used' }], split: 'mode' },
  { id: 'image-search', name: 'Image search', matches: [{ event: 'image_search_used' }] },
  {
    id: 'archives',
    name: 'Archives',
    matches: [navigated('archive'), { event: 'archive_edit_completed' }],
    split: 'event',
  },
  {
    id: 'operation-log',
    name: 'Operation log',
    matches: [{ event: 'operation_log_opened' }, { event: 'operation_rollback_run' }],
    split: 'event',
  },
  { id: 'multi-rename', name: 'Multi-rename tool', matches: [{ event: 'multi_rename_applied' }] },
  { id: 'ask-cmdr', name: 'Ask Cmdr', matches: [{ event: 'ask_cmdr_turn' }], split: 'outcome' },
  {
    id: 'network-drives',
    name: 'Network drives (SMB)',
    matches: [{ event: 'smb_connected' }, navigated('smb')],
    split: 'event',
  },
  {
    id: 'android-devices',
    name: 'Android devices (MTP)',
    matches: [{ event: 'mtp_connected' }, navigated('mtp')],
    split: 'event',
  },
  {
    id: 'android-adb',
    name: 'Android devices (ADB)',
    matches: [{ event: 'adb_connected' }, navigated('adb')],
    split: 'event',
  },
  { id: 'sftp', name: 'SFTP servers', matches: [{ event: 'sftp_connected' }, navigated('sftp')], split: 'event' },
  {
    id: 'webdav',
    name: 'WebDAV servers',
    matches: [{ event: 'webdav_connected' }, navigated('webdav')],
    split: 'event',
  },
  { id: 'nfs', name: 'NFS shares', matches: [{ event: 'nfs_connected' }, navigated('nfs')], split: 'event' },
  { id: 's3', name: 'S3 storage', matches: [{ event: 's3_connected' }, navigated('s3')], split: 'event' },
  { id: 'restic-backups', name: 'Restic backups', matches: [navigated('restic')] },
  { id: 'git-browser', name: 'Git browser', matches: [navigated('git-portal')] },
  { id: 'download-notifications', name: 'Download notifications', matches: [{ event: 'download_jump_used' }] },
  { id: 'go-to-path', name: 'Go to folder', matches: [{ event: 'go_to_folder_used' }] },
  {
    id: 'file-operations',
    name: 'File operations',
    matches: [{ event: 'file_transfer_completed' }, { event: 'delete_used' }],
    split: 'event',
  },
  { id: 'file-viewer', name: 'File viewer', matches: [{ event: 'viewer_opened' }], split: 'content' },
  { id: 'tabs', name: 'Tabs', matches: [{ event: 'tab_opened' }] },
  {
    id: 'drag-and-drop',
    name: 'Drag and drop',
    matches: [{ event: 'drop_received' }, { event: 'drag_out_completed' }],
    split: 'event',
  },
  { id: 'ai-rename', name: 'AI rename', matches: suggestions('rename'), split: 'event' },
  {
    id: 'ai-batch',
    name: 'AI file organization',
    matches: suggestions('move', 'copy', 'trash', 'delete', 'compress', 'extract'),
    split: 'event',
  },
]

/** Every token interpolated into the SQL below must look like this; `assertSafeToken` enforces it. */
const safeToken = /^[a-z0-9_$-]{1,100}$/

function assertSafeToken(token: string): string {
  if (!safeToken.test(token)) throw new Error(`feature-usage: unsafe SQL token ${JSON.stringify(token)}`)
  return token
}

function quoted(token: string): string {
  return `'${assertSafeToken(token)}'`
}

/** The distinct event names the mapping reads, in first-seen order. */
export function mappedEvents(definitions: readonly FeatureUsageDefinition[]): string[] {
  return [...new Set(definitions.flatMap((def) => def.matches.map((m) => m.event)))]
}

/** A property of the event row, as text (D1 hands a JSON `true` back as `1`; text compares stay exact). */
function propSql(prop: string): string {
  return `CAST(json_extract(e.properties_json, '$.${assertSafeToken(prop)}') AS TEXT)`
}

function matchSql(match: FeatureEventMatch): string {
  const event = `e.event = ${quoted(match.event)}`
  if (match.prop === undefined || match.values === undefined) return event
  return `${event} AND ${propSql(match.prop)} IN (${match.values.map(quoted).join(', ')})`
}

function splitSql(split: string | undefined): string {
  if (split === undefined) return 'NULL'
  if (split === 'event') return 'e.event'
  return `COALESCE(${propSql(split)}, '(none)')`
}

/**
 * One row per (feature, part, install) in the window, with that install's event count; the totals and
 * breakdowns fold in `assembleFeatureUsage`. `?1` is the window start as an RFC 3339 UTC string, the
 * shape `occurred_at` is stored in, so the window is a range on `idx_analytics_event_event_occurred`.
 *
 * The mapping is compiled into `CASE` arms, interpolated (every token passes `assertSafeToken`) rather
 * than bound: D1 caps a statement at 100 bound parameters. A row takes the FIRST arm it matches, so
 * one stored event counts toward one feature at most; `feature-usage.test.ts` pins that no two
 * matches overlap.
 *
 * ❌ Don't "simplify" this into a join against a `VALUES` map, or into SQL-side `COUNT(DISTINCT)` over a
 * shared CTE. D1 bills every row a statement reads, temp tables included, and on production (30d,
 * 2026-11-11) those shapes read 970,000 and 6,600,000 rows: the planner re-read `pane_navigated`, by far
 * the biggest event, once per volume kind, or scanned the whole map once per event. This shape reads
 * each event in the window once. `DETAILS.md` § Feature usage.
 */
export function featureUsageSql(definitions: readonly FeatureUsageDefinition[]): string {
  const arms = definitions.flatMap((def) => def.matches.map((match) => ({ def, match })))
  const featureCase = arms.map(({ def, match }) => `WHEN ${matchSql(match)} THEN ${quoted(def.id)}`)
  const partCase = arms.map(({ def, match }) => `WHEN ${matchSql(match)} THEN ${splitSql(def.split)}`)
  const events = mappedEvents(definitions).map(quoted).join(', ')
  return `SELECT feature, part, anal_id AS installId, COUNT(*) AS events
  FROM (
    SELECT e.anal_id AS anal_id,
           CASE ${featureCase.join('\n                ')} END AS feature,
           CASE ${partCase.join('\n                ')} END AS part
      FROM analytics_event e
     WHERE e.event IN (${events}) AND e.occurred_at >= ?1
  )
 WHERE feature IS NOT NULL
 GROUP BY feature, part, anal_id`
}

/**
 * Which of the mapped events have EVER been stored, one indexed probe each (`EXISTS` stops at the
 * first row, so this reads about one row per event however big the table is). It's what lets the
 * dashboard print "no data yet" for an event no release has shipped instead of a 0% that reads as
 * "nobody uses it".
 *
 * The names come in as a `json_each` list because D1 refuses a `UNION ALL` of one probe per event ("too
 * many terms in compound SELECT", production, 2026-11-11), which plain SQLite runs fine.
 */
export function seenEventsSql(definitions: readonly FeatureUsageDefinition[]): string {
  const names = mappedEvents(definitions).map((event) => `"${assertSafeToken(event)}"`)
  return `SELECT ev.value AS event
  FROM json_each('[${names.join(',')}]') ev
 WHERE EXISTS (SELECT 1 FROM analytics_event WHERE event = ev.value)`
}

/**
 * The denominator: installs that beat at least once in the window. Same population as
 * `/admin/heartbeat-dau` (a suppressed or tooling install never beats, and the synthetic sweep removes
 * the rest), so a feature's share reads against the Active use numbers.
 */
export const activeInstallsSql = `SELECT COUNT(DISTINCT anal_id) AS installs FROM heartbeat WHERE created_at >= ?1`

export interface FeatureUsagePart {
  /** The event name for an `'event'` split, else the property's value (`(none)` when absent). */
  key: string
  installs: number
  events: number
}

export interface FeatureUsage {
  id: string
  name: string
  /** The mapping in words, like `pane_navigated[volume_kind=archive]`, so a reader can audit it. */
  sources: string[]
  /** Distinct installs with at least one matching event in the window. */
  installs: number
  events: number
  /** Whether any of the feature's events has ever been stored. `false` means "no data yet", not zero. */
  everSeen: boolean
  /** The `split` breakdown, biggest first; empty when the feature has no split. */
  parts: FeatureUsagePart[]
}

export interface FeatureUsageResponse {
  range: string
  /** The window start, RFC 3339 UTC. */
  since: string
  /** Installs that beat in the window: every share's denominator. */
  activeInstalls: number
  /** In mapping order. */
  features: FeatureUsage[]
}

/** One (feature, part, install) group from `featureUsageSql`. `part` is NULL for an unsplit feature. */
export interface UsageRow {
  feature: string
  part: string | null
  installId: string
  events: number
}

function describeMatch(match: FeatureEventMatch): string {
  if (match.prop === undefined || match.values === undefined) return match.event
  return `${match.event}[${match.prop}=${match.values.join('|')}]`
}

/** Distinct installs and summed events over some rows. */
function tally(rows: UsageRow[]): { installs: number; events: number } {
  return { installs: new Set(rows.map((row) => row.installId)).size, events: rows.reduce((s, r) => s + r.events, 0) }
}

/** Folds the per-install rows into one entry per definition, zero-filled, in mapping order. */
export function assembleFeatureUsage(
  definitions: readonly FeatureUsageDefinition[],
  rows: UsageRow[],
  seenEvents: ReadonlySet<string>,
): FeatureUsage[] {
  return definitions.map((def) => {
    const own = rows.filter((row) => row.feature === def.id)
    const byPart = new Map<string, UsageRow[]>()
    for (const row of own) {
      if (row.part === null) continue
      byPart.set(row.part, [...(byPart.get(row.part) ?? []), row])
    }
    const parts = [...byPart.entries()]
      .map(([key, partRows]) => ({ key, ...tally(partRows) }))
      .sort((a, b) => b.events - a.events || a.key.localeCompare(b.key))
    return {
      id: def.id,
      name: def.name,
      sources: def.matches.map(describeMatch),
      ...tally(own),
      everSeen: def.matches.some((m) => seenEvents.has(m.event)),
      parts,
    }
  })
}

const rangeDays: Partial<Record<string, number>> = { '7d': 7, '30d': 30, '90d': 90 }

/** Runs the three queries for a window starting at `since` (a `Date`). */
export async function queryFeatureUsage(
  db: D1Database,
  since: Date,
  definitions: readonly FeatureUsageDefinition[] = featureUsageDefinitions,
): Promise<{ activeInstalls: number; features: FeatureUsage[] }> {
  const sinceIso = since.toISOString()
  // `heartbeat.created_at` is SQLite's `YYYY-MM-DD HH:MM:SS`, not RFC 3339.
  const sinceSqlite = sinceIso.slice(0, 19).replace('T', ' ')
  const [usage, seen, active] = await Promise.all([
    db.prepare(featureUsageSql(definitions)).bind(sinceIso).all<UsageRow>(),
    db.prepare(seenEventsSql(definitions)).all<{ event: string }>(),
    db.prepare(activeInstallsSql).bind(sinceSqlite).first<{ installs: number }>(),
  ])
  const seenEvents = new Set(seen.results.map((row) => row.event))
  return {
    activeInstalls: active?.installs ?? 0,
    features: assembleFeatureUsage(definitions, usage.results, seenEvents),
  }
}

featureUsage.get('/admin/feature-usage', async (c) => {
  const authError = await verifyAdminAuth(c)
  if (authError) return authError

  const range = c.req.query('range') ?? '30d'
  const days = rangeDays[range]
  if (days === undefined) return c.json({ error: 'Invalid range. Use 7d, 30d, or 90d' }, 400)

  const since = new Date(Date.now() - days * 86_400_000)
  const result = await queryFeatureUsage(c.env.TELEMETRY_DB, since)
  const body: FeatureUsageResponse = { range, since: since.toISOString(), ...result }
  return c.json(body)
})

export { featureUsage }
