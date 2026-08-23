/**
 * The words for a typed `EjectError`: what an eject or a network-share
 * disconnect says to the person who asked for it.
 *
 * Classification is the backend's (`file_system/volume/eject.rs`); the words are
 * all here, pulled from the `errors.eject.*` catalog so every locale gets its
 * own. Same split as the mutation path
 * (`$lib/file-operations/mutation-error-messages.ts`);
 * `docs/guides/error-handling.md` is the map.
 *
 * These render as ONE plain-text line inside a toast, so each message is a
 * single sentence with no markdown and no `{@html}`. Copy is pulled via
 * `getMessage()` (a RAW catalog lookup, never ICU `t()`) for the same reason the
 * mutation factory does it: the surrounding toast interpolates uncontrolled
 * volume names, whose apostrophes and braces collide with ICU grammar.
 *
 * ❌ `diskutil`'s own stderr is NEVER the message. {@link ejectTechnicalDetail}
 * returns it separately, for the log.
 */
import type { EjectError } from '$lib/ipc/bindings'
import { getMessage } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'
import { getAppLogger } from '$lib/logging/logger'
import { asEjectError } from './eject-error'

const log = getAppLogger('eject')

/** Raw catalog lookup for an `errors.eject.*` key. */
function raw(key: MessageKey): string {
  return getMessage(key)
}

/**
 * One renderer per `EjectError` variant.
 *
 * A record rather than a `switch`: the mapped type still demands every variant,
 * so a backend that grows one stops the frontend compiling until it has words.
 */
const EJECT_MESSAGE: { [K in EjectError['type']]: () => string } = {
  busy: () => raw('errors.eject.busy'),
  volumeNotFound: () => raw('errors.eject.volumeNotFound'),
  mtpIdMissingDevicePrefix: () => raw('errors.eject.mtpIdMissingDevicePrefix'),
  notEjectable: () => raw('errors.eject.notEjectable'),
  notAnSmbVolume: () => raw('errors.eject.notAnSmbVolume'),
  mtpDisconnectRefused: () => raw('errors.eject.mtpDisconnectRefused'),
  unmountRefused: () => raw('errors.eject.unmountRefused'),
  timedOut: () => raw('errors.eject.timedOut'),
  // The single honest fallback. ❌ `detail` is never the message; it goes to
  // `ejectTechnicalDetail()`.
  unexpected: () => raw('errors.eject.unexpected'),
}

/** The one sentence an `EjectError` says. */
export function renderEjectError(error: EjectError): string {
  return EJECT_MESSAGE[error.type]()
}

/** `EjectError` variants that carry their own free-text `detail` field. */
const DETAIL_BEARING_VARIANTS = new Set<EjectError['type']>(['unmountRefused', 'mtpDisconnectRefused', 'unexpected'])

/**
 * The backend's own words for this refusal (usually `diskutil`'s stderr, which
 * often names the process holding the drive), for the log.
 *
 * ❌ Never render this as the message: it's untranslated diagnostic text.
 */
export function ejectTechnicalDetail(error: EjectError): string | null {
  return DETAIL_BEARING_VARIANTS.has(error.type) ? (error as { detail: string }).detail : null
}

/**
 * The one sentence a caught eject / disconnect refusal says, with the backend's
 * technical detail routed to the log.
 *
 * The three toasts that word an eject share this so none of them can drift into
 * putting `diskutil`'s untranslated stderr in front of a person. A value that
 * isn't an `EjectError` at all (the IPC transport itself broke) reads as the
 * same honest fallback, with the raw value logged.
 */
export function wordEjectRefusal(error: unknown): string {
  const typed = asEjectError(error)
  if (!typed) {
    log.warn('Eject refused with an untyped value: {error}', { error: String(error) })
    return raw('errors.eject.unexpected')
  }
  const detail = ejectTechnicalDetail(typed)
  if (detail === null) log.warn('Eject refused: {reason}', { reason: typed.type })
  else log.warn('Eject refused: {reason} ({detail})', { reason: typed.type, detail })
  return renderEjectError(typed)
}
