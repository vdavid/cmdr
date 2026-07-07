import { commands, type PastedClipboardFile } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type { PastedClipboardFile } from '$lib/ipc/bindings'

export interface ClipboardReadResult {
  paths: string[]
  isCut: boolean
  /** Per-path top-level kind, index-aligned with `paths`: `true` = directory,
   *  `false` = file, `null` = unknown (stat failed). Lets the paste completion
   *  toast split files vs. folders without walking trees. */
  isDirectory: (boolean | null)[]
}

export async function copyFilesToClipboard(
  listingId: string,
  selectedIndices: number[],
  cursorIndex: number,
  hasParent: boolean,
  includeHidden: boolean,
): Promise<number> {
  const res = await commands.copyFilesToClipboard(listingId, selectedIndices, cursorIndex, hasParent, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

export async function cutFilesToClipboard(
  listingId: string,
  selectedIndices: number[],
  cursorIndex: number,
  hasParent: boolean,
  includeHidden: boolean,
): Promise<number> {
  const res = await commands.cutFilesToClipboard(listingId, selectedIndices, cursorIndex, hasParent, includeHidden)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Writes the given paths to the system clipboard as a copy. Paths-by-value
 * sibling of `copyFilesToClipboard`, used by the search-results pane where
 * there's no backend listing to resolve indices against.
 */
export async function copyPathsToClipboard(paths: string[]): Promise<number> {
  const res = await commands.copyPathsToClipboard(paths)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Writes the given paths to the system clipboard and marks them as cut.
 * Paths-by-value sibling of `cutFilesToClipboard`.
 */
export async function cutPathsToClipboard(paths: string[]): Promise<number> {
  const res = await commands.cutPathsToClipboard(paths)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

export async function readClipboardFiles(): Promise<ClipboardReadResult> {
  const res = await commands.readClipboardFiles()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

export async function readClipboardText(): Promise<string | null> {
  const res = await commands.readClipboardText()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

export async function clearClipboardCutState(): Promise<void> {
  await commands.clearClipboardCutState()
}

/**
 * Reads the highest-intent non-file clipboard flavor (image / PDF / text) and
 * writes it into `directory` as a new `pasted.<ext>` file, returning the created
 * file's name + kind. Resolves to `null` when nothing pasteable is on the
 * clipboard — the caller treats that as "no file created" (today's warn toast),
 * not an error.
 */
export async function pasteClipboardAsFile(
  volumeId: string | null,
  directory: string,
): Promise<PastedClipboardFile | null> {
  const res = await commands.pasteClipboardAsFile(volumeId, directory)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}
