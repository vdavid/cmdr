/**
 * The catalog key for each typed value the card words. Lookup tables only: every key is a literal here, so the
 * catalog's dead-key check sees each one, and a new enum variant is a compile error until it has words.
 */

import type {
  AboutUnavailable,
  ChildKind,
  FactKind,
  FolderClass,
  HiddenInName,
  NotApplicableWhy,
  OwnerPlace,
  Section,
  SniffedKind,
  StandardFolder,
  UnknownWhy,
  VerdictContents,
  VerdictProblem,
  VolumeLimit,
} from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { HintWhy } from './about-verdict'

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
  gitSnapshot: 'aboutThis.label.gitSnapshot',
  imageContents: 'aboutThis.label.imageContents',
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
  volumeStoppedAnswering: 'aboutThis.unknown.volumeStoppedAnswering',
  notFound: 'aboutThis.unknown.notFound',
  permissionDenied: 'aboutThis.unknown.permissionDenied',
  protected: 'aboutThis.unknown.protected',
  nodeChangedSince: 'aboutThis.unknown.nodeChangedSince',
  ambiguous: 'aboutThis.unknown.ambiguous',
  unparseable: 'aboutThis.unknown.unparseable',
  refusedInBackground: 'aboutThis.unknown.refusedInBackground',
  indexingOff: 'aboutThis.unknown.indexingOff',
  onlineOnly: 'aboutThis.unknown.onlineOnly',
}

/** Where a row says an absence in its own words; every other reason takes `UNKNOWN_WHY`. */
export const ROW_UNKNOWN: Partial<Record<FactKind, Partial<Record<UnknownWhy, MessageKey>>>> = {
  download: { noRecord: 'aboutThis.download.noRecord' },
  cmdrOperation: { noRecord: 'aboutThis.cmdrOperation.noRecord' },
  lastOpened: { noRecord: 'aboutThis.lastOpened.noRecord' },
  owner: { noRecord: 'aboutThis.owner.noReceipt', ambiguous: 'aboutThis.owner.ambiguous' },
  imageContents: {
    notIndexed: 'aboutThis.imageContents.notIndexed',
    indexingOff: 'aboutThis.imageContents.indexingOff',
  },
  // The importance store scores folders apart from the drive index, so "not indexed" read as a contradiction under a
  // digest the index had just answered.
  importance: { notIndexed: 'aboutThis.importance.notScored' },
}

/** A row's own words for an absence, else the shared ones. */
export function absenceKey(kind: FactKind, why: UnknownWhy): MessageKey {
  return ROW_UNKNOWN[kind]?.[why] ?? UNKNOWN_WHY[why]
}

/** A standard folder by name, as the owner row and its removal line call it ("Your Downloads folder"). */
export const STANDARD_FOLDER_NAME: Record<StandardFolder, MessageKey> = {
  home: 'aboutThis.standardFolder.home',
  desktop: 'aboutThis.standardFolder.desktop',
  documents: 'aboutThis.standardFolder.documents',
  downloads: 'aboutThis.standardFolder.downloads',
  movies: 'aboutThis.standardFolder.movies',
  music: 'aboutThis.standardFolder.music',
  pictures: 'aboutThis.standardFolder.pictures',
  public: 'aboutThis.standardFolder.public',
  homeLibrary: 'aboutThis.standardFolder.homeLibrary',
  homeApplications: 'aboutThis.standardFolder.homeApplications',
  applications: 'aboutThis.standardFolder.applications',
  library: 'aboutThis.standardFolder.library',
  system: 'aboutThis.standardFolder.system',
  users: 'aboutThis.standardFolder.users',
  startupDisk: 'aboutThis.standardFolder.startupDisk',
}

/** What removing a standard folder means, by what the folder holds. */
export const STANDARD_FOLDER_REMOVAL: Record<StandardFolder, MessageKey> = {
  home: 'aboutThis.removal.standardFolder.home',
  desktop: 'aboutThis.removal.standardFolder.personal',
  documents: 'aboutThis.removal.standardFolder.personal',
  downloads: 'aboutThis.removal.standardFolder.personal',
  movies: 'aboutThis.removal.standardFolder.personal',
  music: 'aboutThis.removal.standardFolder.personal',
  pictures: 'aboutThis.removal.standardFolder.personal',
  public: 'aboutThis.removal.standardFolder.personal',
  homeLibrary: 'aboutThis.removal.standardFolder.homeLibrary',
  homeApplications: 'aboutThis.removal.standardFolder.homeApplications',
  applications: 'aboutThis.removal.standardFolder.applications',
  library: 'aboutThis.removal.standardFolder.library',
  system: 'aboutThis.removal.standardFolder.system',
  users: 'aboutThis.removal.standardFolder.users',
  startupDisk: 'aboutThis.removal.standardFolder.startupDisk',
}

/**
 * A commit id as the file list shows it: the first seven characters (`SHORT_ID_LENGTH` in
 * `file-explorer/views/full-list-utils.ts`). The whole id rides behind Copy.
 */
export function shortCommitId(id: string): string {
  return id.slice(0, 7)
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
}

/** The words for one importance reason token. */
export function importanceReasonKey(token: string): MessageKey {
  return IMPORTANCE_REASON[token] ?? 'aboutThis.importance.reason.other'
}

export const CHILD_KIND: Record<ChildKind, MessageKey> = {
  folder: 'aboutThis.childKind.folder',
  image: 'aboutThis.childKind.image',
  video: 'aboutThis.childKind.video',
  audio: 'aboutThis.childKind.audio',
  document: 'aboutThis.childKind.document',
  archive: 'aboutThis.childKind.archive',
  code: 'aboutThis.childKind.code',
  app: 'aboutThis.childKind.app',
  other: 'aboutThis.childKind.other',
}

/** A folder's class, worded; a plain folder says nothing about it. */
export const FOLDER_CLASS: Record<Exclude<FolderClass, 'plain'>, MessageKey> = {
  dependencies: 'aboutThis.folderClass.dependencies',
  buildOutput: 'aboutThis.folderClass.buildOutput',
  cache: 'aboutThis.folderClass.cache',
  versionControl: 'aboutThis.folderClass.versionControl',
}

export const OWNER_PLACE: Record<OwnerPlace, MessageKey> = {
  caches: 'aboutThis.ownerPlace.caches',
  containers: 'aboutThis.ownerPlace.containers',
  groupContainers: 'aboutThis.ownerPlace.groupContainers',
  applicationSupport: 'aboutThis.ownerPlace.applicationSupport',
  applicationScripts: 'aboutThis.ownerPlace.applicationScripts',
  preferences: 'aboutThis.ownerPlace.preferences',
  savedState: 'aboutThis.ownerPlace.savedState',
  logs: 'aboutThis.ownerPlace.logs',
  webData: 'aboutThis.ownerPlace.webData',
  sandboxHome: 'aboutThis.ownerPlace.sandboxHome',
}

/** Why the ranking leaves a folder out, by the scorer's floor token, said as Cmdr's choice. */
const IMPORTANCE_FLOOR: Record<string, MessageKey | undefined> = {
  nameDenylisted: 'aboutThis.importance.floor.nameDenylisted',
  hiddenOrSystem: 'aboutThis.importance.floor.hiddenOrSystem',
  underFlooredAncestor: 'aboutThis.importance.floor.underFlooredAncestor',
}

/** The sentence for one floor token. A token the scorer adds later reads as the general one. */
export function importanceFloorKey(token: string): MessageKey {
  return IMPORTANCE_FLOOR[token] ?? 'aboutThis.importance.floor.other'
}

/** The Name row's one note, by what the item's name hid that the card shows it without. */
export const NAME_HID: Record<HiddenInName, MessageKey> = {
  reordering: 'aboutThis.identity.nameHidReordering',
  invisible: 'aboutThis.identity.nameHidInvisible',
}

/** The AI take's line saying what it was given beside the facts, shown from the moment the call goes out. */
export const VERDICT_GIVEN: Record<VerdictContents, MessageKey> = {
  none: 'aboutThis.verdict.given.none',
  firstLines: 'aboutThis.verdict.given.firstLines',
  firstPage: 'aboutThis.verdict.given.firstPage',
  entryNames: 'aboutThis.verdict.given.entryNames',
  cameraDetails: 'aboutThis.verdict.given.cameraDetails',
  protected: 'aboutThis.verdict.given.protected',
}

/** Why the AI couldn't write its take, by the provider's typed answer. */
export const VERDICT_PROBLEM: Record<VerdictProblem, MessageKey> = {
  authFailed: 'aboutThis.verdict.problem.authFailed',
  rateLimited: 'aboutThis.verdict.problem.rateLimited',
  unreachable: 'aboutThis.verdict.problem.unreachable',
  overloaded: 'aboutThis.verdict.problem.overloaded',
  managed: 'aboutThis.verdict.problem.managed',
  noAnswer: 'aboutThis.verdict.problem.noAnswer',
  other: 'aboutThis.verdict.problem.other',
}

/** The hint line under the footer, by the setting that stands between the card and an AI take. */
export const HINT_LINE: Record<HintWhy['kind'], MessageKey> = {
  aiOff: 'aboutThis.verdict.aiOffHint',
  noCloudConsent: 'aboutThis.verdict.unavailable.noCloudConsent',
  notConfigured: 'aboutThis.verdict.unavailable.notConfigured',
}
