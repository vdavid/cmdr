/**
 * What each placeholder button's tooltip says: the placeholder's meaning, a few of the forms
 * `src-tauri/src/multi_rename/mask.rs` reads, and live examples. The examples are the backend's own
 * render of each mask for the batch's first file (`renderMultiRenameExamples`), so they show exactly
 * what a rename would; a range also renders what surrounds it, so the tooltip can set the part it
 * takes apart from the rest. Counters need no file: their numbers come from the token. Pure.
 */
import type { MessageKey } from '$lib/intl/keys.gen'
import type { MaskExamples } from '$lib/tauri-commands'
import { counterKind, counterSamples } from './counter-token'

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
  /** What the example line says it shows: the first file, the first files (a counter), or a date. */
  example: 'file' | 'counter' | 'date'
  syntax: SyntaxLine[]
  /** A closing line, after the forms. */
  footnote?: MessageKey
}

/** A piece of an example: the part the mask takes, or what it leaves around it. */
export interface ExamplePiece {
  text: string
  taken: boolean
}

/** The whole file name, `[2-5]`'s full-name range from the first character on. */
export const FULL_NAME_MASK = '[1-]'

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
    example: 'file',
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

/** Every mask the tooltips show an example of, once each, the full name first. Counters aren't asked for. */
export function exampleMasks(helps: readonly PlaceholderHelp[]): string[] {
  const masks = new Set([FULL_NAME_MASK])
  const add = (mask: string | undefined): void => {
    if (mask !== undefined && counterOf(mask) === null) masks.add(mask)
  }
  for (const help of helps) {
    add(help.placeholder)
    for (const line of help.syntax) {
      add(line.around?.before)
      add(line.mask)
      add(line.around?.after)
    }
  }
  return [...masks]
}

/** Each asked mask with its rendered text; one the backend couldn't render is left out. */
export function renderedByMask(masks: readonly string[], examples: MaskExamples): Map<string, string> {
  const rendered = new Map<string, string>()
  masks.forEach((mask, i) => {
    const text = examples.rendered[i]
    if (text !== null) rendered.set(mask, text)
  })
  return rendered
}

/**
 * The example of `hint`: the part it takes, with what the field keeps around it (empty pieces
 * dropped), a counter's first three numbers, or `null` with nothing rendered for it.
 */
export function examplePieces(hint: SyntaxHint, rendered: ReadonlyMap<string, string>): ExamplePiece[] | null {
  const counter = counterOf(hint.mask)
  if (counter !== null) return [{ text: `${counterSamples(counter).join(', ')}…`, taken: true }]
  const taken = rendered.get(hint.mask)
  if (taken === undefined) return null
  const pieces = [
    { text: hint.around?.before === undefined ? '' : (rendered.get(hint.around.before) ?? ''), taken: false },
    { text: taken, taken: true },
    { text: hint.around?.after === undefined ? '' : (rendered.get(hint.around.after) ?? ''), taken: false },
  ]
  return pieces.filter((piece) => piece.text !== '')
}
