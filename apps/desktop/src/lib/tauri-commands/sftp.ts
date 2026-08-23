// SFTP servers: connecting, host-key trust, secrets, and the saved-server list.
//
// The whole flow, end to end, plus what every outcome means:
// `crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend".

import { commands } from '$lib/ipc/bindings'
import type {
  HostKeyPrompt,
  KnownSftpServer,
  SftpConnectResult,
  SftpHostKeyApprovalResult,
  SftpUnattendedReconnect,
  TrustedHostKey,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type { HostKeyPrompt, KnownSftpServer, SftpConnectResult, SftpHostKeyApprovalResult, TrustedHostKey }
export type { SftpAuthRung, ConnectedSftpVolume, SftpHostKeyIdentity, SftpUnattendedReconnect } from '$lib/ipc/bindings'

/** How to reach one SFTP server. No secret: the backend reads those from the secret store itself. */
export interface SftpTarget {
  /** What to call the server in the UI. */
  displayName: string
  /** The host, as the user typed it. */
  host: string
  /** The TCP port. 22 unless the user says otherwise. */
  port: number
  /** The account to sign in as. Part of the volume's identity. */
  username: string
  /** The remote directory to open at. Absolute, server-side. */
  remoteRoot: string
  /** A private key file to offer. A path, not a secret. */
  keyFile?: string | null
  /** Whether the running ssh-agent may be asked. */
  useAgent: boolean
  /**
   * Whether Cmdr may redial this server unattended when the session drops.
   *
   * Independent of whether the secret is remembered, which is the other switch
   * (`hasSftpCredentials` / `saveSftpCredentials` / `deleteSftpCredentials`).
   * Their combination has a precondition, and `getSftpUnattendedReconnect` is
   * what says whether it holds. Defaults to on, which is how SFTP has always
   * behaved.
   */
  autoReconnect: boolean
}

/**
 * Opens an SFTP volume, or says what stands in the way.
 *
 * Switch on `result.outcome`. `connected` carries the volume id to navigate to;
 * `needs_host_key_approval` carries the fingerprint to show, and `kind` says
 * whether it's first contact (`unknown`) or a CHANGED key, which must never take
 * the same one-click path. Nothing here is a message to parse.
 *
 * A successful connect registers the volume and adds the server to the saved list.
 */
export async function connectSftpVolume(target: SftpTarget): Promise<SftpConnectResult> {
  return await commands.connectSftpVolume(
    target.displayName,
    target.host,
    target.port,
    target.username,
    target.remoteRoot,
    target.keyFile ?? null,
    target.useAgent,
    target.autoReconnect,
  )
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
 * Returns `recorded` when the key is now trusted (call `connectSftpVolume` again
 * for a fresh dial), `superseded` when the server presents a different key than
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
export type SavedSftpServer = KnownSftpServer & { autoReconnect: boolean }

/**
 * Every SFTP server the user has connected to.
 *
 * `autoReconnect` is typed optional on the generated `KnownSftpServer` because a
 * file written before that switch existed omits it. This is the one place that
 * gap is closed, and on — SFTP has always reconnected on its own, so reading a
 * missing field as off would switch it off under every server saved so far.
 * Nowhere else should be spelling that default.
 */
export async function getKnownSftpServers(): Promise<SavedSftpServer[]> {
  const servers = await commands.getKnownSftpServers()
  return servers.map((server) => ({ ...server, autoReconnect: server.autoReconnect ?? true }))
}

/**
 * Adds a saved server, or replaces the entry for the same host, port, and account.
 *
 * `connectSftpVolume` already does this on every successful connection; this is
 * for editing one without connecting.
 */
export async function updateKnownSftpServer(target: SftpTarget): Promise<void> {
  await commands.updateKnownSftpServer(
    target.host,
    target.port,
    target.username,
    target.displayName,
    target.remoteRoot,
    target.keyFile ?? null,
    target.useAgent,
    target.autoReconnect,
  )
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
