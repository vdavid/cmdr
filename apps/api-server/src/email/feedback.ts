/**
 * How the three-hour digest (`digest.ts`) renders in-app feedback: one card per message.
 *
 * Cards rather than the crash table, because feedback is prose: a table column shreds a paragraph
 * into a ribbon.
 */

import { formatBytes } from '../types'
import {
  CARD_FOOTER_STYLE,
  CARD_HEADER_STYLE,
  CARD_PROSE_STYLE,
  CARD_STYLE,
  boardLine,
  escapeHtml,
  replyToLine,
  type BoardCard,
} from './layout'

/**
 * The logs a sender chose to include with their message: an error report uploaded under the id
 * the feedback row names. `arrived: false` when that report isn't in D1 (yet).
 */
export type FeedbackEmailLogs =
  | { id: string; arrived: true; sizeBytes: number; downloadUrl: string | null; linkTtlHours: number }
  | { id: string; arrived: false }

/**
 * One in-app feedback message, as the digest renders it. Every field here is either written by a
 * person or copied from their machine, so every one of them is escaped at render time.
 */
export interface FeedbackEmailRow {
  /** `created_at`, in the SQLite `datetime('now')` shape (`YYYY-MM-DD HH:MM:SS`, UTC). */
  when: string
  /** `app_version`. */
  version: string
  /** `os_version`. */
  osVersion: string
  /** What the person wrote, verbatim. Untrusted text; line breaks are theirs and are preserved. */
  message: string
  /** The reply-to address the sender chose to attach, or `null` when they attached none. */
  email: string | null
  /** The logs they included, or null when they included none. */
  logs: FeedbackEmailLogs | null
  /** The message's card on the board, or null when the cron hasn't filed it yet. */
  card: BoardCard | null
}

/** The logs line: the bundle and its link, or why there's no bundle. */
function logsLine(logs: FeedbackEmailLogs): string {
  const label = `Logs: ${escapeHtml(logs.id)}`
  if (!logs.arrived) return `${label} &middot; hadn't arrived yet. If it does, it comes in a later digest.`
  const link = logs.downloadUrl
    ? `<a href="${escapeHtml(logs.downloadUrl)}" style="color: #2563eb;">Download the bundle</a> &middot; the link works for ${logs.linkTtlHours.toString()} hours`
    : 'No download link this time. Fetch the bundle through the Cloudflare API.'
  return `${label} &middot; ${escapeHtml(formatBytes(logs.sizeBytes))} &middot; ${link}`
}

/** One card per message, stacked. */
export function renderFeedbackCard(entry: FeedbackEmailRow): string {
  return `
    <div style="${CARD_STYLE}">
        <div style="${CARD_HEADER_STYLE}">
            ${escapeHtml(entry.when)} UTC &middot; app ${escapeHtml(entry.version)} &middot; OS ${escapeHtml(entry.osVersion)}
        </div>
        <div style="${CARD_PROSE_STYLE}">${escapeHtml(entry.message)}</div>
        <div style="${CARD_FOOTER_STYLE}">${replyToLine(entry.email)}</div>
        ${entry.logs ? `<div style="${CARD_FOOTER_STYLE}">${logsLine(entry.logs)}</div>` : ''}
        <div style="${CARD_FOOTER_STYLE}">${boardLine(entry.card)}</div>
    </div>`
}
