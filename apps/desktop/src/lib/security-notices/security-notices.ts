/**
 * The in-app notice for a refused access attempt (#457, the CRA's "report on possible
 * unauthorised access"): a web page reaching for the MCP server, a server reply refused for its
 * signature, an update that isn't a genuine Cmdr release.
 *
 * The backend decides WHAT reaches the person and HOW OFTEN (`src-tauri/src/security_notices/`:
 * a typed `AccessNotice` per source, re-raised with the grown count at most once a minute). This
 * module decides how it looks, and remembers what the person closed: a closed notice stays quiet
 * for the rest of the session (the security log still counts everything).
 *
 * ⚠️ **Main window only.** `security-notice-raised` reaches every window;
 * `routes/(main)/window-services.ts` is what scopes it.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import type { SecurityNoticeRaised } from '#lib/ipc/bindings.js'
import type { ReportIncident } from '#lib/error-reporter/report-incident.js'
import { onSecurityNoticeRaised } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import SecurityNoticeToastContent from './SecurityNoticeToastContent.svelte'
import { isDismissed, markDismissed, securityNoticeId } from './dismissed-notices'

export { dismissSecurityNotice, _resetSecurityNoticesForTest } from './dismissed-notices'

/** Every security notice counts against one group, so a burst can't push unrelated toasts off. */
export const SECURITY_NOTICE_GROUP = 'security-notice'

const MAX_IN_GROUP = 3

/** Shows (or updates) the toast for one raise, unless the person closed this notice already. */
export function announceSecurityNotice({ notice, count }: SecurityNoticeRaised): void {
  const id = securityNoticeId(notice)
  if (isDismissed(id)) return
  const shared = {
    id,
    dismissal: 'persistent' as const,
    toastGroup: SECURITY_NOTICE_GROUP,
    maxInGroup: MAX_IN_GROUP,
    props: { notice, count },
    onDismiss: () => {
      markDismissed(id)
    },
  }
  switch (notice.kind) {
    case 'mcpForeignOrigin':
    case 'smbSignatureRefused':
      addToast(SecurityNoticeToastContent, { ...shared, level: 'warn' })
      return
    case 'updateNotGenuine': {
      // The scariest case: the report offer is on, with the typed facts and never a path.
      const report: ReportIncident = { kind: 'updateNotGenuine', version: notice.version, check: notice.check }
      addToast(SecurityNoticeToastContent, { ...shared, level: 'error', report })
      return
    }
  }
}

let unlisten: UnlistenFn | null = null

/** Start listening. Call once at app init, from the main window only. */
export async function startSecurityNotices(): Promise<void> {
  if (unlisten) return
  unlisten = await onSecurityNoticeRaised(announceSecurityNotice)
}

export function stopSecurityNotices(): void {
  if (!unlisten) return
  unlisten()
  unlisten = null
}
