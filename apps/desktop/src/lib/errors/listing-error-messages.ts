/**
 * Listing-path friendly-error copy: `reason` + params → user-facing message.
 *
 * The Rust backend classifies a listing/empty-root failure into a typed
 * `ListingErrorReason` plus structured params and ships them over IPC; this
 * factory owns the WORDS. One reason per currently-distinct message, mirroring
 * the old Rust `errno.rs` / `kinds.rs` / `empty_root.rs` arms verbatim (a
 * behavior-preserving move, not a copy redesign).
 *
 * The literal English lives in the `errors.listing.*` message catalog and is
 * pulled via `getMessage()` (a RAW catalog lookup, never ICU `t()`): these
 * strings carry markdown plus `{system_settings}`-style tokens and bypass ICU's
 * brace/apostrophe grammar. Each catalog value may carry `{path}` / `{osMessage}`
 * param tokens; `interpolate(...)` substitutes the escaped runtime value, and
 * `expandSystemStrings(...)` swaps the localized macOS pane labels. See
 * `compose.ts` and `$lib/intl`'s docs.
 */

import type { FriendlyErrorMessage } from './friendly-error-message'
import { esc, expandSystemStrings } from './compose'
import { getMessage } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/**
 * Substitutes `{path}` / `{osMessage}` param tokens in a catalog value with the
 * (already-escaped) runtime values. Disjoint from `expandSystemStrings`' token
 * set, so the two compose in any order. Tokens absent from `params` are left
 * untouched (a reason that carries no path leaves no `{path}` in its catalog
 * value anyway).
 */
function interpolate(template: string, params: Record<string, string>): string {
  let out = template
  for (const [name, value] of Object.entries(params)) {
    out = out.replaceAll(`{${name}}`, value)
  }
  return out
}

/**
 * Pulls the title / explanation / suggestion for a listing reason from the
 * catalog, interpolates the escaped params, and expands the system-string
 * tokens. Centralizes the compose pipeline so every arm is one declarative call.
 */
function compose(reason: string, params: Record<string, string> = {}): FriendlyErrorMessage {
  const key = (part: string) => `errors.listing.${reason}.${part}` as MessageKey
  return {
    title: getMessage(key('title')),
    message: expandSystemStrings(interpolate(getMessage(key('explanation')), params)),
    suggestion: expandSystemStrings(interpolate(getMessage(key('suggestion')), params)),
  }
}

/**
 * Typed listing-error classification from the backend. Variant-carried params
 * keep impossible combinations unrepresentable. Serialized camelCase from Rust
 * (`#[serde(tag = "reason", ...)]`).
 */
export type ListingErrorReason =
  // ── errno: transient ──
  | { reason: 'interrupted' }
  | { reason: 'notEnoughMemory' }
  | { reason: 'resourceBusy'; path: string }
  | { reason: 'temporarilyUnavailable' }
  | { reason: 'networkDown' }
  | { reason: 'networkConnectionDropped' }
  | { reason: 'connectionDropped' }
  | { reason: 'connectionReset' }
  | { reason: 'connectionTimedOutErrno' }
  | { reason: 'hostDown' }
  | { reason: 'staleConnection' }
  | { reason: 'lockUnavailable' }
  | { reason: 'cancelledErrno' }
  // ── errno: needs-action ──
  | { reason: 'notPermitted'; path: string }
  | { reason: 'pathNotFoundErrno'; path: string }
  | { reason: 'noPermissionErrno'; path: string }
  | { reason: 'alreadyExistsErrno'; path: string }
  | { reason: 'crossDeviceOperation' }
  | { reason: 'notAFolder'; path: string }
  | { reason: 'isAFolderErrno'; path: string }
  | { reason: 'diskFullErrno' }
  | { reason: 'readOnlyVolumeErrno' }
  | { reason: 'notSupportedErrno' }
  | { reason: 'networkUnreachable' }
  | { reason: 'connectionRefused' }
  | { reason: 'symlinkLoopErrno'; path: string }
  | { reason: 'nameTooLongErrno' }
  | { reason: 'hostUnreachable' }
  | { reason: 'folderNotEmpty'; path: string }
  | { reason: 'quotaExceeded' }
  | { reason: 'authRequiredEauth' }
  | { reason: 'authRequiredEneedauth' }
  | { reason: 'devicePoweredOff' }
  | { reason: 'attributeNotFound' }
  // ── errno: serious ──
  | { reason: 'diskReadProblem'; path: string }
  | { reason: 'unexpectedSystemResponse' }
  | { reason: 'deviceProblem' }
  | { reason: 'couldntReadUnknown'; path: string }
  // ── typed VolumeError variants (shared "kinds") ──
  | { reason: 'notFound'; path: string }
  | { reason: 'tccRestricted'; path: string }
  | { reason: 'permissionDenied'; path: string }
  | { reason: 'alreadyExists'; path: string }
  | { reason: 'cancelled' }
  | { reason: 'deviceDisconnected'; path: string }
  | { reason: 'deviceReconnecting'; path: string }
  | { reason: 'readOnly' }
  | { reason: 'storageFull' }
  | { reason: 'connectionTimedOut' }
  | { reason: 'notSupported' }
  | { reason: 'deletePending'; path: string }
  | { reason: 'ioSerious'; path: string; osMessage: string }
  | { reason: 'isADirectory'; path: string }
  // ── archive (browsing a `.zip` that can't be read) ──
  | { reason: 'archiveUnreadable' }
  // ── archive (a header-encrypted archive needs its password even to list it) ──
  | { reason: 'archiveNeedsPassword'; wrongAttempt: boolean }
  // ── empty-root hint ──
  | { reason: 'emptyRootICloud' }

/**
 * Maps a classified listing reason to its user-facing message. The literal copy
 * lives in `errors.listing.*`; this fn keys on the reason and feeds the escaped
 * params into `compose`. A reason's variant-carried params (`path`, `osMessage`)
 * are escaped here and passed as token values; param-free reasons pass none. The
 * variant shape is the source of truth for which params exist, so the params are
 * read straight off `r` rather than re-enumerated per reason.
 */
export function getListingErrorMessage(r: ListingErrorReason): FriendlyErrorMessage {
  const params: Record<string, string> = {}
  if ('path' in r) params.path = esc(r.path)
  if ('osMessage' in r) params.osMessage = esc(r.osMessage)
  return compose(r.reason, params)
}
