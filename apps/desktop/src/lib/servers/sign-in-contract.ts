/**
 * What the sign-in sheet is asked for, what it hands back, and the one
 * round-trip in between.
 *
 * ❗ **The sheet owns the form; the caller owns the protocol.** The sheet asks
 * what a `SignInShape` says to ask, renders the refusal under the field it is
 * about, and calls [`SignInAttempt`] as many times as the user retries. It ❌
 * never dials: that is what lets SMB's three sites (a share listing, a share
 * mount, a reconnect) reuse it with their own commands, and what lets S3 plug in
 * with one more renderer.
 *
 * ❗ **The sheet stays open across rounds.** A first connect to a new SFTP server
 * is three round-trips (host key, then credentials, then connected), and a sheet
 * that closed and reopened between them would lose what the user typed and put
 * the refusal somewhere other than under the field it belongs to.
 *
 * Its own module so `connect-flow.ts`, `open-sign-in.ts`, and the component can
 * all name these without importing each other in a circle.
 */

import type {
  HostKeyPrompt,
  SecretOffer,
  SavedServer,
  ServerProtocol,
  ServerTarget,
  SftpHostKeyIdentity,
  SignInShape,
} from '$lib/ipc/bindings'
import type { ConnectRefusalKind } from './connect-refusals'

/** The server a sign-in is FOR, as its read-only header spells it. */
export interface SignInEndpoint {
  protocol: ServerProtocol
  /** The server's own name, for the title. */
  displayName: string
  /** What the header shows: `ada@nas.local:22`, or a base URL. */
  address: string
  /** The host on its own, for the refusals that name the server rather than the account. */
  host: string
  /** The account, where the protocol has one. */
  username?: string
}

/**
 * What the sheet collected, handed to the caller's attempt.
 *
 * Two arms because the two modes collect different things: add mode types a
 * whole server, sign-in mode answers a question about one that exists.
 */
export type SignInSubmission =
  | { mode: 'add'; target: ServerTarget; secret: SecretOffer | null }
  /**
   * ❗ SMB's add path has no target: its connect is a share MOUNT rather than a
   * session, so the caller injects a manual host and opens its places, and no
   * credential is asked until a listing or a mount refuses.
   */
  | { mode: 'add_smb'; address: string }
  | {
      mode: 'sign-in'
      /**
       * ❗ `null` means GUEST: the shape offered it and the user picked it. It
       * cannot mean "nothing typed" — the sheet's submit stays disabled until
       * one or the other is true.
       */
      secret: SecretOffer | null
      /**
       * ❗ Present only when the SHAPE says the username is editable
       * (`username_password`) AND the user is signing in as themselves. SFTP and
       * WebDAV refuse a changed username because the volume id IS the account,
       * and a guest has no account to send.
       */
      username: string | null
    }

/**
 * How one round-trip ended, in the app's own words.
 *
 * ❗ The two host-key arms carry their payloads rather than collapsing into a
 * refusal: the sheet's key step needs the fingerprint to show, and a refusal
 * sentence has nowhere to put one.
 */
export type SignInAttemptOutcome =
  | { kind: 'connected'; volumeId: string }
  /**
   * The caller took it from here and the sheet closes with nothing more to say.
   *
   * Two shapes of that: SMB's add path, which lands in the places browser rather
   * than on a volume, and an SMB site whose round ended somewhere the sheet has
   * no words for — a share list that loaded, a mount that went through, or a
   * refusal about the SHARE rather than the credential, which the pane renders
   * with its own retry.
   */
  | { kind: 'handed_off' }
  | { kind: 'needs_host_key'; prompt: HostKeyPrompt }
  | { kind: 'host_key_revoked'; key: SftpHostKeyIdentity }
  | { kind: 'refused'; refusal: ConnectRefusalKind }
  /** The user pressed Cancel. ❗ Says nothing: they know. */
  | { kind: 'cancelled' }

/**
 * One round-trip with the backend.
 *
 * ❗ It must arm its own cancel before it dials: a dial runs up to 30 s, and the
 * sheet's Cancel has nothing to aim at until the attempt id exists.
 */
export type SignInAttempt = (submission: SignInSubmission) => Promise<SignInAttemptOutcome>

/** What the sheet was opened to do. */
export type SignInSheetRequest =
  /** Type a new server. Address first, protocol second (`address-parser.ts`). */
  | {
      mode: 'add'
      attempt: SignInAttempt
      /**
       * An address the caller already has, from go-to-path or a pasted link, as
       * the user spelled it. ❗ The raw string, ❌ not a parse: the field shows
       * what they had, and the sheet re-reads it the same way a keystroke does.
       */
      prefill?: string
    }
  /** Answer a server that is asking. The endpoint is a read-only header. */
  | {
      mode: 'sign-in'
      attempt: SignInAttempt
      endpoint: SignInEndpoint
      /** What to ask, from the backend. ❌ Never derived from a protocol or a rung. */
      shape: SignInShape
      /** Opens on the key step instead of the fields, when a key is what's in the way. */
      hostKey?: HostKeyPrompt
      /**
       * Where the Remember box starts.
       *
       * ❗ The OPENER decides, ❌ never the sheet: an SFTP place is asked
       * (`hasServerSecret`), and an SMB host is ❌ never asked at all, because
       * every read of its Keychain entry can cost a system prompt
       * (`file-explorer/network/CLAUDE.md`'s never-pre-check rule). ❗ For the
       * protocols the backend can seed a secret for, an attended sign-in
       * REFRESHES a remembered one and never seeds one, so passing `true` there
       * would seed a secret the user already declined.
       */
      remembered: boolean
      /**
       * The refusal that opened the sheet, so the first round already says why
       * a person is being asked. ❗ A typed kind, ❌ never a backend sentence.
       */
      refusal?: ConnectRefusalKind
    }
  /** Change a saved server: the add form prefilled, plus the two switches. */
  | { mode: 'edit'; server: SavedServer }

/** How the sheet closed. */
export type SignInSheetResult =
  /** A live volume under this id. */
  | { kind: 'connected'; volumeId: string }
  /** The caller took over (SMB's add path opens a places list, not a volume). */
  | { kind: 'handed_off' }
  /** Edit mode: the changes are written. */
  | { kind: 'saved' }
  /** Escape, Cancel, or the × . ❗ Says nothing. */
  | { kind: 'cancelled' }
