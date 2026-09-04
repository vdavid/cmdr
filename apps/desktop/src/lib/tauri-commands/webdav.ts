// WebDAV servers: connecting, secrets, and the saved-server list.
//
// The whole flow, end to end, plus what every outcome means:
// `crates/cmdr-webdav/DETAILS.md`.

import { commands } from '$lib/ipc/bindings'
import type { KnownWebdavServer, WebdavConnectResult, WebdavUnattendedReconnect } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type { KnownWebdavServer, WebdavConnectResult, WebdavUnattendedReconnect }
export type { ConnectedWebdavVolume } from '$lib/ipc/bindings'

/** How to reach one WebDAV server. No secret: the backend reads those from the secret store itself. */
export interface WebdavTarget {
  /** What to call the server in the UI. */
  displayName: string
  /** The base URL, as the user typed it: scheme, host, optional port, and the DAV path. */
  url: string
  /** The account to sign in as. Part of the volume's identity. */
  username: string
  /** The remote directory to open at, relative to the base URL's path. */
  remoteRoot: string
  /**
   * Whether Cmdr may redial this server unattended when the session drops.
   *
   * Independent of whether the secret is remembered, which is the other switch
   * (`hasWebdavCredentials` / `saveWebdavCredentials` / `deleteWebdavCredentials`).
   * Their combination has a precondition, and `getWebdavUnattendedReconnect` is
   * what says whether it holds. Defaults to on.
   */
  autoReconnect: boolean
}

/**
 * Opens a WebDAV volume, or says what stands in the way.
 *
 * Switch on `result.outcome`. `connected` carries the volume id to navigate to;
 * `invalid_url` means the address never named an http(s) server and the form marks
 * the field. Nothing here is a message to parse.
 *
 * A successful connect registers the volume and adds the server to the saved list.
 *
 * `attemptId` is this call's own name, and `cancelWebdavConnect` takes the same one.
 * Make a fresh one per attempt with `newWebdavAttemptId()` and hold it: this promise
 * doesn't settle until the dial is over, so the id has to exist before the call for
 * a cancel button to have anything to aim at. A cancelled connect answers
 * `cancelled` and leaves nothing behind.
 */
export async function connectWebdavVolume(target: WebdavTarget, attemptId: string): Promise<WebdavConnectResult> {
  return await commands.connectWebdavVolume(
    target.displayName,
    target.url,
    target.username,
    target.remoteRoot,
    target.autoReconnect,
    attemptId,
  )
}

/** A name for one connect attempt, for the pair of `connectWebdavVolume` and `cancelWebdavConnect`. */
export function newWebdavAttemptId(): string {
  return `webdav-connect-${crypto.randomUUID()}`
}

/**
 * Calls off the connect running under `attemptId`, and returns whether one was.
 *
 * This is what a dialog's cancel button calls. The probe stops where it stands, the
 * `connectWebdavVolume` promise settles with `cancelled`, and no volume, saved
 * server, or secret is left behind.
 *
 * `false` means nobody was connecting under that id, which is what a click landing
 * a moment after the connect finished looks like. Nothing is wrong with it.
 */
export async function cancelWebdavConnect(attemptId: string): Promise<boolean> {
  return await commands.cancelWebdavConnect(attemptId)
}

/**
 * Drops a WebDAV volume's client and takes it out of the volume registry.
 * Returns whether there was a WebDAV volume under that id.
 */
export async function disconnectWebdavVolume(volumeId: string): Promise<boolean> {
  return await commands.disconnectWebdavVolume(volumeId)
}

/**
 * Saves the secret for one account on one server, so the next connection is silent.
 *
 * This call is the "remember the secret" switch: its meaning is exactly "put this in
 * the Keychain". `hasWebdavCredentials` reads the switch back,
 * `deleteWebdavCredentials` turns it off, and there's no second flag that could
 * disagree with the store. Remembering it makes an unattended reconnect possible;
 * turning one on is the other switch (`autoReconnect`).
 *
 * Throws a `KeychainError` if the store refused, or if `url` never named a server.
 */
export async function saveWebdavCredentials(url: string, username: string, secret: string): Promise<void> {
  const res = await commands.saveWebdavCredentials(url, username, secret)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Whether a password is stored for one account on one server.
 *
 * There's deliberately no command that returns the secret itself: the backend
 * reads the store when it builds a client.
 */
export async function hasWebdavCredentials(url: string, username: string): Promise<boolean> {
  return await commands.hasWebdavCredentials(url, username)
}

/** Forgets the stored password for one account on one server. */
export async function deleteWebdavCredentials(url: string, username: string): Promise<void> {
  const res = await commands.deleteWebdavCredentials(url, username)
  if (res.status === 'error') throwIpcError(res.error)
}

/** A saved server with every switch spelled out, which is what a picker or an edit form needs. */
export type SavedWebdavServer = KnownWebdavServer & { autoReconnect: boolean }

/**
 * Every WebDAV server the user has connected to.
 *
 * `autoReconnect` is typed optional on the generated `KnownWebdavServer` because a
 * file written before that switch existed omits it. This is the one place that
 * gap is closed, and on. Nowhere else should be spelling that default.
 */
export async function getKnownWebdavServers(): Promise<SavedWebdavServer[]> {
  const servers = await commands.getKnownWebdavServers()
  return servers.map((server) => ({ ...server, autoReconnect: server.autoReconnect ?? true }))
}

/**
 * Adds a saved server, or replaces the entry for the same URL and account.
 *
 * `connectWebdavVolume` already does this on every successful connection; this is
 * for editing one without connecting.
 */
export async function updateKnownWebdavServer(target: WebdavTarget): Promise<void> {
  await commands.updateKnownWebdavServer(
    target.url,
    target.username,
    target.displayName,
    target.remoteRoot,
    target.autoReconnect,
  )
}

/**
 * Drops a server from the saved list, returning whether one was there.
 *
 * Leaves the stored password alone: `deleteWebdavCredentials` is that.
 */
export async function forgetKnownWebdavServer(url: string, username: string): Promise<boolean> {
  return await commands.forgetKnownWebdavServer(url, username)
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
