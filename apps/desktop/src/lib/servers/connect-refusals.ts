/**
 * The words for every reason a connect or a saved edit can stop.
 *
 * ❗ A `Record` over `ConnectRefusalKind`, so a new reason can't reach a person
 * without someone writing its sentence, and every key appears here as a literal
 * (which is what keeps `desktop-message-keys-unused` honest without a dynamic
 * prefix allowlist).
 *
 * Writing rules: `docs/guides/error-handling.md` § "Writing rules". Never the
 * words "error" or "failed", always what the person can do about it, and ❌ never
 * a backend diagnostic ("PROPFIND", "Digest", "rung", "transport").
 */

import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/**
 * Why a connect stopped, in the vocabulary the app words.
 *
 * ❗ One kind per outcome that is a REASON, ❌ never collapsed:
 * `auth_method_unsupported` is not `authentication_rejected` (the server never
 * saw the secret, so "check your password" is the wrong fix), and
 * `needs_credentials` is not one either (telling someone who has never entered a
 * password that theirs is wrong is what collapsing the two does).
 *
 * It lives beside its sentences so a new kind and its words land together, and
 * so the pane and the sign-in sheet can't drift into two vocabularies.
 */
export type ConnectRefusalKind =
  | 'authentication_rejected'
  | 'needs_credentials'
  | 'auth_method_unsupported'
  | 'certificate_untrusted'
  | 'not_a_webdav_server'
  | 'invalid_url'
  | 'timed_out'
  | 'unreachable'
  | 'host_key_untrusted'
  | 'host_key_revoked'
  /** The start folder is neither the root folder nor inside it. A connect and a save both refuse it. */
  | 'start_folder_outside_root'
  /** A save to a connected place: the new root isn't a folder this account can open on the server. */
  | 'root_not_found'
  /** A save to a connected place: the start folder isn't a folder this account can open on the server. */
  | 'start_folder_not_found'
  /**
   * A save to a connected place needed the server to confirm a folder and it didn't answer in time.
   * ❗ Not `unreachable`: nothing was saved, and in edit mode the address that sentence points at is locked.
   */
  | 'save_unconfirmed'

const REFUSAL_KEYS: Record<ConnectRefusalKind, MessageKey> = {
  authentication_rejected: 'servers.refusal.authenticationRejected',
  needs_credentials: 'servers.refusal.needsCredentials',
  auth_method_unsupported: 'servers.refusal.authMethodUnsupported',
  certificate_untrusted: 'servers.refusal.certificateUntrusted',
  not_a_webdav_server: 'servers.refusal.notAWebdavServer',
  invalid_url: 'servers.refusal.invalidUrl',
  timed_out: 'servers.refusal.timedOut',
  unreachable: 'servers.refusal.unreachable',
  host_key_untrusted: 'servers.refusal.hostKeyUntrusted',
  host_key_revoked: 'servers.refusal.hostKeyRevoked',
  start_folder_outside_root: 'servers.refusal.startFolderOutsideRoot',
  root_not_found: 'servers.refusal.rootNotFound',
  start_folder_not_found: 'servers.refusal.startFolderNotFound',
  save_unconfirmed: 'servers.refusal.saveUnconfirmed',
}

/** What the place is called in a refusal: its host where there is one, else its name. */
export interface RefusalSubject {
  /** The server's own name, from the place's path. */
  host: string
  /** The account, for the sentence about a password that didn't work. */
  username: string
}

/** The one sentence a refusal says. */
export function wordConnectRefusal(kind: ConnectRefusalKind, subject: RefusalSubject): string {
  return tString(REFUSAL_KEYS[kind], { host: subject.host, username: subject.username })
}

/**
 * Which field a refusal belongs under, so the sentence lands beside the thing
 * the reader can change.
 *
 * ❗ A refusal floating at the top of a form is one a person reads as being about
 * the whole form. "That password didn't work" under the password field is an
 * instruction; the same words above the address are a puzzle.
 */
export type RefusalField =
  /** The password or passphrase. */
  | 'secret'
  /** The address, which in add mode is where a wrong endpoint is fixed. */
  | 'address'
  /** The root folder, the ceiling nothing navigates above. */
  | 'root'
  /** The start folder, where opening the place lands. */
  | 'start_folder'
  /** Nothing the user can retype. It reads above the buttons. */
  | 'form'

const REFUSAL_FIELDS: Record<ConnectRefusalKind, RefusalField> = {
  authentication_rejected: 'secret',
  needs_credentials: 'secret',
  // ❗ The secret was never sent, so the field is not where the fix is.
  auth_method_unsupported: 'form',
  certificate_untrusted: 'form',
  not_a_webdav_server: 'address',
  invalid_url: 'address',
  timed_out: 'address',
  unreachable: 'address',
  host_key_untrusted: 'form',
  host_key_revoked: 'form',
  // ❗ Each folder refusal under its OWN folder: "Cmdr can't open this folder"
  // under the start folder sends someone to retype the wrong path.
  start_folder_outside_root: 'start_folder',
  root_not_found: 'root',
  start_folder_not_found: 'start_folder',
  // No field fixes a server that didn't answer.
  save_unconfirmed: 'form',
}

/** Where `kind`'s sentence goes. */
export function refusalField(kind: ConnectRefusalKind): RefusalField {
  return REFUSAL_FIELDS[kind]
}
