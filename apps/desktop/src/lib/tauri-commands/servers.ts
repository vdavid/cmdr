// One command family for "a server", whatever protocol it speaks: the saved
// list, the two dials, cancel, disconnect, the pin, and the two forgets.
//
// ❗ The frontend branches on protocol NOWHERE above this line. The per-protocol
// commands (`./sftp`, `./webdav`) stay for what only that protocol has (host
// keys, key files, a WebDAV URL); everything the hub, the switcher, and the
// sign-in sheet do goes through here.
//
// What each outcome means and which side owns it:
// `src-tauri/src/commands/DETAILS.md` § `servers.rs`, over
// `src-tauri/src/commands/servers.rs`.

import { commands } from '$lib/ipc/bindings'
import type {
  SavedPlace,
  SavedServer,
  SavedServerOutcome,
  SecretOffer,
  ServerConnectOutcome,
  ServerProtocol,
  ServerTarget,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type {
  SavedPlace,
  SavedServer,
  SavedServerOutcome,
  SecretOffer,
  ServerConnectOutcome,
  ServerProtocol,
  ServerTarget,
}

/**
 * Every server the user has saved, across all three stores.
 *
 * Cached state only, so calling it on a `volumes-changed` refresh costs no
 * network traffic. An SMB host lists no places and can't be pinned; see the
 * type's own note.
 */
export async function listSavedServers(): Promise<SavedServer[]> {
  return await commands.listSavedServers()
}

/**
 * A name for one connect attempt, minted BEFORE the dial so a cancel button is
 * armed from the first millisecond. `cancelServerConnect` takes the same one.
 */
export function newServerAttemptId(): string {
  return `server-connect-${crypto.randomUUID()}`
}

/**
 * Dials a server the user already saved, by its place's volume id.
 *
 * ❗ Only for a place with NO registered volume. A registered volume that dropped
 * is mended by `reconnectVolumeWithCredentials`, and re-dialing it would register
 * a second volume. `servers/connect-flow.ts` is what picks between them; the two
 * ways of picking wrong throw a typed `SavedPlaceRefusal`, which a user should
 * never see.
 */
export async function connectSavedPlace(
  volumeId: string,
  attemptId: string,
  secret: SecretOffer | null = null,
): Promise<ServerConnectOutcome> {
  const result = await commands.connectSavedPlace(volumeId, attemptId, secret)
  if (result.status === 'error') throwIpcError(result.error)
  return result.data
}

/**
 * Dials a server the user just typed, in add mode. A successful dial registers
 * the volume AND saves the server, so its second use costs one keystroke.
 */
export async function connectServer(
  target: ServerTarget,
  attemptId: string,
  secret: SecretOffer | null = null,
): Promise<ServerConnectOutcome> {
  return await commands.connectServer(target, attemptId, secret)
}

/**
 * Calls off the connect running under `attemptId`, answering whether one was.
 *
 * `false` is what a click landing just after the dial finished looks like, and
 * nothing is wrong with it.
 */
export async function cancelServerConnect(attemptId: string): Promise<boolean> {
  return await commands.cancelServerConnect(attemptId)
}

/**
 * Drops a place's session, answering whether there was one.
 *
 * ❗ The place stays SAVED: a pinned place becomes a `saved` row in the switcher.
 * Emits `VolumeUnmounted`, so a pane standing on it goes home.
 */
export async function disconnectPlace(volumeId: string): Promise<boolean> {
  return await commands.disconnectPlace(volumeId)
}

/**
 * Moves a place's pin, answering whether a saved place was there.
 *
 * Emits `volumes-changed`: the Network group IS the pinned and connected places,
 * so an unpin the list never hears about leaves a row nothing will remove.
 */
export async function setPlacePinned(volumeId: string, pinned: boolean): Promise<boolean> {
  return await commands.setPlacePinned(volumeId, pinned)
}

/**
 * Drops a server from the saved list, answering whether one was there.
 *
 * Also drops the session and unregisters the volume, and leaves the stored
 * secret alone: `forgetServerSecret` is the other request.
 */
export async function forgetServer(id: string): Promise<boolean> {
  return await commands.forgetServer(id)
}

/**
 * Whether a secret is remembered for the place `id` names.
 *
 * The protocol-agnostic reader behind the "Forget saved password" menu item. A
 * store that didn't answer reads as `false`, the same harmless collapse the
 * per-protocol readers make.
 */
export async function hasServerSecret(id: string): Promise<boolean> {
  return await commands.hasServerSecret(id)
}

/** Forgets a place's remembered secret, keeping the SERVER saved. */
export async function forgetServerSecret(id: string): Promise<boolean> {
  return await commands.forgetServerSecret(id)
}

/**
 * Saves an edited server, or adds one without connecting.
 *
 * A saved server's PIN isn't in the patch: `setPlacePinned` is the one writer
 * that moves one. A start folder outside the root answers
 * `start_folder_outside_root`, and nothing is written.
 */
export async function updateSavedServer(server: ServerTarget): Promise<SavedServerOutcome> {
  return await commands.updateSavedServer(server)
}
