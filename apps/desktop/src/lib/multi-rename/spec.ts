/**
 * The Multi-Rename sheet's settings: the default (Total Commander's `<Default>`:
 * no change), the built-in presets, and the counts the footer shows. Pure, so the
 * sheet's state module and its tests share it.
 */

import type { MultiRenameSpec, PreviewRow } from '$lib/tauri-commands'

/** TC's `<Default>`: every name stays as it is. */
export const DEFAULT_SPEC: MultiRenameSpec = {
  nameMask: '[N]',
  extensionMask: '[E]',
  search: '',
  replace: '',
  caseSensitive: false,
  firstOnly: false,
  includeExtension: false,
  regex: false,
  substitute: false,
  case: 'unchanged',
  removeDiacritics: false,
  counterStart: 1,
  counterStep: 1,
  counterDigits: 1,
}

/** A preset that ships with Cmdr. Its name is a message key, so it's translated. */
export interface BuiltInPreset {
  id: string
  nameKey: 'multiRename.preset.removeDiacritics'
  spec: MultiRenameSpec
}

export const BUILT_IN_PRESETS: BuiltInPreset[] = [
  {
    id: 'builtin:remove-diacritics',
    nameKey: 'multiRename.preset.removeDiacritics',
    spec: { ...DEFAULT_SPEC, removeDiacritics: true },
  },
]

/** How the preview's rows add up, for the footer and the Rename button. */
export interface PreviewCounts {
  ready: number
  unchanged: number
  problems: number
}

export function countPreview(rows: PreviewRow[]): PreviewCounts {
  let ready = 0
  let unchanged = 0
  for (const row of rows) {
    if (row.status.type === 'ready') ready++
    else if (row.status.type === 'unchanged') unchanged++
  }
  return { ready, unchanged, problems: rows.length - ready - unchanged }
}

/** Inserts `placeholder` into `mask` at the caret (or at the end). Returns the new mask and caret. */
export function insertAtCaret(
  mask: string,
  placeholder: string,
  caret: number | null,
): { mask: string; caret: number } {
  const at = caret === null ? mask.length : Math.max(0, Math.min(caret, mask.length))
  return { mask: mask.slice(0, at) + placeholder + mask.slice(at), caret: at + placeholder.length }
}
