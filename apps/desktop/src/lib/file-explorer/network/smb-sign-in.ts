/**
 * SMB's credential sites, on the one sign-in sheet.
 *
 * Three places ask an SMB server for a username and password: a share LISTING
 * that came back needing one (`PlacesBrowser`), a share MOUNT that was refused
 * (`NetworkMountView`), and the "Connect directly" upgrade of a share the OS
 * already mounted (`pane/smb-view-state.svelte.ts`). Each owns its own command;
 * all three ask the same question, so they ask it through here.
 *
 * ❗ **The sheet owns the form; the caller owns the protocol**
 * (`servers/DETAILS.md` § "The sheet contract"). This module is the translator
 * in between: it builds the SMB endpoint header, resolves the username the
 * server already knows, and turns the sheet's generic `SignInSubmission` into
 * the `(username, password, remember)` triple every SMB command takes. It ❌
 * never dials.
 *
 * ❗ **Remember starts ON, and is ❌ never probed.** `has_smb_credentials` is
 * `get_credentials(…).is_ok()`, so asking whether a password is stored costs the
 * same macOS Keychain prompt as reading one (`CLAUDE.md`'s never-pre-check
 * rule). The box is visible and unchecked with one click, and nothing is written
 * until a sign-in actually works, so a checked box promises nothing that hasn't
 * been shown.
 */

import { getKnownShareByName, getUsernameHint } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import type { MountError, ShareListError } from '../types'
import { openSignInSheet } from '$lib/servers/sign-in-sheet-state.svelte'
import type { ConnectRefusalKind } from '$lib/servers/connect-refusals'
import type {
  SignInAttemptOutcome,
  SignInEndpoint,
  SignInSheetResult,
  SignInSubmission,
} from '$lib/servers/sign-in-contract'

const log = getAppLogger('fileExplorer')

/** The server a sign-in is for, as little of a `NetworkHost` as the sheet needs. */
export interface SmbSignInHost {
  /** ❗ The stable Bonjour name: it is what the credential store and the hints are keyed by. */
  name: string
}

/**
 * What the user answered, in the shape every SMB command takes.
 *
 * ❗ `username: null` IS guest. The three commands all take a nullable username
 * and read it that way, which is why this doesn't carry a separate flag that
 * could disagree with it.
 */
export interface SmbCredentialAnswer {
  username: string | null
  password: string | null
  /** Write the credential to the Keychain — ❗ on SUCCESS only, and by the caller. */
  remember: boolean
}

export interface SmbSignInRequest {
  host: SmbSignInHost
  /** The share this is about, when it is about one. Listing auth is server-level. */
  shareName?: string
  /** Whether to offer "Connect as guest". ❗ `false` where guest has already been tried and refused. */
  guestAllowed: boolean
  /** A username an earlier attempt tried. Wins over what the store remembers. */
  initialUsername?: string
  /** Why the sheet is opening, so its first round already says so. */
  refusal?: ConnectRefusalKind
  /** One round-trip with the server, which the CALLER owns. */
  attempt: (answer: SmbCredentialAnswer) => Promise<SignInAttemptOutcome>
}

/** Opens the sheet for an SMB server, and resolves once the user is done with it. */
export async function openSmbSignInSheet(request: SmbSignInRequest): Promise<SignInSheetResult> {
  const username = await rememberedUsername(request)
  return await openSignInSheet({
    mode: 'sign-in',
    endpoint: smbEndpoint(request.host, request.shareName, username),
    // SMB's shape, always: the SHARE is the identity and the account is a field
    // on it, so the username stays editable and re-auth-as-someone-else works.
    shape: { kind: 'username_password', guestAllowed: request.guestAllowed },
    remembered: true,
    refusal: request.refusal,
    attempt: (submission) => request.attempt(answerFrom(submission)),
  })
}

/**
 * The account the server already knows, in the order the SMB form has always
 * resolved it: what an earlier attempt tried, then the share's own last
 * username, then the server-level hint.
 *
 * ❗ Both lookups take the server BY NAME and match on its stable identity in
 * Rust, so a hint saved under one spelling (`Naspolya`) is found when the sheet
 * opens under another (`Naspolya._smb._tcp.local`). ❌ Don't rebuild the key in
 * TypeScript: doing that is what made the two sides disagree.
 *
 * ❗ A lookup that breaks down answers `undefined` rather than throwing. This
 * runs BEFORE the sheet opens, and one of the three callers cannot await it
 * (`direct-connect.ts` returns as soon as the ask is up), so a rejection here
 * would be an unhandled one AND a sheet that never appeared — over a pre-fill
 * nobody would miss.
 */
async function rememberedUsername(request: SmbSignInRequest): Promise<string | undefined> {
  if (request.initialUsername) return request.initialUsername
  try {
    if (request.shareName) {
      const known = await getKnownShareByName(request.host.name, request.shareName)
      if (known?.username) return known.username
    }
    return (await getUsernameHint(request.host.name)) ?? undefined
  } catch (e) {
    log.warn('Reading the remembered username for {host} broke down: {error}', {
      host: request.host.name,
      error: String(e),
    })
    return undefined
  }
}

/** The read-only header: which server, and which share when it is about one. */
function smbEndpoint(host: SmbSignInHost, shareName: string | undefined, username: string | undefined): SignInEndpoint {
  return {
    protocol: 'smb',
    displayName: shareName ? `${host.name}/${shareName}` : host.name,
    address: shareName ? `smb://${host.name}/${shareName}` : `smb://${host.name}`,
    host: host.name,
    username,
  }
}

/** The sheet's generic answer, in SMB's own words. */
function answerFrom(submission: SignInSubmission): SmbCredentialAnswer {
  // ❗ Add mode never reaches an SMB sign-in site, and a submission with no
  // secret is the guest arm. Both come out as guest, which is the answer that
  // asks the server for nothing the user didn't offer.
  if (submission.mode !== 'sign-in' || submission.secret === null) {
    return { username: null, password: null, remember: false }
  }
  return {
    username: submission.username ?? '',
    password: submission.secret.secret,
    remember: submission.secret.remember,
  }
}

/**
 * A share LISTING that stopped, in the sheet's vocabulary.
 *
 * ❗ Only the two credential kinds keep the sheet open. Everything else is about
 * the SERVER, and the pane's own error state is where it lands, with the retry
 * and (for a missing dependency) the install command the sheet has no room for.
 */
export function refusalForShareError(error: ShareListError): ConnectRefusalKind {
  switch (error.type) {
    case 'auth_failed':
      return 'authentication_rejected'
    case 'auth_required':
    case 'signing_required':
      return 'needs_credentials'
    case 'timeout':
      return 'timed_out'
    default:
      return 'unreachable'
  }
}

/**
 * A share MOUNT that stopped, in the same vocabulary.
 *
 * ❗ `auth_required` is ❌ NOT `auth_failed`: telling someone who has never
 * entered a password that theirs is wrong is what collapsing the two does.
 * `auth_required` covers the NetAuth -6600 code the backend maps.
 */
export function refusalForMountError(error: MountError): ConnectRefusalKind {
  return error.type === 'auth_required' ? 'needs_credentials' : 'authentication_rejected'
}

/** Whether a mount failure is a credential question, so the sheet is what answers it. */
export function isMountAuthError(error: MountError): boolean {
  return error.type === 'auth_failed' || error.type === 'auth_required'
}

/**
 * Whether a LISTING failure is a credential question.
 *
 * ❗ `signing_required` counts: the server wants a signed session, which it only
 * grants an authenticated one, so the fix is the same one the user can offer.
 */
export function isListingAuthError(error: ShareListError): boolean {
  return error.type === 'auth_required' || error.type === 'signing_required'
}
