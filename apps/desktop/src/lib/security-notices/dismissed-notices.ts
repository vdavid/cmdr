/**
 * Which toast each notice owns, and which ones the person closed this session. Its own module so
 * the toast component and `security-notices.ts` (which mounts it) can both reach it without an
 * import cycle.
 */

import type { AccessNotice } from '#lib/ipc/bindings.js'
import { dismissToast } from '#lib/ui/toast/index.js'

/** One toast per source: the same id replaces the toast in place as the count grows. */
export function securityNoticeId(notice: AccessNotice): string {
  switch (notice.kind) {
    case 'mcpForeignOrigin':
      return 'security-notice:mcp-foreign-origin'
    case 'smbSignatureRefused':
      return `security-notice:smb-signature:${notice.volumeId}`
    case 'updateNotGenuine':
      return `security-notice:update-not-genuine:${notice.version}`
  }
}

/** Notices the person closed this session. Frontend-side on purpose: it gates display only. */
const dismissed = new Set<string>()

export function isDismissed(id: string): boolean {
  return dismissed.has(id)
}

/** The toast's X: keeps the notice quiet for the rest of the session. */
export function markDismissed(id: string): void {
  dismissed.add(id)
}

/** The notice's own Dismiss: closes it and keeps it quiet for the session, like the X does. */
export function dismissSecurityNotice(notice: AccessNotice): void {
  const id = securityNoticeId(notice)
  dismissed.add(id)
  dismissToast(id)
}

/** Forgets what the person closed. Tests only. */
export function _resetSecurityNoticesForTest(): void {
  dismissed.clear()
}
