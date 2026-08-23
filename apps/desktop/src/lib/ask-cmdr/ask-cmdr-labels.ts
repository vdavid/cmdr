/**
 * Typed maps from wire enum values to localized strings for the Ask Cmdr rail. Kept as
 * literal records so every catalog key has a static call site (`desktop-message-keys-unused`
 * would flag a key only reached through a computed prefix).
 */

import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import type { AskCmdrErrorKind, RenameEvidenceSource, SkipBreakdown, SkipReason } from '$lib/tauri-commands'
import type { MessageKey } from '$lib/intl/keys.gen'

/** Present-tense (running) and past-tense (done) label keys per read-only tool. */
const TOOL_LABEL_KEYS: Record<string, { doing: MessageKey; done: MessageKey }> = {
  app_state: { doing: 'askCmdr.tool.appState.doing', done: 'askCmdr.tool.appState.done' },
  list_dir: { doing: 'askCmdr.tool.listDir.doing', done: 'askCmdr.tool.listDir.done' },
  list_pane_files: { doing: 'askCmdr.tool.listDir.doing', done: 'askCmdr.tool.listDir.done' },
  important_folders: { doing: 'askCmdr.tool.importantFolders.doing', done: 'askCmdr.tool.importantFolders.done' },
  folder_importance: { doing: 'askCmdr.tool.folderImportance.doing', done: 'askCmdr.tool.folderImportance.done' },
  list_volumes: { doing: 'askCmdr.tool.listVolumes.doing', done: 'askCmdr.tool.listVolumes.done' },
  operations_list: { doing: 'askCmdr.tool.operationsList.doing', done: 'askCmdr.tool.operationsList.done' },
  operations_get: { doing: 'askCmdr.tool.operationsGet.doing', done: 'askCmdr.tool.operationsGet.done' },
  search_photos: { doing: 'askCmdr.tool.searchPhotos.doing', done: 'askCmdr.tool.searchPhotos.done' },
  image_facts: { doing: 'askCmdr.tool.imageFacts.doing', done: 'askCmdr.tool.imageFacts.done' },
  propose_rename_plan: { doing: 'askCmdr.tool.proposeRenamePlan.doing', done: 'askCmdr.tool.proposeRenamePlan.done' },
  list_suggestions: { doing: 'askCmdr.tool.listSuggestions.doing', done: 'askCmdr.tool.listSuggestions.done' },
  get_suggestion_group: {
    doing: 'askCmdr.tool.getSuggestionGroup.doing',
    done: 'askCmdr.tool.getSuggestionGroup.done',
  },
  propose_suggestions: { doing: 'askCmdr.tool.proposeSuggestions.doing', done: 'askCmdr.tool.proposeSuggestions.done' },
  nothing_to_suggest: {
    doing: 'askCmdr.tool.nothingToSuggest.doing',
    done: 'askCmdr.tool.nothingToSuggest.done',
  },
  memory_write: { doing: 'askCmdr.tool.memoryWrite.doing', done: 'askCmdr.tool.memoryWrite.done' },
  memory_edit: { doing: 'askCmdr.tool.memoryEdit.doing', done: 'askCmdr.tool.memoryEdit.done' },
}

const UNKNOWN_TOOL_KEYS = { doing: 'askCmdr.tool.unknown.doing', done: 'askCmdr.tool.unknown.done' } as const

/** The localized label for a tool line, in its running or finished phase. An unrecognized
 * tool name (a refused/hallucinated call) falls to the generic label. */
export function toolLabel(tool: string, running: boolean): string {
  const keys = TOOL_LABEL_KEYS[tool] ?? UNKNOWN_TOOL_KEYS
  return tString(running ? keys.doing : keys.done)
}

/** The label for a tool call that was refused (read-only guard, or a handler problem). */
export function toolRefusedLabel(): string {
  return tString('askCmdr.tool.refused')
}

const ERROR_KEYS: Record<AskCmdrErrorKind, MessageKey> = {
  noKey: 'askCmdr.error.noKey',
  notConfigured: 'askCmdr.error.notConfigured',
  noConsent: 'askCmdr.error.noConsent',
  localWindowTooSmall: 'askCmdr.error.localWindowTooSmall',
  unavailable: 'askCmdr.error.unavailable',
  timeout: 'askCmdr.error.timeout',
  authFailed: 'askCmdr.error.authFailed',
  rateLimited: 'askCmdr.error.rateLimited',
  budgetExhausted: 'askCmdr.error.budgetExhausted',
  unfinishedReply: 'askCmdr.error.unfinishedReply',
  provider: 'askCmdr.error.provider',
}

/** The friendly, honest message for a typed turn failure (never the words error/failed). */
export function errorMessage(kind: AskCmdrErrorKind): string {
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
