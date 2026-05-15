/**
 * Lazy trigger for the macOS Local Network permission prompt.
 *
 * mDNS browsing isn't started at app launch on fresh installs. Instead, the first user
 * action that depends on networking calls `triggerNetworkDiscovery()`. This is what fires
 * the system "Cmdr wants to find devices on local networks" prompt: macOS gates it on the
 * actual multicast browse, not on app startup. After the first trigger we persist
 * `network.firstTriggerDone = true` so subsequent launches start mDNS eagerly without
 * surprising the user.
 *
 * Callers: `NetworkBrowser` mount, `ConnectToServerDialog` open, the OS-mount → direct-smb2
 * upgrade click in `VolumeBreadcrumb`.
 *
 * No-op when `network.enabled === false`. The caller doesn't need to gate; this is the
 * single chokepoint.
 */

import { ensureNetworkDiscoveryStarted } from '$lib/tauri-commands'
import { getSetting, setSetting } from '$lib/settings'

export function triggerNetworkDiscovery(): void {
  if (!getSetting('network.enabled')) return

  void ensureNetworkDiscoveryStarted()

  if (!getSetting('network.firstTriggerDone')) {
    setSetting('network.firstTriggerDone', true)
  }
}
