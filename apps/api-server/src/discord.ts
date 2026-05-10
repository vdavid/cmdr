/**
 * Minimal Discord webhook client for #error-reports notifications.
 *
 * No queue infra. Single retry on 429 honoring `Retry-After`, then
 * `console.error` and drop. The channel is internal-only — at most one
 * dropped notification is acceptable.
 */

export interface ErrorReportNotification {
  id: string
  kind: 'user' | 'auto'
  /**
   * Forwarded from the manifest. The embed title gets `[DEV]` for debug builds
   * (`cfg!(debug_assertions)`) and `[PROD]` for release builds so triage can tell
   * them apart at a glance regardless of channel. Defaults to `'release'` upstream
   * when older clients don't set it.
   */
  buildMode: 'release' | 'debug'
  appVersion: string
  osVersion: string
  arch: string
  sizeBytes: number
  uploadedUnixSeconds: number
  downloadUrl: string
  userNote?: string
}

export interface EvictionInfo {
  evictedCount: number
  freedBytes: number
  newTotalBytes: number
}

const ERROR_REPORT_EMBED_COLOR = 0xff6b6b
const USER_NOTE_EMBED_CAP = 500

/** "1.23 GB", "456 MB", "789 KB", "12 B". */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes.toString()} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let i = 0
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024
    i++
  }
  return `${value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : 2)} ${units[i]}`
}

/** Build the Discord webhook JSON body for a new error report. */
export function buildErrorReportPayload(n: ErrorReportNotification): unknown {
  const truncatedNote =
    n.userNote && n.userNote.length > USER_NOTE_EMBED_CAP
      ? n.userNote.slice(0, USER_NOTE_EMBED_CAP) + '… (full note in bundle)'
      : n.userNote

  const fields: { name: string; value: string; inline?: boolean }[] = [
    { name: 'Kind', value: n.kind, inline: true },
    { name: 'App version', value: n.appVersion, inline: true },
    { name: 'OS', value: n.osVersion, inline: true },
    { name: 'Arch', value: n.arch, inline: true },
    { name: 'Size', value: formatBytes(n.sizeBytes), inline: true },
    { name: 'Uploaded', value: `<t:${n.uploadedUnixSeconds.toString()}:R>`, inline: true },
    { name: 'Download', value: `[Download bundle](${n.downloadUrl}) (link valid 7 days)` },
  ]
  if (truncatedNote) {
    fields.push({ name: 'User note', value: truncatedNote })
  }

  const titlePrefix = n.buildMode === 'debug' ? '[DEV] ' : '[PROD] '
  return {
    embeds: [
      {
        title: `${titlePrefix}Error report ${n.id}`,
        color: ERROR_REPORT_EMBED_COLOR,
        fields,
      },
    ],
  }
}

/** Build the Discord webhook JSON body for an eviction summary. */
export function buildEvictionPayload(info: EvictionInfo): unknown {
  return {
    content:
      `Eviction sweep: removed ${info.evictedCount.toString()} oldest bundle(s), ` +
      `freed ${formatBytes(info.freedBytes)}. New total: ${formatBytes(info.newTotalBytes)}.`,
  }
}

async function postOnce(url: string, body: unknown): Promise<Response> {
  return fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
}

/**
 * POST `body` to the webhook. On 429, sleep for `Retry-After` and retry once.
 * On second failure, log and drop — no exception to caller.
 */
async function postWithRetry(url: string, body: unknown, label: string): Promise<void> {
  try {
    let res = await postOnce(url, body)
    if (res.status === 429) {
      const retryAfterRaw = res.headers.get('Retry-After') ?? '1'
      const retryAfterSec = Math.max(0, Math.min(60, parseFloat(retryAfterRaw) || 1))
      await new Promise((r) => setTimeout(r, retryAfterSec * 1000))
      res = await postOnce(url, body)
    }
    if (!res.ok) {
      console.error(`Discord ${label} POST failed: HTTP ${res.status.toString()}`)
    }
  } catch (e) {
    console.error(`Discord ${label} POST threw:`, e)
  }
}

export async function postErrorReportNotification(
  webhookUrl: string,
  notification: ErrorReportNotification,
): Promise<void> {
  await postWithRetry(webhookUrl, buildErrorReportPayload(notification), 'error-report')
}

export async function postEvictionNotification(webhookUrl: string, info: EvictionInfo): Promise<void> {
  await postWithRetry(webhookUrl, buildEvictionPayload(info), 'eviction')
}
