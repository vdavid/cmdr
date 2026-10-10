/**
 * What each placeholder button's tooltip says: the placeholder's meaning, a few of the forms
 * `src-tauri/src/multi_rename/mask.rs` reads, and examples on a made-up file
 * (`rename-examples.ts`), rendered by the real engine. A range's example renders what the field
 * keeps around it too, with the part it takes between the marks, so the tooltip can set that
 * part apart. Counters need no file: their numbers come from the token. Pure.
 */
import type { MessageKey } from '$lib/intl/keys.gen'
import type { RenameExample } from '$lib/tauri-commands'
import { counterKind, counterSamples } from './counter-token'
import { SAMPLE_FILE, example, examplePieces, marked, type ExamplePiece } from './rename-examples'

/** One form of a placeholder. */
export interface SyntaxHint {
  mask: string
  /** For a range, the masks of what the field keeps before and after the part it takes. */
  around?: { before?: string; after?: string }
}

export interface SyntaxLine extends SyntaxHint {
  meaning: MessageKey
  meaningParams?: Record<string, number>
}

export interface PlaceholderHelp {
  placeholder: string
  meaning: MessageKey
  /** What the example line says it shows: the sample file, it with its folders, three counts, or a date. */
  example: 'file' | 'folder' | 'counter' | 'date'
  syntax: SyntaxLine[]
  /** A closing line, after the forms. */
  footnote?: MessageKey
}

/** What the lead line names for a folder or date example, as masks the engine renders for the sample file. */
export const FOLDER_LEAD_MASK = '[G]/[P]/[N].[E]'
export const DATE_LEAD_MASK = '[Y]-[M]-[D] [h]:[m]:[s]'

/** The placeholder buttons, in order, with their help. */
export const PLACEHOLDER_HELP: readonly PlaceholderHelp[] = [
  {
    placeholder: '[N]',
    meaning: 'multiRename.placeholderHelp.name',
    example: 'file',
    syntax: [
      {
        mask: '[N3]',
        around: { before: '[N1-2]', after: '[N4-]' },
        meaning: 'multiRename.placeholderHelp.oneChar',
        meaningParams: { at: 3 },
      },
      {
        mask: '[N2-5]',
        around: { before: '[N1]', after: '[N6-]' },
        meaning: 'multiRename.placeholderHelp.charRange',
        meaningParams: { from: 2, to: 5 },
      },
      {
        mask: '[N-3-]',
        around: { before: '[N1--4]' },
        meaning: 'multiRename.placeholderHelp.lastChars',
        meaningParams: { count: 3 },
      },
    ],
  },
  {
    placeholder: '[E]',
    meaning: 'multiRename.placeholderHelp.extension',
    example: 'file',
    syntax: [
      { mask: '[E1]', around: { after: '[E2-]' }, meaning: 'multiRename.placeholderHelp.firstChar' },
      {
        mask: '[E2-3]',
        around: { before: '[E1]', after: '[E4-]' },
        meaning: 'multiRename.placeholderHelp.charRange',
        meaningParams: { from: 2, to: 3 },
      },
    ],
  },
  {
    placeholder: '[P]',
    meaning: 'multiRename.placeholderHelp.parent',
    example: 'folder',
    syntax: [
      {
        mask: '[P1-3]',
        around: { after: '[P4-]' },
        meaning: 'multiRename.placeholderHelp.firstChars',
        meaningParams: { count: 3 },
      },
      { mask: '[G]', meaning: 'multiRename.placeholderHelp.grandparent' },
    ],
  },
  {
    placeholder: '[C]',
    meaning: 'multiRename.placeholderHelp.counter',
    example: 'counter',
    syntax: [
      { mask: '[C10]', meaning: 'multiRename.placeholderHelp.counterStart', meaningParams: { start: 10 } },
      { mask: '[C+5]', meaning: 'multiRename.placeholderHelp.counterStep', meaningParams: { step: 5 } },
      { mask: '[C:3]', meaning: 'multiRename.placeholderHelp.counterDigits', meaningParams: { digits: 3 } },
      { mask: '[C10-1:2]', meaning: 'multiRename.placeholderHelp.counterAll' },
    ],
    footnote: 'multiRename.placeholderHelp.counterEditHint',
  },
  {
    placeholder: '[YMD]',
    meaning: 'multiRename.placeholderHelp.date',
    example: 'date',
    syntax: [
      { mask: '[d]', meaning: 'multiRename.placeholderHelp.isoDate' },
      { mask: '[yMD]', meaning: 'multiRename.placeholderHelp.shortYear' },
      { mask: '[D]', meaning: 'multiRename.placeholderHelp.datePart' },
    ],
  },
  {
    placeholder: '[hms]',
    meaning: 'multiRename.placeholderHelp.time',
    example: 'date',
    syntax: [
      { mask: '[t]', meaning: 'multiRename.placeholderHelp.dottedTime' },
      { mask: '[hm]', meaning: 'multiRename.placeholderHelp.hoursMinutes' },
    ],
  },
]

function counterOf(mask: string) {
  return mask.startsWith('[') && mask.endsWith(']') ? counterKind.parse(mask.slice(1, -1)) : null
}

/** `hint` as one name mask: what the field keeps around it, and the part it takes between the marks. */
export function hintMask(hint: SyntaxHint): string {
  return `${hint.around?.before ?? ''}${marked(hint.mask)}${hint.around?.after ?? ''}`
}

function maskExample(mask: string): RenameExample {
  return example(SAMPLE_FILE, { nameMask: mask, extensionMask: '' })
}

/** Every example the tooltips show, keyed by its mask, once each. Counters aren't asked for. */
export function placeholderExamples(helps: readonly PlaceholderHelp[]): Map<string, RenameExample> {
  const asked = new Map<string, RenameExample>()
  const add = (mask: string): void => {
    asked.set(mask, maskExample(mask))
  }
  add(FOLDER_LEAD_MASK)
  add(DATE_LEAD_MASK)
  for (const help of helps) {
    for (const hint of [{ mask: help.placeholder }, ...help.syntax]) {
      if (counterOf(hint.mask) === null) add(hintMask(hint))
    }
  }
  return asked
}

/**
 * The example of `hint`: the part it takes, with what the field keeps around it, a counter's
 * first three numbers, or `null` with nothing rendered for it.
 */
export function hintPieces(hint: SyntaxHint, rendered: ReadonlyMap<string, string>): ExamplePiece[] | null {
  const counter = counterOf(hint.mask)
  if (counter !== null) return [{ text: `${counterSamples(counter).join(', ')}…`, marked: true }]
  const text = rendered.get(hintMask(hint))
  return text === undefined ? null : examplePieces(text)
}
