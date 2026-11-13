/**
 * The catalog key for each typed value the card words. Lookup tables only: every key is a literal here, so the
 * catalog's dead-key check sees each one, and a new enum variant is a compile error until it has words.
 */

import type {
  AboutUnavailable,
  FactKind,
  NotApplicableWhy,
  Section,
  SniffedKind,
  UnknownWhy,
  VolumeLimit,
} from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

export const SECTION_TITLE: Record<Section, MessageKey> = {
  origin: 'aboutThis.section.origin',
  nature: 'aboutThis.section.nature',
  use: 'aboutThis.section.use',
  context: 'aboutThis.section.context',
  contents: 'aboutThis.section.contents',
  removal: 'aboutThis.section.removal',
}

/** Each row's label. Identity draws several labeled lines of its own, so it has none here. */
export const ROW_LABEL: Record<Exclude<FactKind, 'identity'>, MessageKey> = {
  realType: 'aboutThis.label.realType',
  bundle: 'aboutThis.label.bundle',
  signature: 'aboutThis.label.signature',
  download: 'aboutThis.label.download',
  cmdrOperation: 'aboutThis.label.cmdrOperation',
  lastOpened: 'aboutThis.label.lastOpened',
  defaultApp: 'aboutThis.label.defaultApp',
  importance: 'aboutThis.label.importance',
  owner: 'aboutThis.label.owner',
  git: 'aboutThis.label.git',
  recursiveSize: 'aboutThis.label.size',
  folderDigest: 'aboutThis.label.contents',
  appData: 'aboutThis.label.appData',
}

export const UNKNOWN_WHY: Record<UnknownWhy, MessageKey> = {
  noRecord: 'aboutThis.unknown.noRecord',
  notInStatus: 'aboutThis.unknown.notInStatus',
  noInstalledAppMatches: 'aboutThis.unknown.noInstalledAppMatches',
  volumeCantTell: 'aboutThis.unknown.volumeCantTell',
  libraryNameMatchesNoApp: 'aboutThis.unknown.libraryNameMatchesNoApp',
  notIndexed: 'aboutThis.unknown.notIndexed',
  timedOut: 'aboutThis.unknown.timedOut',
  unreachable: 'aboutThis.unknown.unreachable',
  permissionDenied: 'aboutThis.unknown.permissionDenied',
  protected: 'aboutThis.unknown.protected',
  nodeChangedSince: 'aboutThis.unknown.nodeChangedSince',
  ambiguous: 'aboutThis.unknown.ambiguous',
  unparseable: 'aboutThis.unknown.unparseable',
  refusedInBackground: 'aboutThis.unknown.refusedInBackground',
}

/** Where a row says an absence in its own words; every other reason takes `UNKNOWN_WHY`. */
export const ROW_UNKNOWN: Partial<Record<FactKind, Partial<Record<UnknownWhy, MessageKey>>>> = {
  download: { noRecord: 'aboutThis.download.noRecord' },
  cmdrOperation: { noRecord: 'aboutThis.cmdrOperation.noRecord' },
  lastOpened: { noRecord: 'aboutThis.lastOpened.noRecord' },
  owner: { noRecord: 'aboutThis.owner.noReceipt', ambiguous: 'aboutThis.owner.ambiguous' },
}

export const NOT_APPLICABLE_WHY: Record<NotApplicableWhy, MessageKey> = {
  noCodeSigningOnThisOs: 'aboutThis.notApplicable.noCodeSigningOnThisOs',
  notInARepo: 'aboutThis.notApplicable.notInARepo',
  notABundle: 'aboutThis.notApplicable.notABundle',
  volumeHasNone: 'aboutThis.notApplicable.volumeHasNone',
}

export const KIND_LABEL: Record<SniffedKind, MessageKey> = {
  diskImage: 'aboutThis.kind.diskImage',
  installerPackage: 'aboutThis.kind.installerPackage',
  machO: 'aboutThis.kind.machO',
  elf: 'aboutThis.kind.elf',
  windowsProgram: 'aboutThis.kind.windowsProgram',
  otherCode: 'aboutThis.kind.otherCode',
  script: 'aboutThis.kind.script',
  archive: 'aboutThis.kind.archive',
  pdf: 'aboutThis.kind.pdf',
  image: 'aboutThis.kind.image',
  video: 'aboutThis.kind.video',
  audio: 'aboutThis.kind.audio',
  font: 'aboutThis.kind.font',
  document: 'aboutThis.kind.document',
  database: 'aboutThis.kind.database',
  text: 'aboutThis.kind.text',
  other: 'aboutThis.kind.other',
  unrecognized: 'aboutThis.kind.unrecognized',
}

export const VOLUME_NOTE: Record<VolumeLimit, MessageKey> = {
  identityOnly: 'aboutThis.volumeNote.identityOnly',
  someFacts: 'aboutThis.volumeNote.someFacts',
  unreachable: 'aboutThis.volumeNote.unreachable',
}

export const UNAVAILABLE: Record<AboutUnavailable, MessageKey> = {
  unknownVolume: 'aboutThis.unavailable.unknownVolume',
  pathOutsideVolume: 'aboutThis.unavailable.pathOutsideVolume',
  noVolumeAtPath: 'aboutThis.unavailable.noVolumeAtPath',
  notFound: 'aboutThis.unavailable.notFound',
  permissionDenied: 'aboutThis.unavailable.permissionDenied',
  unreachable: 'aboutThis.unavailable.unreachable',
  unreadable: 'aboutThis.unavailable.unreadable',
}

/** The importance scorer's reason tokens. A token the scorer adds later reads as "other signals" until it has words. */
const IMPORTANCE_REASON: Record<string, MessageKey | undefined> = {
  projectMarker: 'aboutThis.importance.reason.projectMarker',
  mtimeRecency: 'aboutThis.importance.reason.mtimeRecency',
  visitActivity: 'aboutThis.importance.reason.visitActivity',
  lastUsed: 'aboutThis.importance.reason.lastUsed',
  extensionDiversity: 'aboutThis.importance.reason.extensionDiversity',
  projectRoot: 'aboutThis.importance.reason.projectRoot',
  userContent: 'aboutThis.importance.reason.userContent',
  nameDenylisted: 'aboutThis.importance.reason.nameDenylisted',
  hiddenOrSystem: 'aboutThis.importance.reason.hiddenOrSystem',
  underFlooredAncestor: 'aboutThis.importance.reason.underFlooredAncestor',
}

/** The words for one importance reason token. */
export function importanceReasonKey(token: string): MessageKey {
  return IMPORTANCE_REASON[token] ?? 'aboutThis.importance.reason.other'
}
