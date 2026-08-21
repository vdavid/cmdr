/**
 * Who can render the inline SMB credential form right now.
 *
 * The form lives inside a file pane (`FilePane` renders `NetworkLoginForm` off
 * `smbView.smbUpgradeLogin`), so anything outside a pane that needs credentials —
 * the OS-mount fallback notice, a toast, anything app-global — has no way to raise
 * it on its own. Each pane registers its opener here while it's mounted; callers
 * ask for one by volume.
 *
 * A pane can host a form for a volume it isn't showing: that's already how the
 * breadcrumb dropdown works, where picking "Connect directly" on any listed volume
 * opens the form in the pane whose dropdown it is. So a host that matches the
 * volume is preferred, and any host will do.
 */

import type { UpgradeResult } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')

/** Shows the inline credential form for `volumeId`. */
export type CredentialsPrompt = (info: UpgradeResult & { status: 'credentialsNeeded' }, volumeId: string) => void

export interface SmbLoginHost {
  /** The volume this host is currently showing. Read live: panes navigate. */
  getVolumeId: () => string
  open: CredentialsPrompt
}

/** Insertion-ordered, so "any host" resolves to the longest-lived pane. */
const hosts = new Set<SmbLoginHost>()

/**
 * Registers a pane as able to host the credential form. Returns the unregister
 * function, so an `$effect` can `return registerSmbLoginHost(...)` directly.
 */
export function registerSmbLoginHost(host: SmbLoginHost): () => void {
  hosts.add(host)
  return () => {
    hosts.delete(host)
  }
}

/**
 * Raises the credential form for `volumeId`, preferring a pane already showing
 * that volume. Returns `false` when no pane is mounted to host it, which leaves
 * the caller to say something rather than look like the click did nothing.
 */
export function promptForSmbCredentials(
  info: UpgradeResult & { status: 'credentialsNeeded' },
  volumeId: string,
): boolean {
  const mounted = [...hosts]
  const host = mounted.find((h) => h.getVolumeId() === volumeId) ?? mounted.at(0)
  if (!host) {
    log.warn('No pane can host the SMB credential form for {volumeId}', { volumeId })
    return false
  }
  host.open(info, volumeId)
  return true
}

/** Test-only: drop every registration so one spec can't leak into the next. */
export function _clearSmbLoginHostsForTests(): void {
  hosts.clear()
}
