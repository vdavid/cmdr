// Multi-Rename Tool (⌃M): preview, apply, and presets. The work is the backend's
// (`src-tauri/src/multi_rename/`); these are pass-throughs.

import {
  commands,
  type ExpectedRename,
  type MultiRenameError,
  type MultiRenamePreset,
  type MultiRenameSpec,
  type MultiRenameStarted,
  type PreviewRow,
} from '$lib/ipc/bindings'

export type { ExpectedRename, MultiRenameError, MultiRenamePreset, MultiRenameSpec, MultiRenameStarted, PreviewRow }

/** A typed answer the sheet words itself: the rows, or why there are none. */
export type MultiRenameResult<T> = { ok: true; value: T } | { ok: false; error: MultiRenameError }

/**
 * Each row's new name and whether it can take it. `rows` are backend row numbers
 * in rename order; `null` previews every row the pane shows.
 */
export async function previewMultiRename(
  listingId: string,
  includeHidden: boolean,
  rows: number[] | null,
  spec: MultiRenameSpec,
): Promise<MultiRenameResult<PreviewRow[]>> {
  const res = await commands.previewMultiRename(listingId, includeHidden, rows, spec)
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
}

/**
 * Renames the rows the user saw as ready (`expected`), as one operation (queue,
 * Undo). `previewOutOfDate` when the folder changed since that preview.
 */
export async function applyMultiRename(
  listingId: string,
  includeHidden: boolean,
  rows: number[] | null,
  spec: MultiRenameSpec,
  expected: ExpectedRename[],
): Promise<MultiRenameResult<MultiRenameStarted>> {
  const res = await commands.applyMultiRename(listingId, includeHidden, rows, spec, expected)
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
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
