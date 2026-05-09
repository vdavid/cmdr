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
