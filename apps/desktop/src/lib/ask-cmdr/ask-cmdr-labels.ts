/**
 * Typed maps from wire enum values to localized strings for the Ask Cmdr rail. Kept as
 * literal records so every catalog key has a static call site (`desktop-message-keys-unused`
 * would flag a key only reached through a computed prefix).
 */

import { tString } from '$lib/intl/messages.svelte'
import type { AskCmdrErrorKind } from '$lib/tauri-commands'
import type { MessageKey } from '$lib/intl/keys.gen'

/** Present-tense (running) and past-tense (done) label keys per read-only tool. */
const TOOL_LABEL_KEYS: Record<string, { doing: MessageKey; done: MessageKey }> = {
  app_state: { doing: 'askCmdr.tool.appState.doing', done: 'askCmdr.tool.appState.done' },
  list_dir: { doing: 'askCmdr.tool.listDir.doing', done: 'askCmdr.tool.listDir.done' },
  list_pane_files: { doing: 'askCmdr.tool.listDir.doing', done: 'askCmdr.tool.listDir.done' },
  largest_dirs: { doing: 'askCmdr.tool.largestDirs.doing', done: 'askCmdr.tool.largestDirs.done' },
  important_folders: { doing: 'askCmdr.tool.importantFolders.doing', done: 'askCmdr.tool.importantFolders.done' },
  folder_importance: { doing: 'askCmdr.tool.folderImportance.doing', done: 'askCmdr.tool.folderImportance.done' },
  list_volumes: { doing: 'askCmdr.tool.listVolumes.doing', done: 'askCmdr.tool.listVolumes.done' },
  operations_list: { doing: 'askCmdr.tool.operationsList.doing', done: 'askCmdr.tool.operationsList.done' },
  operations_get: { doing: 'askCmdr.tool.operationsGet.doing', done: 'askCmdr.tool.operationsGet.done' },
  search_photos: { doing: 'askCmdr.tool.searchPhotos.doing', done: 'askCmdr.tool.searchPhotos.done' },
  image_facts: { doing: 'askCmdr.tool.imageFacts.doing', done: 'askCmdr.tool.imageFacts.done' },
  propose_rename_plan: { doing: 'askCmdr.tool.proposeRenamePlan.doing', done: 'askCmdr.tool.proposeRenamePlan.done' },
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
