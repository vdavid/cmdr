/**
 * The words for a direct-connection attempt that didn't work.
 *
 * Rust classifies the failure into a typed, word-free `UpgradeFailure` and names
 * the server; this file owns the sentence, resolved from the message catalog.
 * Same split as `$lib/error-messages/` (classification in Rust, copy on the frontend),
 * kept here because these are plain toast strings rather than the markdown
 * `FriendlyErrorMessage` pipeline the error pane uses.
 *
 * None of these copy variants says the connection "failed": nothing broke. The
 * share is still there, still browsable, just over its slower regular
 * connection, and every message says so.
 */

import type { UpgradeFailure } from '$lib/ipc/bindings'
import { tString } from '$lib/intl/messages.svelte'

/**
 * The toast text for a direct-connection attempt that couldn't reach the server.
 *
 * `server` is the friendly name the backend resolved (an mDNS hostname when it
 * has one, the IP otherwise).
 */
export function directConnectionUnavailableMessage(reason: UpgradeFailure, server: string): string {
  switch (reason) {
    case 'unreachable':
      return tString('fileExplorer.pane.directConnectionUnreachableToast', { server })
    case 'tooSlow':
      return tString('fileExplorer.pane.directConnectionTooSlowToast', { server })
    case 'unexpected':
      return tString('fileExplorer.pane.directConnectionUnexpectedToast', { server })
  }
}
