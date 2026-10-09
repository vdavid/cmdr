// Multi-Rename Tool (⌃M): the session, its preview pages, apply, and presets. The
// work and the file names are the backend's (`src-tauri/src/multi_rename/`); these
// are pass-throughs.

import {
  commands,
  type MaskExamples,
  type MultiRenameError,
  type MultiRenameOpened,
  type MultiRenamePreset,
  type MultiRenamePreview,
  type MultiRenameSpec,
  type MultiRenameStarted,
  type PreviewCounts,
  type PreviewFilter,
  type PreviewRow,
} from '$lib/ipc/bindings'

export type {
  MaskExamples,
  MultiRenameError,
  MultiRenameOpened,
  MultiRenamePreset,
  MultiRenamePreview,
  MultiRenameSpec,
  MultiRenameStarted,
  PreviewCounts,
  PreviewFilter,
  PreviewRow,
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
 * `masks` rendered for the session's first file, for the placeholder tooltips' examples.
 * `null` when there's no first file to show (it's gone, or the session is).
 */
export async function renderMultiRenameExamples(sessionId: string, masks: string[]): Promise<MaskExamples | null> {
  const res = await commands.renderMultiRenameExamples(sessionId, masks)
  return res.status === 'ok' ? res.data : null
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
