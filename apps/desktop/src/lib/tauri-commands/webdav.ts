// WebDAV servers: secrets and the saved-server list. Connecting goes through
// the protocol-agnostic `servers.ts` (`connectServer` / `connectSavedPlace`).
//
// The whole flow, end to end, plus what every outcome means:
// `crates/cmdr-webdav/DETAILS.md`.

import { commands } from '$lib/ipc/bindings'
import type { KnownWebdavServer, WebdavUnattendedReconnect } from '$lib/ipc/bindings'
import { throwKeychainError } from '$lib/servers/keychain-failure'

export type { KnownWebdavServer, WebdavUnattendedReconnect }

/**
 * Saves the secret for one account on one server, so the next connection is silent.
 *
 * This call is the "remember the secret" switch: its meaning is exactly "put this in
 * the Keychain". `hasServerSecret` reads the switch back, `forgetServerSecret` turns
 * it off (both in `servers.ts`), and there's no second flag that could disagree with
 * the store. Remembering it makes an unattended reconnect possible;
 * turning one on is the other switch (`autoReconnect`).
 *
 * Throws a `KeychainFailure` if the store refused, or if `url` never named a server.
 */
export async function saveWebdavCredentials(url: string, username: string, secret: string): Promise<void> {
  const res = await commands.saveWebdavCredentials(url, username, secret)
  if (res.status === 'error') throwKeychainError(res.error)
}

/** A saved server with every switch spelled out, which is what a picker or an edit form needs. */
export type SavedWebdavServer = KnownWebdavServer & { autoReconnect: boolean; pinned: boolean }

/**
 * Every WebDAV server the user has connected to.
 *
 * `autoReconnect` and `pinned` are typed optional on the generated
 * `KnownWebdavServer` because a file written before either existed omits it.
 * This is the one place both gaps are closed, and they close the OPPOSITE way:
 * `autoReconnect` defaults ON, `pinned` defaults OFF (nothing was in the switcher
 * before pins, so on would drop every saved server into it at once).
 * ❌ Nowhere else should be spelling either default.
 */
export async function getKnownWebdavServers(): Promise<SavedWebdavServer[]> {
  const servers = await commands.getKnownWebdavServers()
  return servers.map((server) => ({
    ...server,
    autoReconnect: server.autoReconnect ?? true,
    pinned: server.pinned ?? false,
  }))
}

/**
 * Whether a mounted WebDAV volume can actually come back on its own as it stands.
 *
 * The backend's own answer to "auto-reconnect is on and nothing happens", so no UI
 * has to derive it from a credential check. `no_stored_secret` is the one to warn
 * about: the switch is on and nothing is stored.
 *
 * `null` when nothing WebDAV is mounted under that id. Ask when a banner renders
 * rather than polling; it can reach the Keychain.
 */
export async function getWebdavUnattendedReconnect(volumeId: string): Promise<WebdavUnattendedReconnect | null> {
  return await commands.getWebdavUnattendedReconnect(volumeId)
}
