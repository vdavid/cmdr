// SFTP servers: host-key trust, secrets, and the saved-server list. Connecting
// goes through the protocol-agnostic `servers.ts` (`connectServer` /
// `connectSavedPlace`).
//
// The whole flow, end to end, plus what every outcome means:
// `crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend".

import { commands } from '$lib/ipc/bindings'
import type {
  HostKeyPrompt,
  KnownSftpServer,
  SftpHostKeyApprovalResult,
  SftpUnattendedReconnect,
  TrustedHostKey,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type { HostKeyPrompt, KnownSftpServer, SftpHostKeyApprovalResult, TrustedHostKey }
export type { SftpHostKeyIdentity, SftpUnattendedReconnect } from '$lib/ipc/bindings'

/**
 * Calls off the connect running under `attemptId`, and returns whether one was.
 *
 * This is what a dialog's cancel button calls. The key exchange and the auth ladder
 * stop where they stand; a cancel landing in the SFTP hello ends the wait just the
 * same and lets the protocol engine finish quietly on its own. Either way the
 * connect promise (`connectServer` / `connectSavedPlace`) settles with `cancelled`,
 * and no volume, saved server, or secret is left behind.
 *
 * `false` means nobody was connecting under that id, which is what a click landing
 * a moment after the connect finished looks like. Nothing is wrong with it.
 */
export async function cancelSftpConnect(attemptId: string): Promise<boolean> {
  return await commands.cancelSftpConnect(attemptId)
}

/**
 * Drops an SFTP volume's session and takes it out of the volume registry.
 * Returns whether there was an SFTP volume under that id.
 */
export async function disconnectSftpVolume(volumeId: string): Promise<boolean> {
  return await commands.disconnectSftpVolume(volumeId)
}

/**
 * Records a host key the user approved, and only if the server still presents it.
 *
 * Returns `recorded` when the key is now trusted (dial again for a fresh
 * connect), `superseded` when the server presents a different key than
 * the one shown (nothing was written; start over on the key it carries), or
 * `unreachable` when the server couldn't be re-asked.
 */
export async function approveSftpHostKey(prompt: {
  host: string
  port: number
  algorithm: string
  fingerprint: string
}): Promise<SftpHostKeyApprovalResult> {
  return await commands.approveSftpHostKey(prompt.host, prompt.port, prompt.algorithm, prompt.fingerprint)
}

/**
 * Drops the approval for one host key, so the next connection to that server is
 * first contact again. Returns whether anything was there.
 */
export async function forgetSftpHostKey(host: string, port: number, algorithm: string): Promise<boolean> {
  return await commands.forgetSftpHostKey(host, port, algorithm)
}

/** Every SSH host key this machine has approved, for a settings screen. */
export async function listTrustedSftpHostKeys(): Promise<TrustedHostKey[]> {
  return await commands.listTrustedSftpHostKeys()
}

/**
 * Saves the secret for one account on one server, so the next connection is silent.
 *
 * This call is the "remember the secret" switch: its meaning is exactly "put this in
 * the Keychain". `hasSftpCredentials` reads the switch back, `deleteSftpCredentials`
 * turns it off, and there's no second flag that could disagree with the store.
 *
 * One entry per account, whatever the rung uses it for: the backend offers it as the
 * password on the password and keyboard-interactive rungs, and as the key file's
 * passphrase on the key-file rung. Remembering it makes an unattended reconnect
 * possible on those rungs; turning one on is the other switch (`autoReconnect`).
 *
 * Throws a `KeychainError` if the store refused.
 */
export async function saveSftpCredentials(host: string, port: number, username: string, secret: string): Promise<void> {
  const res = await commands.saveSftpCredentials(host, port, username, secret)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Whether a password is stored for one account on one server.
 *
 * There's deliberately no command that returns the secret itself: the backend
 * reads the store when it builds a session.
 */
export async function hasSftpCredentials(host: string, port: number, username: string): Promise<boolean> {
  return await commands.hasSftpCredentials(host, port, username)
}

/** Forgets the stored password for one account on one server. */
export async function deleteSftpCredentials(host: string, port: number, username: string): Promise<void> {
  const res = await commands.deleteSftpCredentials(host, port, username)
  if (res.status === 'error') throwIpcError(res.error)
}

/** A saved server with every switch spelled out, which is what a picker or an edit form needs. */
export type SavedSftpServer = KnownSftpServer & { autoReconnect: boolean; pinned: boolean }

/**
 * Every SFTP server the user has connected to.
 *
 * `autoReconnect` and `pinned` are typed optional on the generated
 * `KnownSftpServer` because a file written before either existed omits it. This
 * is the one place both gaps are closed, and they close the OPPOSITE way:
 * `autoReconnect` defaults ON (SFTP has always reconnected on its own, so
 * reading a missing field as off would switch it off under every server saved so
 * far) and `pinned` defaults OFF (nothing was in the switcher before pins, so on
 * would drop every saved server into it at once). ❌ Nowhere else should be
 * spelling either default.
 */
export async function getKnownSftpServers(): Promise<SavedSftpServer[]> {
  const servers = await commands.getKnownSftpServers()
  return servers.map((server) => ({
    ...server,
    autoReconnect: server.autoReconnect ?? true,
    pinned: server.pinned ?? false,
  }))
}

/**
 * Drops a server from the saved list, returning whether one was there.
 *
 * Leaves the stored password and the trusted host key alone: `deleteSftpCredentials`
 * and `forgetSftpHostKey` are those.
 */
export async function forgetKnownSftpServer(host: string, port: number, username: string): Promise<boolean> {
  return await commands.forgetKnownSftpServer(host, port, username)
}

/**
 * Whether a mounted SFTP volume can actually come back on its own as it stands.
 *
 * The backend's own answer to "auto-reconnect is on and nothing happens", so no UI
 * has to derive it from a rung plus a credential check. `needs_stored_secret` is the
 * one to warn about: the switch is on, this volume signs in from the secret store,
 * and nothing is stored. `rung_cannot` means the server asks its own questions every
 * time, so remembering a secret wouldn't help.
 *
 * `null` when nothing SFTP is mounted under that id: the answer depends on which
 * credential proved the live session, and there isn't one. Ask when a banner
 * renders rather than polling; it can reach the Keychain.
 */
export async function getSftpUnattendedReconnect(volumeId: string): Promise<SftpUnattendedReconnect | null> {
  return await commands.getSftpUnattendedReconnect(volumeId)
}
