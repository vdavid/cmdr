/**
 * The five search options, shown as toggle chips beside the search fields: each one's glyph,
 * full name, and a tiny replace on a made-up file that shows what it changes, rendered by the
 * real engine with the option on and off (`rename-examples.ts`). The replacement travels
 * between the marks, so the tooltip sets apart exactly what the replace put in. Pure.
 */

import type { MessageKey } from '$lib/intl/keys.gen'
import type { RenameExample } from '$lib/tauri-commands'
import type { ToggleField } from './option-keys'
import { example, marked } from './rename-examples'

/** The options that tune search & replace; Remove diacritics changes the whole name. */
export type SearchOptionField = Exclude<ToggleField, 'removeDiacritics'>

export interface SearchOptionHelp {
  field: SearchOptionField
  /** The chip's face: a few characters, the way code editors draw find options. */
  glyph: string
  label: MessageKey
  /** The replace the tooltip shows, on a file named so the option makes a difference. */
  fileName: string
  search: string
  replace: string
}

/** The chips, in order. */
export const SEARCH_OPTIONS: readonly SearchOptionHelp[] = [
  {
    field: 'caseSensitive',
    glyph: 'Aa',
    label: 'multiRename.caseSensitive',
    fileName: 'Photo photo.jpg',
    search: 'photo',
    replace: 'pic',
  },
  {
    field: 'firstOnly',
    glyph: '1×',
    label: 'multiRename.firstOnly',
    fileName: '2026_07_14.jpg',
    search: '_',
    replace: '-',
  },
  {
    field: 'includeExtension',
    glyph: '.ext',
    label: 'multiRename.includeExtension',
    fileName: 'beach.jpeg',
    search: 'jpeg',
    replace: 'jpg',
  },
  {
    field: 'regex',
    glyph: '.*',
    label: 'multiRename.regex',
    fileName: 'IMG_0042.jpg',
    search: '\\d+',
    replace: '#',
  },
  {
    field: 'substitute',
    glyph: '^$',
    label: 'multiRename.substitute',
    fileName: 'IMG_0042.jpg',
    search: 'IMG',
    replace: 'Lisbon',
  },
]

/** The key of `field`'s example with the option on or off. */
export function searchExampleKey(field: SearchOptionField, on: boolean): string {
  return `${field}:${on ? 'on' : 'off'}`
}

/** Each option's replace, once with it on and once off (every other option off). */
export function searchOptionExamples(options: readonly SearchOptionHelp[]): Map<string, RenameExample> {
  const asked = new Map<string, RenameExample>()
  for (const { field, fileName, search, replace } of options) {
    for (const on of [true, false]) {
      asked.set(searchExampleKey(field, on), example(fileName, { search, replace: marked(replace), [field]: on }))
    }
  }
  return asked
}
