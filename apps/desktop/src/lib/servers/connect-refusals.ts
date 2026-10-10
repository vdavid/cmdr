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
import type { ServerProtocol, UnreachableHint } from '$lib/ipc/bindings'
import { systemStrings } from '$lib/system-strings.svelte'

/**
 * Why a connect stopped, in the vocabulary the app words.
 *
 * ❗ One kind per outcome that is a REASON, ❌ never collapsed:
 * `auth_method_unsupported` is not `authentication_rejected` (the server never
 * saw the secret, so "check your password" is the wrong fix), and
 * `needs_credentials` is not one either (telling someone who has never entered a
 * password that theirs is wrong is what collapsing the two does).
 * `account_not_permitted` is a third one: the account SIGNED IN, and the place
 * (an SMB share, and whatever a later protocol's equivalent is) turned it away,
 * so the password is the one thing known to be right.
 *
 * It lives beside its sentences so a new kind and its words land together, and
 * so the pane and the sign-in sheet can't drift into two vocabularies.
 */
export type ConnectRefusalKind =
  | 'authentication_rejected'
  | 'needs_credentials'
  | 'account_not_permitted'
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
  /**
   * Sign-in mode: Remember went on and the Keychain wouldn't store the password, so the round never dialed.
   * ❗ Not `authentication_rejected`: no server was asked anything.
   */
  | 'secret_not_stored'
  /**
   * Edit mode: the settings saved, then the Keychain wouldn't store or forget the password.
   * ❗ Not `unreachable`: the edit landed and no server was contacted.
   */
  | 'saved_secret_not_updated'
  /**
   * Add mode: the password field was empty and the store holds none for the account, so nothing was dialed
   * (WebDAV asks the store before it dials). ❗ Not `needs_credentials`, whose sentence reads as a server's answer.
   */
  | 'password_missing'
  /**
   * S3: the bucket refused this key, which is a wrong secret OR a key without rights here (a bodyless 403
   * can't say which), so its words ask about both.
   */
  | 'access_denied'
  /**
   * S3: the account root needs to list buckets and this key may not (on some servers, a wrong secret answers
   * the same). The way in is typing a bucket.
   */
  | 'bucket_list_refused'
  /** S3: no bucket by that name on this endpoint. */
  | 'bucket_not_found'
  /** S3: the bucket lives in another region than the one chosen. Names it when the server did (`RefusalSubject.region`). */
  | 'region_mismatch'
  /** S3: this Mac's clock is too far off for the server to accept a signature. */
  | 'clock_skewed'
  /** S3: the address answers, but not as S3. */
  | 'not_an_s3_endpoint'
  /**
   * Add mode, before any round trip: an S3 region, location, or account ID holds something besides
   * `a–z 0–9 -`, which no host name can carry.
   */
  | 's3_field_malformed'
  /** Add mode, before any round trip: the "Other S3-compatible" endpoint isn't `http(s)://host[:port]`. */
  | 'endpoint_malformed'
  /**
   * Edit mode: the new address is already another saved server's (`RefusalSubject.takenBy` names it). Refused
   * rather than merged, and nothing was saved.
   */
  | 'address_taken'
  /**
   * Edit mode: moving the server to its new address couldn't take its saved password along, so nothing moved.
   * ❗ Not `saved_secret_not_updated`, whose edit DID land.
   */
  | 'secret_not_moved'
  /** Edit mode: the edit named another account or protocol. The sheet locks both, so only a broken caller sees it. */
  | 'account_changed'
  /**
   * Edit mode: a copy, move, or delete is queued, running, or paused on the server, and moving it to its new address
   * would drop the session under it. Nothing was saved.
   */
  | 'operation_running'

const REFUSAL_KEYS: Record<ConnectRefusalKind, MessageKey> = {
  authentication_rejected: 'servers.refusal.authenticationRejected',
  needs_credentials: 'servers.refusal.needsCredentials',
  account_not_permitted: 'servers.refusal.accountNotPermitted',
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
  secret_not_stored: 'servers.refusal.secretNotStored',
  saved_secret_not_updated: 'servers.refusal.savedSecretNotUpdated',
  password_missing: 'servers.refusal.passwordMissing',
  access_denied: 'servers.refusal.accessDenied',
  bucket_list_refused: 'servers.refusal.bucketListRefused',
  bucket_not_found: 'servers.refusal.bucketNotFound',
  region_mismatch: 'servers.refusal.regionMismatch',
  clock_skewed: 'servers.refusal.clockSkewed',
  not_an_s3_endpoint: 'servers.refusal.notAnS3Endpoint',
  s3_field_malformed: 'servers.refusal.s3FieldMalformed',
  endpoint_malformed: 'servers.refusal.endpointMalformed',
  address_taken: 'servers.refusal.addressTaken',
  secret_not_moved: 'servers.refusal.secretNotMoved',
  account_changed: 'servers.refusal.accountChanged',
  operation_running: 'servers.refusal.operationRunning',
}

/**
 * The sentences that say "password", worded for an S3 account, which has a secret
 * access key instead. ❗ A sentence asking for a password sends the reader looking
 * for something their provider never gave them.
 */
const S3_REFUSAL_KEYS: Partial<Record<ConnectRefusalKind, MessageKey>> = {
  authentication_rejected: 'servers.refusal.s3AuthenticationRejected',
  needs_credentials: 'servers.refusal.s3NeedsCredentials',
  password_missing: 'servers.refusal.s3SecretMissing',
  secret_not_stored: 'servers.refusal.s3SecretNotStored',
  saved_secret_not_updated: 'servers.refusal.s3SavedSecretNotUpdated',
  secret_not_moved: 'servers.refusal.s3SecretNotMoved',
}

/** What the place is called in a refusal: its host where there is one, else its name. */
export interface RefusalSubject {
  /** The server's own name, from the place's path. */
  host: string
  /** The account, for the sentence about a password that didn't work. */
  username: string
  /** Which protocol is asking, where its words differ: an S3 account has a secret access key, not a password. */
  protocol?: ServerProtocol
  /** `region_mismatch` only: the region the server says the bucket lives in, when it said. */
  region?: string | null
  /** `address_taken` only: what the saved server already at that address is called. */
  takenBy?: string | null
}

/** The one sentence a refusal says. */
export function wordConnectRefusal(kind: ConnectRefusalKind, subject: RefusalSubject): string {
  if (kind === 'region_mismatch' && subject.region) {
    return tString('servers.refusal.regionMismatchNamed', { region: subject.region })
  }
  if (kind === 'address_taken') {
    return tString('servers.refusal.addressTaken', { name: subject.takenBy ?? subject.host })
  }
  const key = (subject.protocol === 's3' ? S3_REFUSAL_KEYS[kind] : undefined) ?? REFUSAL_KEYS[kind]
  return tString(key, { host: subject.host, username: subject.username })
}

/** A saved place in a pane, which also has the name the user gave it. */
export interface PaneRefusalSubject extends RefusalSubject {
  /** The place's display name. Empty falls back to the host. */
  name: string
}

/**
 * The sentence a pane standing on a saved place says. ❗ `unreachable` names the
 * place by the name the user gave it ("Naspolya", not "nas.local"); the sheet and
 * the Add form keep the host, since there it's the address the person typed.
 */
export function wordPaneRefusal(kind: ConnectRefusalKind, subject: PaneRefusalSubject): string {
  if (kind === 'unreachable') {
    return tString('servers.paneState.unreachable', { name: subject.name.trim() || subject.host })
  }
  return wordConnectRefusal(kind, subject)
}

/**
 * Something besides the server worth checking, which the backend reads off how a
 * probe failed. Rendered as a softer line under the refusal, ❌ never instead of
 * it: a hint only suggests (a stuck Local Network permission and a server that's
 * off look the same to the Add probe).
 */
export type RefusalHint = UnreachableHint

const HINT_KEYS: Record<RefusalHint, MessageKey> = {
  local_network_permission: 'servers.refusal.localNetworkHint',
}

/** The softer line under a refusal. The permission's name is the one System Settings shows. */
export function wordRefusalHint(hint: RefusalHint): string {
  return tString(HINT_KEYS[hint], { localNetwork: systemStrings.localNetwork })
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
  /**
   * The address, which in add mode is where a wrong endpoint is fixed. For S3, whose form has no address,
   * the field that makes the endpoint: the Other endpoint URL, else the preset's region, location, or account ID.
   */
  | 'address'
  /** S3: the region (or a preset's location or account ID), which also picks the endpoint for a preset. */
  | 'region'
  /** S3: the bucket field, where typing a bucket is the way past a key that can't list buckets. */
  | 'bucket'
  /** The root folder, the ceiling nothing navigates above. */
  | 'root'
  /** The start folder, where opening the place lands. */
  | 'start_folder'
  /** Nothing the user can retype. It reads above the buttons. */
  | 'form'

const REFUSAL_FIELDS: Record<ConnectRefusalKind, RefusalField> = {
  authentication_rejected: 'secret',
  needs_credentials: 'secret',
  password_missing: 'secret',
  // ❗ The password WORKED, so marking its field invalid would point at the one
  // thing that isn't wrong. The fix is another account.
  account_not_permitted: 'form',
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
  // The password is the one thing that didn't land, so its field is where the retry happens.
  secret_not_stored: 'secret',
  saved_secret_not_updated: 'secret',
  // ❗ Under the secret, ❌ not the form: a wrong secret is one of the two things it can mean, and the
  // sentence asks about the key's rights too.
  access_denied: 'secret',
  // Typing a bucket is the way past both.
  bucket_list_refused: 'bucket',
  bucket_not_found: 'bucket',
  region_mismatch: 'region',
  s3_field_malformed: 'region',
  not_an_s3_endpoint: 'address',
  endpoint_malformed: 'address',
  // No field fixes this Mac's clock.
  clock_skewed: 'form',
  // The address is what to change: to one nothing else holds.
  address_taken: 'address',
  // The password is the one thing that didn't move, so its field is where the retry happens.
  secret_not_moved: 'secret',
  account_changed: 'form',
  // No field fixes a copy that's still running: letting it finish or canceling it does.
  operation_running: 'form',
}

/** Where `kind`'s sentence goes. */
export function refusalField(kind: ConnectRefusalKind): RefusalField {
  return REFUSAL_FIELDS[kind]
}

/**
 * The refusal a sign-in sheet shows the moment it opens, or `null` for a clean form.
 *
 * ❗ Red is feedback on something the person did. `needs_credentials` is only the
 * reason the sheet is asking, which its title already says, so it opens clean and
 * the sentence waits for a round the person sent (cmdr-reports#10). Every other
 * kind is about an answer already given (a stored password turned away, an
 * account the share refused), so it shows from the start.
 */
export function refusalShownOnOpen(kind: ConnectRefusalKind | undefined): ConnectRefusalKind | null {
  if (kind === undefined || kind === 'needs_credentials') return null
  return kind
}
