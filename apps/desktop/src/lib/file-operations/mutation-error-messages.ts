/**
 * The words for a typed `MutationError`: what a rename, New Folder, or New File
 * refusal says to the person who asked for it.
 *
 * Classification is the backend's (`write_operations/mutation_error.rs`); the
 * words are all here, pulled from the `errors.mutation.*` and `errors.volume.*`
 * catalogs so every locale gets its own. Same split as the listing path
 * (`$lib/error-messages/`) and the transfer path
 * (`./transfer/transfer-error-messages.ts`); `docs/guides/error-handling.md` is
 * the map.
 *
 * These render as ONE plain-text line, inline under the field the user is
 * standing in or in a toast, so each message is a single sentence with no
 * markdown and no `{@html}`. Copy is pulled via `getMessage()` (a RAW catalog
 * lookup, never ICU `t()`) because the values interpolate uncontrolled
 * filenames, whose apostrophes and braces would collide with ICU grammar. The
 * three shared-with-live-validation cases (`nameEmpty`,
 * `nameHasDisallowedCharacter`, `alreadyExists`) deliberately reuse the
 * `fileOperations.validation.*` keys through ICU `tString()`, so a name the
 * backend turns down reads exactly the way the red border already read.
 *
 * `Unexpected.detail` and the backend diagnostics inside a `VolumeError` are
 * NEVER the message: `technicalDetail()` returns them separately, for a details
 * disclosure or the log.
 */
import type { MutationError, VolumeError } from '$lib/ipc/bindings'
import { getMessage, tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'
import { getGitErrorMessage } from '$lib/error-messages/git-error-messages'

/** What is being named, so the reused validation copy agrees grammatically. */
export type NamedKind = 'file' | 'folder'

/** Substitutes `{token}` placeholders in a catalog value with runtime strings. */
function interpolate(template: string, params: Record<string, string>): string {
  let out = template
  for (const [name, value] of Object.entries(params)) out = out.replaceAll(`{${name}}`, value)
  return out
}

/** Raw catalog lookup for an `errors.mutation.*` / `errors.volume.*` key. */
function raw(key: string, params?: Record<string, string>): string {
  const value = getMessage(key as MessageKey)
  return params ? interpolate(value, params) : value
}

/**
 * One renderer per `VolumeError` variant.
 *
 * A record rather than a `switch`: the mapped type still demands every variant
 * (drop one and this stops compiling), and a table of one-liners reads better
 * than a twenty-arm switch, which the complexity lint rightly objects to.
 */
const VOLUME_MESSAGE: { [K in VolumeError['type']]: (error: Extract<VolumeError, { type: K }>) => string } = {
  notFound: (e) => raw('errors.volume.notFound', { path: e.data }),
  permissionDenied: (e) => raw('errors.volume.permissionDenied', { path: e.data }),
  alreadyExists: (e) => raw('errors.volume.alreadyExists', { path: e.data }),
  notSupported: () => raw('errors.volume.notSupported'),
  deviceDisconnected: () => raw('errors.volume.deviceDisconnected'),
  deviceSessionReset: () => raw('errors.volume.deviceSessionReset'),
  readOnly: () => raw('errors.volume.readOnly'),
  storageFull: () => raw('errors.volume.storageFull'),
  connectionTimeout: () => raw('errors.volume.connectionTimeout'),
  cancelled: () => raw('errors.volume.cancelled'),
  isADirectory: (e) => raw('errors.volume.isADirectory', { path: e.data }),
  invalidName: () => raw('errors.volume.invalidName'),
  deletePending: () => raw('errors.volume.deletePending'),
  staleDestinationHandle: () => raw('errors.volume.staleDestinationHandle'),
  ioError: () => raw('errors.volume.ioError'),
  needsPassword: (e) => raw(e.data.wrongAttempt ? 'errors.volume.passwordRejected' : 'errors.volume.needsPassword'),
  // Git already has its own typed kinds and its own translated factory, so a
  // git-shaped failure keeps speaking git rather than being flattened into a
  // generic "the volume refused".
  friendlyGit: (e) => getGitErrorMessage(e.data.kind).message,
}

/** One renderer per `MutationError` variant. Same shape and reasoning as `VOLUME_MESSAGE`. */
const MUTATION_MESSAGE: {
  [K in MutationError['type']]: (error: Extract<MutationError, { type: K }>, kind: NamedKind) => string
} = {
  // The three the live validation already words. Reused so a name the backend
  // turns down reads the way the red border read a moment earlier.
  nameEmpty: (_e, kind) => tString('fileOperations.validation.empty', { kind }),
  nameHasDisallowedCharacter: (_e, kind) => tString('fileOperations.validation.disallowedChars', { kind }),
  alreadyExists: (e) => tString('fileOperations.validation.conflict', { name: e.name }),

  trashNotSupported: () => raw('errors.mutation.trashNotSupported'),
  trashRefused: () => raw('errors.mutation.trashRefused'),

  notFound: (e) => raw('errors.mutation.notFound', { path: e.path }),
  cantRenameVolumeRoot: () => raw('errors.mutation.cantRenameVolumeRoot'),
  parentNotWritable: (e) => raw('errors.mutation.parentNotWritable', { path: e.path }),
  fileLocked: () => raw('errors.mutation.fileLocked'),
  sipProtected: () => raw('errors.mutation.sipProtected'),
  volumeGone: () => raw('errors.mutation.volumeGone'),
  archiveNotEditable: () => raw('errors.mutation.archiveNotEditable'),
  archiveReadOnly: () => raw('errors.mutation.archiveReadOnly'),
  renameOutOfArchive: () => raw('errors.mutation.renameOutOfArchive'),
  renameAcrossArchives: () => raw('errors.mutation.renameAcrossArchives'),
  archiveEditNotReady: () => raw('errors.mutation.archiveEditNotReady'),
  archiveEditCouldntStart: () => raw('errors.mutation.archiveEditCouldntStart'),
  timedOut: () => raw('errors.mutation.timedOut'),
  volume: (e) => renderVolumeError(e.error),
  // The single honest fallback. ❌ `detail` is never the message; it goes to
  // `technicalDetail()`.
  unexpected: () => raw('errors.mutation.unexpected'),
}

/**
 * The one sentence a `VolumeError` says.
 *
 * Exported because a volume's refusal is not mutation-specific; any surface that
 * gets one can word it from here rather than growing a second vocabulary.
 */
export function renderVolumeError(error: VolumeError): string {
  // TypeScript can't narrow a record lookup keyed by a union's own discriminant,
  // so the correlation is asserted here, once, where the table above proves it.
  const render = VOLUME_MESSAGE[error.type] as (error: VolumeError) => string
  return render(error)
}

/** The one sentence a `MutationError` says. `kind` shapes the reused validation copy. */
export function renderMutationError(error: MutationError, kind: NamedKind = 'file'): string {
  const render = MUTATION_MESSAGE[error.type] as (error: MutationError, kind: NamedKind) => string
  return render(error, kind)
}

/**
 * The technical half of a `VolumeError`, or `null` when it carries none.
 *
 * The variants listed first are the ones whose payload IS the backend's
 * diagnostic string; the path-carrying ones are absent because the message
 * already names their payload, so repeating it adds nothing.
 */
function volumeDetail(volume: VolumeError): string | null {
  switch (volume.type) {
    case 'deviceDisconnected':
    case 'deviceSessionReset':
    case 'readOnly':
    case 'connectionTimeout':
    case 'cancelled':
    case 'invalidName':
      return volume.data
    case 'storageFull':
      return volume.data.message
    case 'friendlyGit':
      return volume.data.raw ?? null
    case 'ioError':
      return volume.data.rawOsError === null
        ? volume.data.message
        : `${volume.data.message} (errno ${String(volume.data.rawOsError)})`
    default:
      return null
  }
}

/** `MutationError` variants that carry their own free-text `detail` field. */
const DETAIL_BEARING_VARIANTS = new Set<MutationError['type']>([
  'unexpected',
  'archiveEditCouldntStart',
  'trashRefused',
])

/**
 * The backend's own words for this refusal, for a technical-details disclosure
 * or a log line. `null` when the variant carries nothing technical.
 *
 * ❌ Never render this as the message: it's untranslated diagnostic text.
 */
export function technicalDetail(error: MutationError): string | null {
  if (DETAIL_BEARING_VARIANTS.has(error.type)) return (error as { detail: string }).detail
  return error.type === 'volume' ? volumeDetail(error.error) : null
}
