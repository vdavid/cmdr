// Selection dialog IPC commands: typed wrappers around the Rust backend (M5).

import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'
import type { SelectionHistoryEntry, SelectionTranslateResult } from '$lib/ipc/bindings'

/**
 * Translates a natural-language selection request into a glob/regex plus optional
 * size and date filters. Cloud-only: the backend rejects the call when the AI
 * provider isn't `cloud` (small local models can't reliably handle a 200+-name
 * folder sample plus the structured prompt).
 */
export async function translateSelectionQuery(
  prompt: string,
  sampleNames: string[],
): Promise<SelectionTranslateResult> {
  const res = await commands.translateSelectionQuery(prompt, sampleNames)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Returns the persisted recent-selections entries (newest first). `limit = null` returns all. */
export async function getRecentSelections(limit: number | null = null): Promise<SelectionHistoryEntry[]> {
  return commands.getRecentSelections(limit)
}

/** Adds an entry to the recent-selections store. The backend dedupes by canonical key and caps. */
export async function addRecentSelection(entry: SelectionHistoryEntry, maxCount: number | null = null): Promise<void> {
  const res = await commands.addRecentSelection(entry, maxCount)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Removes a single recent-selection entry by id. No-op if the id isn't present. */
export async function removeRecentSelection(id: string): Promise<void> {
  const res = await commands.removeRecentSelection(id)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Clears every recent-selection entry. */
export async function clearRecentSelections(): Promise<void> {
  const res = await commands.clearRecentSelections()
  if (res.status === 'error') throwIpcError(res.error)
}

/** Live-applies a new `selection.recentSelections.maxCount` cap. */
export async function applyRecentSelectionsMaxCount(maxCount: number): Promise<void> {
  const res = await commands.applyRecentSelectionsMaxCount(maxCount)
  if (res.status === 'error') throwIpcError(res.error)
}
