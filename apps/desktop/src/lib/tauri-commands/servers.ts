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
  SavedPlaceRefusal,
  SavedServer,
  SavedServerOutcome,
  SecretOffer,
  ServerConnectOutcome,
  ServerProtocol,
  ServerTarget,
} from '$lib/ipc/bindings'
import { TypedFailure, failureOf } from '$lib/ipc/typed-failure'
import { throwKeychainError } from '$lib/servers/keychain-failure'

export type {
  SavedPlace,
  SavedPlaceRefusal,
  SavedServer,
  SavedServerOutcome,
  SecretOffer,
  ServerConnectOutcome,
  ServerProtocol,
  ServerTarget,
}

/**
 * An `Error` that still carries `connect_saved_place`'s typed refusal.
 *
 * ❗ Both refusals mean the row the dial was picked from went stale:
 * `volumes-changed` is debounced, so another pane's dial or a forget can land in
 * between. Each caller maps them to a MOVE (reload, close), ❌ never a sentence.
 */
export class SavedPlaceFailure extends TypedFailure<SavedPlaceRefusal> {
  constructor(failure: SavedPlaceRefusal) {
    super(failure, `saved place refused: ${failure.reason} (${failure.volumeId})`)
    this.name = 'SavedPlaceFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asSavedPlaceRefusal(error: unknown): SavedPlaceRefusal | null {
  return failureOf(SavedPlaceFailure, error)
}

/**
 * Every server the user has saved, across all three stores.
 *
 * Cached state only, so calling it on a `volumes-changed` refresh costs no
 * network traffic. An SMB host's places are its saved shares, which carry the
 * pins; the host row never does. See the type's own note.
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
 * a second volume. `servers/connect-flow.ts` is what picks between them; a stale
 * pick throws a {@link SavedPlaceFailure}, which a user should never see.
 *
 * `username` is the sheet's account field, which only an SMB share's sheet has;
 * SFTP, WebDAV, and S3 ignore it.
 */
export async function connectSavedPlace(
  volumeId: string,
  attemptId: string,
  secret: SecretOffer | null = null,
  username: string | null = null,
): Promise<ServerConnectOutcome> {
  const result = await commands.connectSavedPlace(volumeId, attemptId, secret, username)
  if (result.status === 'error') throw new SavedPlaceFailure(result.error)
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
 * Moves a saved place's "Reconnect automatically" switch, answering whether a
 * saved place was there. Moves the stored switch AND a connected volume's live
 * one, the same two copies the sign-in sheet's save moves, but touches no other
 * field, so a row menu can't clobber a sheet edit. Emits `volumes-changed`.
 */
export async function setPlaceAutoReconnect(volumeId: string, autoReconnect: boolean): Promise<boolean> {
  return await commands.setPlaceAutoReconnect(volumeId, autoReconnect)
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
 *
 * `editing` is the id of the saved place the edit sheet opened on, `null` for
 * "Add anyway". ❗ The backend decides whether the edit moved the address (and
 * moves the server, its password, and its favorites if so), so a caller never
 * compares addresses itself. Required, so no caller forgets which it is.
 */
export async function updateSavedServer(server: ServerTarget, editing: string | null): Promise<SavedServerOutcome> {
  return await commands.updateSavedServer(server, editing)
}

/**
 * The id the servers listing gives the account `server` names, minted by the
 * backend's own id funnel, or `null` for a WebDAV address that isn't a URL.
 * ❗ How to find the row a save made: the stores normalize addresses, so a typed
 * spelling can miss it.
 */
export async function savedServerId(server: ServerTarget): Promise<string | null> {
  return await commands.savedServerId(server)
}

/**
 * Forgets the saved SMB host the listing calls `id`: its manual entry, its
 * sign-in history, and every share saved under it, exactly the rows the listing
 * showed under it. Nothing is unmounted and no password is touched. Answers
 * whether anything was there.
 *
 * ❗ The id alone, never an address: the backend finds the host in the same
 * listing the row came from, so a Forget can't reach a host the row didn't show.
 */
export async function forgetSavedSmbHost(id: string): Promise<boolean> {
  return await commands.forgetSavedSmbHost(id)
}

/**
 * Forgets every password stored for the saved SMB host the listing calls `id`
 * (Forget server's "Also forget the saved password"), answering whether one was
 * there. ❗ Call it BEFORE `forgetSavedSmbHost`, which takes away the rows its
 * names come from. A store that refuses throws a `KeychainFailure`.
 */
export async function forgetSavedSmbHostPassword(id: string): Promise<boolean> {
  const res = await commands.forgetSavedSmbHostPassword(id)
  if (res.status === 'error') throwKeychainError(res.error)
  return res.data
}

/**
 * Names the saved SMB host the listing calls `id` and sets the account it's used
 * with; an empty name unnames it and a `null` account clears it. Answers whether
 * there was a host to name. The address never changes here, and it isn't passed:
 * the backend reads it off the same listing the row came from.
 */
export async function updateSavedSmbHost(id: string, name: string, username: string | null): Promise<boolean> {
  return await commands.updateSavedSmbHost(id, name, username)
}

/**
 * Names the saved S3 account the listing calls `id` (its row's id); an empty name
 * unnames it, so it reads as `key id@host` again. Answers whether any saved place
 * belongs to it. The account carries the name: a bucket reads as its own.
 */
export async function updateSavedS3Account(id: string, name: string): Promise<boolean> {
  return await commands.updateSavedS3Account(id, name)
}
