import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export interface ClipboardReadResult {
  paths: string[]
  isCut: boolean
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
 * sibling of `copyFilesToClipboard`, used by the search-results pane (M8d)
 * where there's no backend listing to resolve indices against.
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
