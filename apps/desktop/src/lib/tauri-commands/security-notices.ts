// The in-app notice for a refused access attempt (#457): a web page at the MCP server, a server
// reply refused for its signature, an update that isn't a genuine Cmdr release. The backend's
// gate decides what reaches the person and how often (`src-tauri/src/security_notices/`).

import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, events, type SecurityNoticeRaised } from '#lib/ipc/bindings.js'

/**
 * What launch raised before any window listened (a launch in MCP direct mode), handed over once.
 */
export function takeSecurityNoticesFromLaunch(): Promise<SecurityNoticeRaised[]> {
  return commands.takeSecurityNoticesFromLaunch()
}

/**
 * Subscribe to `security-notice-raised`. One-shot per raise and NOT seeded: a refusal raised before
 * this window existed was for a moment that's gone. A notice about launch itself waits for
 * {@link takeSecurityNoticesFromLaunch}.
 *
 * ⚠️ It reaches EVERY window. Subscribe from the main window only, or the settings window raises
 * its own copy of the toast.
 */
export function onSecurityNoticeRaised(callback: (payload: SecurityNoticeRaised) => void): Promise<UnlistenFn> {
  return events.securityNoticeRaised.listen((event) => {
    callback(event.payload)
  })
}
