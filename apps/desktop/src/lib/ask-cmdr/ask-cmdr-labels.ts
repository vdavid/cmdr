/**
 * Typed maps from wire enum values to localized strings for the Ask Cmdr rail. Kept as
 * literal records so every catalog key has a static call site (`desktop-message-keys-unused`
 * would flag a key only reached through a computed prefix).
 */

import { t, tString, type TranslationParams } from '#lib/intl/messages.svelte.js'
import { formatConjunctionList } from '#lib/intl/list-format.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { formatMilliseconds } from '#lib/units/index.js'
import type { AskCmdrErrorKind, RenameEvidenceSource, SkipBreakdown, SkipReason } from '#lib/tauri-commands/index.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type {
  ConnectOutcome,
  IndexUnavailableReason,
  LinePath,
  ManagedAiRefusal,
  MemoryRefusalKind,
  MissToken,
  RefusalReason,
  ToolOutcome,
  ToolSubject,
  TurnLimit,
  TurnStats,
} from '#lib/ipc/bindings.js'
import { managedAiRefusalMessage } from '#lib/managed-policy/ai-refusal.js'
import { formatTokenCount, totalTokens } from './ask-cmdr-cost'
import type { RailToolCall } from './ask-cmdr-messages'
import { outcomeOk, placeName, type PathTarget } from './ask-cmdr-tool-line'

/**
 * A tool line's label keys: present tense while it runs, past tense once it's done, and, where
 * the past tense would claim a success a refusal didn't have ("Saved a note"), the words for
 * having tried.
 */
interface PhaseKeys {
  doing: MessageKey
  done: MessageKey
  tried?: MessageKey
}

/** Param-less label keys per read-only tool: what a line says when its subject names nothing. */
const TOOL_LABEL_KEYS: Partial<Record<string, PhaseKeys>> = {
  app_state: { doing: 'askCmdr.tool.appState.doing', done: 'askCmdr.tool.appState.done' },
  list_dir: { doing: 'askCmdr.tool.listDir.doing', done: 'askCmdr.tool.listDir.done' },
  list_pane_files: { doing: 'askCmdr.tool.listDir.doing', done: 'askCmdr.tool.listDir.done' },
  important_folders: { doing: 'askCmdr.tool.importantFolders.doing', done: 'askCmdr.tool.importantFolders.done' },
  folder_importance: { doing: 'askCmdr.tool.folderImportance.doing', done: 'askCmdr.tool.folderImportance.done' },
  list_volumes: { doing: 'askCmdr.tool.listVolumes.doing', done: 'askCmdr.tool.listVolumes.done' },
  inspect_file: { doing: 'askCmdr.tool.inspectFile.doing', done: 'askCmdr.tool.inspectFile.done' },
  operations_list: { doing: 'askCmdr.tool.operationsList.doing', done: 'askCmdr.tool.operationsList.done' },
  operations_get: { doing: 'askCmdr.tool.operationsGet.doing', done: 'askCmdr.tool.operationsGet.done' },
  search: { doing: 'askCmdr.tool.search.doing', done: 'askCmdr.tool.search.done' },
  search_photos: { doing: 'askCmdr.tool.searchPhotos.doing', done: 'askCmdr.tool.searchPhotos.done' },
  image_facts: { doing: 'askCmdr.tool.imageFacts.doing', done: 'askCmdr.tool.imageFacts.done' },
  propose_rename_plan: { doing: 'askCmdr.tool.proposeRenamePlan.doing', done: 'askCmdr.tool.proposeRenamePlan.done' },
  list_suggestions: { doing: 'askCmdr.tool.listSuggestions.doing', done: 'askCmdr.tool.listSuggestions.done' },
  get_suggestion_group: {
    doing: 'askCmdr.tool.getSuggestionGroup.doing',
    done: 'askCmdr.tool.getSuggestionGroup.done',
  },
  propose_suggestions: { doing: 'askCmdr.tool.proposeSuggestions.doing', done: 'askCmdr.tool.proposeSuggestions.done' },
  propose_server: { doing: 'askCmdr.tool.proposeServer.doing', done: 'askCmdr.tool.proposeServer.done' },
  nothing_to_suggest: {
    doing: 'askCmdr.tool.nothingToSuggest.doing',
    done: 'askCmdr.tool.nothingToSuggest.done',
  },
  memory_write: {
    doing: 'askCmdr.tool.memoryWrite.doing',
    done: 'askCmdr.tool.memoryWrite.done',
    tried: 'askCmdr.tool.memoryWrite.tried',
  },
  memory_edit: {
    doing: 'askCmdr.tool.memoryEdit.doing',
    done: 'askCmdr.tool.memoryEdit.done',
    tried: 'askCmdr.tool.memoryEdit.tried',
  },
  connect_volume: {
    doing: 'askCmdr.tool.connectVolume.doing',
    done: 'askCmdr.tool.connectVolume.done',
    tried: 'askCmdr.tool.connectVolume.tried',
  },
}

const UNKNOWN_TOOL_KEYS: PhaseKeys = { doing: 'askCmdr.tool.unknown.doing', done: 'askCmdr.tool.unknown.done' }

const LIST_FOLDER_KEYS: PhaseKeys = {
  doing: 'askCmdr.tool.listDir.folder.doing',
  done: 'askCmdr.tool.listDir.folder.done',
}

/** Label keys that name one folder, per tool whose subject is a `folder`. */
const FOLDER_KEYS: Partial<Record<string, PhaseKeys>> = {
  list_dir: LIST_FOLDER_KEYS,
  folder_importance: {
    doing: 'askCmdr.tool.folderImportance.folder.doing',
    done: 'askCmdr.tool.folderImportance.folder.done',
  },
}

/** Label keys that name the first of a call's files, per tool whose subject is `files`. */
const FILES_KEYS: Partial<Record<string, PhaseKeys>> = {
  inspect_file: { doing: 'askCmdr.tool.inspectFile.files.doing', done: 'askCmdr.tool.inspectFile.files.done' },
  image_facts: { doing: 'askCmdr.tool.imageFacts.files.doing', done: 'askCmdr.tool.imageFacts.files.done' },
}

/** Label keys that name a note file, per tool whose subject is a `memoryFile`. */
const MEMORY_FILE_KEYS: Partial<Record<string, PhaseKeys>> = {
  memory_write: {
    doing: 'askCmdr.tool.memoryWrite.file.doing',
    done: 'askCmdr.tool.memoryWrite.file.done',
    tried: 'askCmdr.tool.memoryWrite.file.tried',
  },
  memory_edit: {
    doing: 'askCmdr.tool.memoryEdit.file.doing',
    done: 'askCmdr.tool.memoryEdit.file.done',
    tried: 'askCmdr.tool.memoryEdit.file.tried',
  },
}

/** A path a tool line names, and how the line shows it. */
export interface LinePlace extends PathTarget {
  /** What the line shows: the path's last segment, or a note's own path inside its folder. */
  shown: string
  /** Whether it's a place to go: the backend says so (`LinePath.linkable`). A relative or cut
   * path isn't, nor a note path that leads outside the notes folder. */
  linked: boolean
}

/**
 * A tool line's label as a catalog message: the key, its params, and the paths its tags stand
 * for (`<place>` one path, `<places>` a list of them), so the line can render each as a link.
 */
export interface LineLabel {
  key: MessageKey
  params: TranslationParams
  place: LinePlace | null
  places: LinePlace[]
}

/** What a subject adds to its tool's label. */
interface SubjectWords {
  keys: PhaseKeys
  params: Record<string, string | number>
  place?: LinePlace
  places?: LinePlace[]
}

function linePlace({ text, linkable }: LinePath, kind: LinePlace['kind']): LinePlace {
  return { path: text, kind, shown: placeName(text), linked: linkable }
}

function folderPlace(path: LinePath): LinePlace {
  return linePlace(path, 'folder')
}

function counts(count: number): { countText: string; count: number } {
  return { countText: formatInteger(count), count }
}

type SubjectOf<K extends ToolSubject['kind']> = Extract<ToolSubject, { kind: K }>

/** Words for a subject that names nothing beyond what the tool's own label already says. */
const noWords = (): null => null

function placeWords(keys: PhaseKeys | undefined, place: LinePlace): SubjectWords | null {
  return keys === undefined ? null : { keys, params: { name: place.shown }, place }
}

/**
 * A search's words follow what it has: a pattern, folders to look in, both, or neither. `folders`
 * rides beside `placesText` so a case language can tell "in the folder X" from "in the folders X and Y".
 */
function searchWords(subject: SubjectOf<'search'>): SubjectWords | null {
  const { query } = subject
  const places = subject.scopePaths.map(folderPlace)
  const placesText = formatConjunctionList(places.map((place) => place.shown))
  const folders = places.length
  if (query !== null && folders > 0) {
    const keys: PhaseKeys = {
      doing: 'askCmdr.tool.search.queryInFolders.doing',
      done: 'askCmdr.tool.search.queryInFolders.done',
    }
    return { keys, params: { query, placesText, folders }, places }
  }
  if (query !== null) {
    const keys: PhaseKeys = { doing: 'askCmdr.tool.search.query.doing', done: 'askCmdr.tool.search.query.done' }
    return { keys, params: { query } }
  }
  if (folders === 0) return null
  const keys: PhaseKeys = { doing: 'askCmdr.tool.search.inFolders.doing', done: 'askCmdr.tool.search.inFolders.done' }
  return { keys, params: { placesText, folders }, places }
}

/**
 * The words each subject gives its line, or `null` to keep the tool's param-less label (a subject
 * that names nothing worth saying, or one this tool doesn't word). A `Record` over every kind, so
 * a new subject kind is a compile error until it's decided.
 */
const SUBJECT_WORDS: { [K in ToolSubject['kind']]: (subject: SubjectOf<K>, tool: string) => SubjectWords | null } = {
  focusedPane: (subject) => {
    if (subject.folder === null) return null
    const keys: PhaseKeys =
      subject.selectionOnly === true
        ? { doing: 'askCmdr.tool.listPaneFiles.selection.doing', done: 'askCmdr.tool.listPaneFiles.selection.done' }
        : LIST_FOLDER_KEYS
    return placeWords(keys, folderPlace(subject.folder))
  },
  folder: (subject, tool) => placeWords(FOLDER_KEYS[tool], folderPlace(subject.path)),
  files: (subject, tool) => {
    const words = placeWords(FILES_KEYS[tool], linePlace(subject.firstPath, 'file'))
    const others = Math.max(subject.count - 1, 0)
    return words && { ...words, params: { ...words.params, others, othersText: formatInteger(others) } }
  },
  search: searchWords,
  photoSearch: (subject) => {
    if (subject.query.length === 0) return null
    const keys: PhaseKeys = {
      doing: 'askCmdr.tool.searchPhotos.query.doing',
      done: 'askCmdr.tool.searchPhotos.query.done',
    }
    return { keys, params: { query: subject.query } }
  },
  renamePlan: (subject) => ({
    keys: { doing: 'askCmdr.tool.proposeRenamePlan.count.doing', done: 'askCmdr.tool.proposeRenamePlan.count.done' },
    params: counts(subject.count),
  }),
  suggestionSweep: (subject) => ({
    keys: { doing: 'askCmdr.tool.proposeSuggestions.count.doing', done: 'askCmdr.tool.proposeSuggestions.count.done' },
    params: counts(subject.count),
  }),
  memoryFile: (subject, tool) =>
    placeWords(MEMORY_FILE_KEYS[tool], {
      path: subject.absolute ?? subject.path,
      kind: 'file',
      shown: subject.path,
      linked: subject.absolute !== null,
    }),
  unknownTool: (subject) => {
    if (subject.name.length === 0) return null
    const keys: PhaseKeys = { doing: 'askCmdr.tool.unknown.named.doing', done: 'askCmdr.tool.unknown.named.done' }
    return { keys, params: { name: subject.name } }
  },
  panes: noWords,
  importantFolders: noWords,
  drives: noWords,
  // A volume id is a token (`smb-192-168-1-5-…`), not a name a person knows the server by.
  volume: noWords,
  operationLog: noWords,
  operation: noWords,
  suggestions: noWords,
  suggestionGroup: noWords,
  // The server's name is the model's text; the card shows it, as a fact Cmdr checked.
  serverSuggestion: noWords,
  nothingToSuggest: noWords,
  unparsed: noWords,
}

function subjectWords(subject: ToolSubject, tool: string): SubjectWords | null {
  // TypeScript can't correlate the kind with its handler's parameter, so widen the handler once here.
  const words = SUBJECT_WORDS[subject.kind] as (subject: ToolSubject, tool: string) => SubjectWords | null
  return words(subject, tool)
}

/**
 * A tool line's label: what the call looks at, in its phase. A call with no subject yet (its
 * arguments still streaming) or an unreadable one keeps the tool's param-less words, and an
 * unrecognized tool name falls to the generic ones.
 */
export function toolLineLabel(call: RailToolCall): LineLabel {
  const words = call.subject === null ? null : subjectWords(call.subject, call.tool)
  const keys = words?.keys ?? TOOL_LABEL_KEYS[call.tool] ?? UNKNOWN_TOOL_KEYS
  return {
    key: phaseKey(keys, call),
    params: words?.params ?? {},
    place: words?.place ?? null,
    places: words?.places ?? [],
  }
}

function phaseKey(keys: PhaseKeys, call: RailToolCall): MessageKey {
  if (call.running) return keys.doing
  if (keys.tried !== undefined && call.outcome !== null && !outcomeOk(call.outcome)) return keys.tried
  return keys.done
}

/** A label as plain text, each tag's content kept as is: what the line's overflow tooltip shows. */
export function lineLabelText(label: LineLabel): string {
  const asText = (chunks: unknown[]): string => chunks.join('')
  const result = t(label.key, { ...label.params, place: asText, places: asText })
  return Array.isArray(result) ? result.join('') : String(result)
}

const REFUSAL_KEYS: Record<Exclude<RefusalReason['kind'], 'memory'>, MessageKey> = {
  toolUnavailable: 'askCmdr.tool.outcome.toolUnavailable',
  repeatedCall: 'askCmdr.tool.outcome.repeatedCall',
  problem: 'askCmdr.tool.outcome.problem',
}

const MEMORY_REFUSAL_KEYS: Record<MemoryRefusalKind, MessageKey> = {
  outsideMemory: 'askCmdr.tool.outcome.memory.outsideMemory',
  notMarkdown: 'askCmdr.tool.outcome.memory.notMarkdown',
  noPath: 'askCmdr.tool.outcome.memory.noPath',
  directoryFull: 'askCmdr.tool.outcome.memory.directoryFull',
  noSuchFile: 'askCmdr.tool.outcome.memory.noSuchFile',
  noMatch: 'askCmdr.tool.outcome.memory.noMatch',
  notUnique: 'askCmdr.tool.outcome.memory.notUnique',
  unwritable: 'askCmdr.tool.outcome.memory.unwritable',
  unavailable: 'askCmdr.tool.outcome.memory.unavailable',
  other: 'askCmdr.tool.outcome.memory.other',
}

const INDEX_UNAVAILABLE_KEYS: Record<IndexUnavailableReason, MessageKey> = {
  imageIndexingOff: 'askCmdr.tool.outcome.imageIndexingOff',
  semanticModelNotInstalled: 'askCmdr.tool.outcome.semanticModelNotInstalled',
}

const MISS_KEYS: Record<MissToken, MessageKey> = {
  unknownVolume: 'askCmdr.tool.outcome.miss.unknownVolume',
  pathOutsideVolume: 'askCmdr.tool.outcome.miss.pathOutsideVolume',
  noVolumeAtPath: 'askCmdr.tool.outcome.miss.noVolumeAtPath',
  volumeNotConnected: 'askCmdr.tool.outcome.miss.volumeNotConnected',
  needsDiskAccess: 'askCmdr.tool.outcome.miss.needsDiskAccess',
  unreachable: 'askCmdr.tool.outcome.miss.unreachable',
  notFound: 'askCmdr.tool.outcome.miss.notFound',
  excluded: 'askCmdr.tool.outcome.miss.excluded',
  permissionDenied: 'askCmdr.tool.outcome.miss.permissionDenied',
  cancelled: 'askCmdr.tool.outcome.miss.cancelled',
}

/** `null` for a fresh connection: the label ("Connected to your server") already says it. */
const CONNECTION_KEYS: Record<ConnectOutcome, MessageKey | null> = {
  connected: null,
  alreadyConnected: 'askCmdr.tool.outcome.connection.alreadyConnected',
  stillReconnecting: 'askCmdr.tool.outcome.connection.stillReconnecting',
  needsCredentials: 'askCmdr.tool.outcome.connection.needsCredentials',
  needsApproval: 'askCmdr.tool.outcome.connection.needsApproval',
  rejected: 'askCmdr.tool.outcome.connection.rejected',
  unreachable: 'askCmdr.tool.outcome.connection.unreachable',
  notSaved: 'askCmdr.tool.outcome.connection.notSaved',
  cancelled: 'askCmdr.tool.outcome.connection.cancelled',
}

/**
 * What came back, as the short note after a line's label. `null` while the call runs, and for a
 * plain answer or a saved note, where the label and the glyph already say it all, and for a count
 * the label already names (`restatesSubject`). A settled call with no outcome (a stopped or crashed
 * turn) says no answer was recorded.
 */
export function toolOutcomeText(call: RailToolCall): string | null {
  if (call.running) return null
  if (call.outcome === null) return tString('askCmdr.tool.outcome.noAnswer')
  if (restatesSubject(call.subject, call.outcome)) return null
  return outcomeText(call.outcome)
}

/**
 * Whether the outcome only repeats how many paths the label already names: "Looked inside X and 1
 * more" answered for both paths would add "2 items". A partial answer (some paths got no row) still
 * says so.
 */
function restatesSubject(subject: ToolSubject | null, outcome: ToolOutcome): boolean {
  if (subject?.kind !== 'files' || outcome.kind !== 'items' || outcome.lowerBound) return false
  return outcome.returned === subject.count && outcome.total === subject.count
}

type OutcomeOf<K extends ToolOutcome['kind']> = Extract<ToolOutcome, { kind: K }>

/** A list's count, or how many of how many the model saw when it got part of it. */
function itemsText({ returned, total, lowerBound }: OutcomeOf<'items'>): string {
  if (total === null || (total <= returned && !lowerBound)) {
    return tString('askCmdr.tool.outcome.items', counts(total ?? returned))
  }
  const key = lowerBound ? 'askCmdr.tool.outcome.itemsShownAtLeast' : 'askCmdr.tool.outcome.itemsShown'
  return tString(key, { shownText: formatInteger(returned), totalText: formatInteger(total) })
}

/**
 * The note each outcome puts after its label. `null` for a plain answer or a saved note, where
 * the label and the glyph already say it all. A `Record` over every kind, so a new outcome is a
 * compile error until it has words.
 */
const OUTCOME_TEXT: { [K in ToolOutcome['kind']]: (outcome: OutcomeOf<K>) => string | null } = {
  items: itemsText,
  matches: (outcome) =>
    tString(
      outcome.lowerBound ? 'askCmdr.tool.outcome.matchesSoFar' : 'askCmdr.tool.outcome.matches',
      counts(outcome.matchCount),
    ),
  nothingFound: (outcome) =>
    tString(outcome.lowerBound ? 'askCmdr.tool.outcome.nothingFoundYet' : 'askCmdr.tool.outcome.nothingFound'),
  indexUnavailable: (outcome) => tString(INDEX_UNAVAILABLE_KEYS[outcome.reason]),
  staged: (outcome) => tString('askCmdr.tool.outcome.staged', counts(outcome.count)),
  notStaged: () => tString('askCmdr.tool.outcome.notStaged'),
  refused: ({ reason }) =>
    tString(reason.kind === 'memory' ? MEMORY_REFUSAL_KEYS[reason.refusal] : REFUSAL_KEYS[reason.kind]),
  locationMiss: ({ miss }) => tString(MISS_KEYS[miss]),
  connection: ({ outcome }) => {
    const key = CONNECTION_KEYS[outcome]
    return key === null ? null : tString(key)
  },
  answered: () => null,
  saved: () => null,
}

function outcomeText(outcome: ToolOutcome): string | null {
  // TypeScript can't correlate the kind with its handler's parameter, so widen the handler once here.
  const text = OUTCOME_TEXT[outcome.kind] as (outcome: ToolOutcome) => string | null
  return text(outcome)
}

/**
 * What a turn spent, as the quiet line under its bubble: tool calls, wall time, and tokens. The
 * tokens are the cost meter's (prompt plus completion per row), so a thread's turns add up to its
 * cost footer. `null` for a turn with no stats or no tool call; a turn recorded before wall time
 * was measured leaves the time out, and a stopped turn leads with saying so. The token count is
 * compact from 2,000 up ("194.8k"); {@link turnStatsTokensTooltip} carries the exact one.
 */
export function turnStatsText(stats: TurnStats | null): string | null {
  if (stats === null || stats.toolCalls === 0) return null
  const tokens = totalTokens(stats)
  const params = {
    calls: stats.toolCalls,
    callsText: formatInteger(stats.toolCalls),
    tokens,
    tokensText: formatTokenCount(tokens),
  }
  const summary =
    stats.wallMs === null
      ? tString('askCmdr.turnStats.summaryNoDuration', params)
      : tString('askCmdr.turnStats.summary', { ...params, duration: formatMilliseconds(stats.wallMs) })
  return stats.stopped === true ? tString('askCmdr.turnStats.stopped', { summary }) : summary
}

/** The exact token count behind a stats line's compact one ("194,776 tokens"), or `undefined`
 * when the line already shows it exactly. */
export function turnStatsTokensTooltip(stats: TurnStats | null): string | undefined {
  if (stats === null || stats.toolCalls === 0) return undefined
  const tokens = totalTokens(stats)
  if (formatTokenCount(tokens) === formatInteger(tokens)) return undefined
  return tString('askCmdr.cost.tokens', { count: tokens, countText: formatInteger(tokens) })
}

/**
 * Which per-answer ceiling a turn reached, and its value, as the first line of its limit notice.
 * The value is the one stored with the turn, so a reloaded thread names the limit that applied
 * then. Wall time is stored in seconds and said in whole minutes (the ceiling is minutes long).
 */
export function limitNoticeText(limit: TurnLimit): string {
  if (limit.kind === 'wallTime') {
    const minutes = Math.max(1, Math.round(limit.max / 60))
    return tString('askCmdr.limit.wallTime', { minutes, minutesText: formatInteger(minutes) })
  }
  const params = { max: limit.max, maxText: formatInteger(limit.max) }
  return limit.answered
    ? tString('askCmdr.limit.toolRounds.answered', params)
    : tString('askCmdr.limit.toolRounds.unfinished', params)
}

const ERROR_KEYS: Record<AskCmdrErrorKind, MessageKey> = {
  noKey: 'askCmdr.error.noKey',
  notConfigured: 'askCmdr.error.notConfigured',
  // Both are rare races (the switch moved between opening the rail and sending): the send
  // refusal also flips the rail back to its gate, which carries the way out.
  askCmdrOff: 'askCmdr.error.askCmdrOff',
  noCloudConsent: 'askCmdr.error.noCloudConsent',
  managedByOrganization: 'askCmdr.error.managedByOrganization',
  localWindowTooSmall: 'askCmdr.error.localWindowTooSmall',
  unavailable: 'askCmdr.error.unavailable',
  timeout: 'askCmdr.error.timeout',
  authFailed: 'askCmdr.error.authFailed',
  rateLimited: 'askCmdr.error.rateLimited',
  budgetExhausted: 'askCmdr.error.budgetExhausted',
  repeatedToolCall: 'askCmdr.error.repeatedToolCall',
  unfinishedReply: 'askCmdr.error.unfinishedReply',
  provider: 'askCmdr.error.provider',
}

/**
 * The friendly, honest message for a typed turn failure (never the words error/failed). The
 * organization's refusal names the rule that refused (`managed`, from the backend) through the
 * shared copy map, so "only on-device AI" never reads as "doesn't allow this AI service".
 */
export function errorMessage(kind: AskCmdrErrorKind, managed?: ManagedAiRefusal | null): string {
  if (kind === 'managedByOrganization' && managed) return managedAiRefusalMessage(managed)
  return tString(ERROR_KEYS[kind])
}

const EVIDENCE_KEYS: Record<RenameEvidenceSource, MessageKey> = {
  imageText: 'askCmdr.renameReview.evidence.imageText',
  imageTags: 'askCmdr.renameReview.evidence.imageTags',
  filename: 'askCmdr.renameReview.evidence.filename',
  metadata: 'askCmdr.renameReview.evidence.metadata',
  userInstruction: 'askCmdr.renameReview.evidence.userInstruction',
  userEdited: 'askCmdr.renameReview.evidence.userEdited',
}

/**
 * What a proposed rename name is based on, named honestly. The two image sources say the
 * contents were read; the other three say plainly that they weren't, so a name with no
 * content behind it can't look content-derived in the review dialog. `userEdited` is the user's
 * own name: it claims nothing, and the row carries no evidence at all.
 */
export function evidenceSourceLabel(source: RenameEvidenceSource): string {
  return tString(EVIDENCE_KEYS[source])
}

/**
 * Per reason an undo can leave a file alone: the line that NAMES the one file it applies
 * to, and the line that COUNTS them when it applies to several. Two keys rather than one
 * ICU plural, because "name it" vs "count them" is a display decision, not a plural
 * category — a locale with only `other` (Chinese, Vietnamese) could not express both.
 *
 * `null` means the reason has no line of its own, so the caller falls back to the
 * reason-class line; the count is reported either way, so a reason with no copy can never
 * hide that files stayed behind. `alreadyGone` is deliberately `null`: an item already
 * back where it belongs counts as restored, not skipped, so it never reaches here.
 *
 * Exhaustive over the wire enum, so a new reason is a compile error until it's decided.
 */
const SKIP_REASON_KEYS: Record<SkipReason, { named: MessageKey; counted: MessageKey } | null> = {
  drift: {
    named: 'askCmdr.renameUndo.skipReason.drift.named',
    counted: 'askCmdr.renameUndo.skipReason.drift.counted',
  },
  restoreTargetOccupied: {
    named: 'askCmdr.renameUndo.skipReason.nameTaken.named',
    counted: 'askCmdr.renameUndo.skipReason.nameTaken.counted',
  },
  unverifiablePrecondition: {
    named: 'askCmdr.renameUndo.skipReason.unverifiable.named',
    counted: 'askCmdr.renameUndo.skipReason.unverifiable.counted',
  },
  dirNotEmpty: {
    named: 'askCmdr.renameUndo.skipReason.folderNotEmpty.named',
    counted: 'askCmdr.renameUndo.skipReason.folderNotEmpty.counted',
  },
  failed: {
    named: 'askCmdr.renameUndo.skipReason.failed.named',
    counted: 'askCmdr.renameUndo.skipReason.failed.counted',
  },
  vanished: {
    named: 'askCmdr.renameUndo.skipReason.vanished.named',
    counted: 'askCmdr.renameUndo.skipReason.vanished.counted',
  },
  // Only a REMOVAL meets a lock it can't clear, and a rename's undo renames back.
  locked: null,
  // Only undoing a COPY checks an original, and a rename's undo renames back.
  originalGone: null,
  alreadyGone: null,
}

/**
 * What happened to the files one reason applies to: the file NAMED when it's the only one,
 * COUNTED when there are several. `null` when the reason has no line of its own.
 */
export function undoSkipMessage(group: SkipBreakdown): string | null {
  const keys = SKIP_REASON_KEYS[group.reason]
  if (!keys) return null
  if (group.count === 1) return tString(keys.named, { name: group.exampleName })
  return tString(keys.counted, { countText: formatInteger(group.count), count: group.count })
}
