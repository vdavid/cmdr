/**
 * The Multi-Rename sheet's settings: the default (Total Commander's `<Default>`:
 * no change), the presets Cmdr seeds the list with, comparing two specs, and placeholder insertion. Pure, so the
 * sheet's state module and its tests share it.
 */

import type { MultiRenameSpec } from '#lib/tauri-commands/index.js'

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
  greekToLatin: false,
  normalizeUnicode: false,
}

/**
 * A preset Cmdr starts the list with. Its name is a message key, read in the UI language when the
 * backend seeds the list (`presets.rs`'s `seed_in`); from then on it's an ordinary preset, the user's to
 * rename, update, move, or delete.
 */
export interface PresetSeedSource {
  nameKey:
    | 'multiRename.preset.dateAndTime'
    | 'multiRename.preset.removeDiacritics'
    | 'multiRename.preset.greekToLatin'
    | 'multiRename.preset.normalizeUnicode'
  spec: MultiRenameSpec
}

/** The seeds, top to bottom. */
export const PRESET_SEEDS: PresetSeedSource[] = [
  { nameKey: 'multiRename.preset.dateAndTime', spec: { ...DEFAULT_SPEC, nameMask: '[Y-M-D h.m.s]' } },
  { nameKey: 'multiRename.preset.removeDiacritics', spec: { ...DEFAULT_SPEC, removeDiacritics: true } },
  { nameKey: 'multiRename.preset.greekToLatin', spec: { ...DEFAULT_SPEC, greekToLatin: true } },
  { nameKey: 'multiRename.preset.normalizeUnicode', spec: { ...DEFAULT_SPEC, normalizeUnicode: true } },
]

/**
 * Whether two specs set the same thing. Field by field, so a spec that came back from
 * the backend (its own field order) still equals the one the sheet holds.
 */
export function specsEqual(a: MultiRenameSpec, b: MultiRenameSpec): boolean {
  return (Object.keys(DEFAULT_SPEC) as (keyof MultiRenameSpec)[]).every((field) => a[field] === b[field])
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
