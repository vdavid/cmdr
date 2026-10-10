// Multi-Rename Tool (⌃M): the session, its preview pages, apply, Results, presets, the last settings, and the
// fields' history. The work and the file names are the backend's (`src-tauri/src/multi_rename/`); these are
// pass-throughs.

import {
  commands,
  type FieldHistoryEntry,
  type HistoryField,
  type LoadedPreset,
  type MultiRenameError,
  type MultiRenameLastSettings,
  type MultiRenameOpened,
  type MultiRenamePreset,
  type MultiRenamePreview,
  type MultiRenameSpec,
  type MultiRenameStarted,
  type PreviewCounts,
  type PreviewFilter,
  type PreviewRow,
  type RenameExample,
} from '$lib/ipc/bindings'

export type {
  FieldHistoryEntry,
  HistoryField,
  LoadedPreset,
  MultiRenameError,
  MultiRenameLastSettings,
  MultiRenameOpened,
  MultiRenamePreset,
  MultiRenamePreview,
  MultiRenameSpec,
  MultiRenameStarted,
  PreviewCounts,
  PreviewFilter,
  PreviewRow,
  RenameExample,
}

/** A typed answer the sheet words itself: the value, or why there is none. */
export type MultiRenameResult<T> = { ok: true; value: T } | { ok: false; error: MultiRenameError }

function result<T>(
  res: { status: 'ok'; data: T } | { status: 'error'; error: MultiRenameError },
): MultiRenameResult<T> {
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
}

/**
 * Opens a session over a pane's selection: `selectedIndices` are backend row
 * numbers in rename order (`null` for every row the pane shows), read at the
 * pane's applied `expectedSequence`. `selectionChanged` when they're stale.
 */
export async function openMultiRename(
  listingId: string,
  includeHidden: boolean,
  selectedIndices: number[] | null,
  expectedSequence: number,
): Promise<MultiRenameResult<MultiRenameOpened>> {
  return result(await commands.openMultiRename(listingId, includeHidden, selectedIndices, expectedSequence))
}

/** The session's preview of `spec`: its id, the counts, and the first rows. */
export async function previewMultiRename(
  sessionId: string,
  spec: MultiRenameSpec,
): Promise<MultiRenameResult<MultiRenamePreview>> {
  return result(await commands.previewMultiRename(sessionId, spec))
}

/**
 * Rows `offset..offset + limit` of a preview, counted among all its rows or its problem
 * rows alone (`filter`). `previewOutOfDate` once a newer one replaced it.
 */
export async function getMultiRenamePreviewRows(
  sessionId: string,
  previewId: number,
  offset: number,
  limit: number,
  filter: PreviewFilter,
): Promise<MultiRenameResult<PreviewRow[]>> {
  return result(await commands.getMultiRenamePreviewRows(sessionId, previewId, offset, limit, filter))
}

/**
 * Each example's new name, rendered by the rename engine on a made-up file, for the
 * sheet's tooltips; `null` for a spec that doesn't run.
 */
export async function renderMultiRenameExamples(examples: RenameExample[]): Promise<(string | null)[]> {
  return commands.renderMultiRenameExamples(examples)
}

/**
 * Renames the rows preview `previewId` showed as ready, as one operation (queue,
 * Undo). `previewOutOfDate` when the folder changed since that preview.
 */
export async function applyMultiRename(
  sessionId: string,
  previewId: number,
): Promise<MultiRenameResult<MultiRenameStarted>> {
  return result(await commands.applyMultiRename(sessionId, previewId))
}

/** Results (⌥⏎): writes preview `previewId`'s rows as `old<TAB>new` lines and returns the file's path, for the editor. */
export async function writeMultiRenameNames(sessionId: string, previewId: number): Promise<MultiRenameResult<string>> {
  return result(await commands.writeMultiRenameNames(sessionId, previewId))
}

/** Reads the session's Results file back; answers how many rows now carry a typed name. */
export async function readMultiRenameNames(sessionId: string): Promise<MultiRenameResult<number>> {
  return result(await commands.readMultiRenameNames(sessionId))
}

/** Drops the names typed in Results, and its file: every row follows the settings again. */
export async function clearMultiRenameNames(sessionId: string): Promise<MultiRenameResult<null>> {
  return result(await commands.clearMultiRenameNames(sessionId))
}

/** Ends the session (the sheet closed). */
export async function closeMultiRename(sessionId: string): Promise<void> {
  await commands.closeMultiRename(sessionId)
}

export async function getMultiRenamePresets(): Promise<MultiRenamePreset[]> {
  return commands.getMultiRenamePresets()
}

export async function saveMultiRenamePreset(preset: MultiRenamePreset): Promise<void> {
  await commands.saveMultiRenamePreset(preset)
}

export async function deleteMultiRenamePreset(id: string): Promise<void> {
  await commands.deleteMultiRenamePreset(id)
}

/** Renames a preset in place; a preset already called `name` is replaced. */
export async function renameMultiRenamePreset(id: string, name: string): Promise<void> {
  await commands.renameMultiRenamePreset(id, name)
}

/** Gives a preset new settings in place, so the menu's numbers don't move. */
export async function updateMultiRenamePreset(id: string, spec: MultiRenameSpec): Promise<void> {
  await commands.updateMultiRenamePreset(id, spec)
}

/** The settings the sheet last closed with, and the preset they came from; `null` before the first close. */
export async function getMultiRenameLastSettings(): Promise<MultiRenameLastSettings | null> {
  return commands.getMultiRenameLastSettings()
}

/** Remembers the settings the sheet closes with, and the preset they came from, for the next ⌃M. */
export async function saveMultiRenameLastSettings(spec: MultiRenameSpec, preset: LoadedPreset | null): Promise<void> {
  await commands.saveMultiRenameLastSettings(spec, preset)
}

/** What the text fields held when earlier renames ran, newest first, every field together. */
export async function getMultiRenameHistory(): Promise<FieldHistoryEntry[]> {
  return commands.getMultiRenameHistory()
}
