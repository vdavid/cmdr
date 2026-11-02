/**
 * Which catalog message each tool's line says, per phase. Kept as literal records so every catalog
 * key has a static call site (`desktop-message-keys-unused` would flag a key only reached through a
 * computed prefix). `ask-cmdr-labels.ts` picks among them.
 */

import type { MessageKey } from '#lib/intl/keys.gen.js'

/**
 * A tool line's label keys: present tense while it runs, past tense once it's done, and, where
 * the past tense would claim a success a refusal didn't have ("Saved a note"), the words for
 * having tried. `waiting` is for a tool whose past tense would claim what it left to a person
 * ("Created a folder", when it only opened the dialog that names one).
 */
export interface PhaseKeys {
  doing: MessageKey
  done: MessageKey
  tried?: MessageKey
  waiting?: MessageKey
}

/**
 * Param-less label keys per tool: what a line says when its subject names nothing. One entry per
 * entry in the tool table, or the line falls to the generic words
 * (`every_known_tool_has_an_ask_cmdr_rail_label`).
 */
export const TOOL_LABEL_KEYS: Partial<Record<string, PhaseKeys>> = {
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
  select_volume: {
    doing: 'askCmdr.tool.selectVolume.doing',
    done: 'askCmdr.tool.selectVolume.done',
    tried: 'askCmdr.tool.selectVolume.tried',
  },
  nav_to_path: {
    doing: 'askCmdr.tool.navToPath.doing',
    done: 'askCmdr.tool.navToPath.done',
    tried: 'askCmdr.tool.navToPath.tried',
  },
  nav_to_parent: {
    doing: 'askCmdr.tool.navToParent.doing',
    done: 'askCmdr.tool.navToParent.done',
    tried: 'askCmdr.tool.navToParent.tried',
  },
  nav_back: {
    doing: 'askCmdr.tool.navBack.doing',
    done: 'askCmdr.tool.navBack.done',
    tried: 'askCmdr.tool.navBack.tried',
  },
  nav_forward: {
    doing: 'askCmdr.tool.navForward.doing',
    done: 'askCmdr.tool.navForward.done',
    tried: 'askCmdr.tool.navForward.tried',
  },
  scroll_to: {
    doing: 'askCmdr.tool.scrollTo.doing',
    done: 'askCmdr.tool.scrollTo.done',
    tried: 'askCmdr.tool.scrollTo.tried',
  },
  move_cursor: {
    doing: 'askCmdr.tool.moveCursor.doing',
    done: 'askCmdr.tool.moveCursor.done',
    tried: 'askCmdr.tool.moveCursor.tried',
  },
  open_under_cursor: {
    doing: 'askCmdr.tool.openUnderCursor.doing',
    done: 'askCmdr.tool.openUnderCursor.done',
    tried: 'askCmdr.tool.openUnderCursor.tried',
  },
  select: {
    doing: 'askCmdr.tool.select.doing',
    done: 'askCmdr.tool.select.done',
    tried: 'askCmdr.tool.select.tried',
  },
  copy: {
    doing: 'askCmdr.tool.copy.doing',
    done: 'askCmdr.tool.copy.done',
    tried: 'askCmdr.tool.copy.tried',
  },
  move: {
    doing: 'askCmdr.tool.move.doing',
    done: 'askCmdr.tool.move.done',
    tried: 'askCmdr.tool.move.tried',
  },
  compress: {
    doing: 'askCmdr.tool.compress.doing',
    done: 'askCmdr.tool.compress.done',
    tried: 'askCmdr.tool.compress.tried',
  },
  delete: {
    doing: 'askCmdr.tool.delete.doing',
    done: 'askCmdr.tool.delete.done',
    tried: 'askCmdr.tool.delete.tried',
  },
  rename: {
    doing: 'askCmdr.tool.rename.doing',
    done: 'askCmdr.tool.rename.done',
    tried: 'askCmdr.tool.rename.tried',
  },
  multi_rename: {
    doing: 'askCmdr.tool.multiRename.doing',
    done: 'askCmdr.tool.multiRename.done',
    tried: 'askCmdr.tool.multiRename.tried',
  },
  mkdir: {
    doing: 'askCmdr.tool.mkdir.doing',
    done: 'askCmdr.tool.mkdir.done',
    tried: 'askCmdr.tool.mkdir.tried',
    waiting: 'askCmdr.tool.mkdir.waiting',
  },
  mkfile: {
    doing: 'askCmdr.tool.mkfile.doing',
    done: 'askCmdr.tool.mkfile.done',
    tried: 'askCmdr.tool.mkfile.tried',
    waiting: 'askCmdr.tool.mkfile.waiting',
  },
  refresh: {
    doing: 'askCmdr.tool.refresh.doing',
    done: 'askCmdr.tool.refresh.done',
    tried: 'askCmdr.tool.refresh.tried',
  },
  tag: {
    doing: 'askCmdr.tool.tag.doing',
    done: 'askCmdr.tool.tag.done',
    tried: 'askCmdr.tool.tag.tried',
  },
  toggle_hidden: {
    doing: 'askCmdr.tool.toggleHidden.doing',
    done: 'askCmdr.tool.toggleHidden.done',
    tried: 'askCmdr.tool.toggleHidden.tried',
  },
  set_view_mode: {
    doing: 'askCmdr.tool.setViewMode.doing',
    done: 'askCmdr.tool.setViewMode.done',
    tried: 'askCmdr.tool.setViewMode.tried',
  },
  sort: {
    doing: 'askCmdr.tool.sort.doing',
    done: 'askCmdr.tool.sort.done',
    tried: 'askCmdr.tool.sort.tried',
  },
  tab: {
    doing: 'askCmdr.tool.tab.doing',
    done: 'askCmdr.tool.tab.done',
    tried: 'askCmdr.tool.tab.tried',
  },
  dialog: {
    doing: 'askCmdr.tool.dialog.doing',
    done: 'askCmdr.tool.dialog.done',
    tried: 'askCmdr.tool.dialog.tried',
  },
  dialog_confirm: {
    doing: 'askCmdr.tool.dialogConfirm.doing',
    done: 'askCmdr.tool.dialogConfirm.done',
    tried: 'askCmdr.tool.dialogConfirm.tried',
  },
  open_search_dialog: {
    doing: 'askCmdr.tool.openSearchDialog.doing',
    done: 'askCmdr.tool.openSearchDialog.done',
    tried: 'askCmdr.tool.openSearchDialog.tried',
  },
  quit: {
    doing: 'askCmdr.tool.quit.doing',
    done: 'askCmdr.tool.quit.done',
    tried: 'askCmdr.tool.quit.tried',
  },
  switch_pane: {
    doing: 'askCmdr.tool.switchPane.doing',
    done: 'askCmdr.tool.switchPane.done',
    tried: 'askCmdr.tool.switchPane.tried',
  },
  swap_panes: {
    doing: 'askCmdr.tool.swapPanes.doing',
    done: 'askCmdr.tool.swapPanes.done',
    tried: 'askCmdr.tool.swapPanes.tried',
  },
  ai_search: {
    doing: 'askCmdr.tool.aiSearch.doing',
    done: 'askCmdr.tool.aiSearch.done',
  },
  read_resource: {
    doing: 'askCmdr.tool.readResource.doing',
    done: 'askCmdr.tool.readResource.done',
    tried: 'askCmdr.tool.readResource.tried',
  },
  set_setting: {
    doing: 'askCmdr.tool.setSetting.doing',
    done: 'askCmdr.tool.setSetting.done',
    tried: 'askCmdr.tool.setSetting.tried',
  },
  indexing: {
    doing: 'askCmdr.tool.indexing.doing',
    done: 'askCmdr.tool.indexing.done',
    tried: 'askCmdr.tool.indexing.tried',
  },
  queue: {
    doing: 'askCmdr.tool.queue.doing',
    done: 'askCmdr.tool.queue.done',
    tried: 'askCmdr.tool.queue.tried',
  },
  queue_move_locked: {
    doing: 'askCmdr.tool.queueMoveLocked.doing',
    done: 'askCmdr.tool.queueMoveLocked.done',
    tried: 'askCmdr.tool.queueMoveLocked.tried',
  },
  resolve_conflict: {
    doing: 'askCmdr.tool.resolveConflict.doing',
    done: 'askCmdr.tool.resolveConflict.done',
    tried: 'askCmdr.tool.resolveConflict.tried',
  },
  unlock: {
    doing: 'askCmdr.tool.unlock.doing',
    done: 'askCmdr.tool.unlock.done',
    tried: 'askCmdr.tool.unlock.tried',
  },
  favorites: {
    doing: 'askCmdr.tool.favorites.doing',
    done: 'askCmdr.tool.favorites.done',
    tried: 'askCmdr.tool.favorites.tried',
  },
  connect_to_server: {
    doing: 'askCmdr.tool.connectToServer.doing',
    done: 'askCmdr.tool.connectToServer.done',
    tried: 'askCmdr.tool.connectToServer.tried',
  },
  remove_manual_server: {
    doing: 'askCmdr.tool.removeManualServer.doing',
    done: 'askCmdr.tool.removeManualServer.done',
    tried: 'askCmdr.tool.removeManualServer.tried',
  },
  upgrade_smb_to_direct: {
    doing: 'askCmdr.tool.upgradeSmbToDirect.doing',
    done: 'askCmdr.tool.upgradeSmbToDirect.done',
    tried: 'askCmdr.tool.upgradeSmbToDirect.tried',
  },
  eject: {
    doing: 'askCmdr.tool.eject.doing',
    done: 'askCmdr.tool.eject.done',
    tried: 'askCmdr.tool.eject.tried',
  },
  await: {
    doing: 'askCmdr.tool.await.doing',
    done: 'askCmdr.tool.await.done',
  },
  go_to_latest_download: {
    doing: 'askCmdr.tool.goToLatestDownload.doing',
    done: 'askCmdr.tool.goToLatestDownload.done',
    tried: 'askCmdr.tool.goToLatestDownload.tried',
  },
  operations_rollback: {
    doing: 'askCmdr.tool.operationsRollback.doing',
    done: 'askCmdr.tool.operationsRollback.done',
    tried: 'askCmdr.tool.operationsRollback.tried',
  },
  memory_diagnostics: {
    doing: 'askCmdr.tool.memoryDiagnostics.doing',
    done: 'askCmdr.tool.memoryDiagnostics.done',
  },
  cpu_diagnostics: {
    doing: 'askCmdr.tool.cpuDiagnostics.doing',
    done: 'askCmdr.tool.cpuDiagnostics.done',
  },
}

export const UNKNOWN_TOOL_KEYS: PhaseKeys = { doing: 'askCmdr.tool.unknown.doing', done: 'askCmdr.tool.unknown.done' }

export const LIST_FOLDER_KEYS: PhaseKeys = {
  doing: 'askCmdr.tool.listDir.folder.doing',
  done: 'askCmdr.tool.listDir.folder.done',
}

/** Label keys that name one folder, per tool whose subject is a `folder`. */
export const FOLDER_KEYS: Partial<Record<string, PhaseKeys>> = {
  list_dir: LIST_FOLDER_KEYS,
  folder_importance: {
    doing: 'askCmdr.tool.folderImportance.folder.doing',
    done: 'askCmdr.tool.folderImportance.folder.done',
  },
}

/** Label keys that name the first of a call's files, per tool whose subject is `files`. */
export const FILES_KEYS: Partial<Record<string, PhaseKeys>> = {
  inspect_file: { doing: 'askCmdr.tool.inspectFile.files.doing', done: 'askCmdr.tool.inspectFile.files.done' },
  image_facts: { doing: 'askCmdr.tool.imageFacts.files.doing', done: 'askCmdr.tool.imageFacts.files.done' },
}

/** Label keys that name a note file, per tool whose subject is a `memoryFile`. */
export const MEMORY_FILE_KEYS: Partial<Record<string, PhaseKeys>> = {
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

/** Label keys that name one place, per tool whose subject is a `place`. */
export const PLACE_KEYS: Partial<Record<string, PhaseKeys>> = {
  nav_to_path: {
    doing: 'askCmdr.tool.navToPath.place.doing',
    done: 'askCmdr.tool.navToPath.place.done',
    tried: 'askCmdr.tool.navToPath.place.tried',
  },
  unlock: {
    doing: 'askCmdr.tool.unlock.place.doing',
    done: 'askCmdr.tool.unlock.place.done',
    tried: 'askCmdr.tool.unlock.place.tried',
  },
}
