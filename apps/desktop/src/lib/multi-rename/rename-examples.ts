/**
 * The sheet's tooltip examples: made-up files run through the real rename engine
 * (`renderMultiRenameExamples`), so an example shows exactly what a rename would do and can't
 * drift from it. The part an example is about (the slice a placeholder takes, the text a
 * replace puts in) travels inside the spec between two private-use marks, which the engine
 * copies through like any other text; the tooltip sets the marked part apart. Pure, but for
 * the one call.
 */

import { renderMultiRenameExamples, type MultiRenameSpec, type RenameExample } from '$lib/tauri-commands'
import { DEFAULT_SPEC } from './spec'

/** Opens and closes the marked part. Private-use characters: no real name or mask has them. */
export const MARK_START = ''
export const MARK_END = ''

/** The file most examples rename. The engine adds its folder (`Trips/Lisbon 2026`) and date. */
export const SAMPLE_FILE = 'Beach day.jpg'

/** A piece of an example's text: the marked part, or what's around it. */
export interface ExamplePiece {
  text: string
  marked: boolean
}

/** `text` between the marks. */
export function marked(text: string): string {
  return `${MARK_START}${text}${MARK_END}`
}

/** An example of `spec`'s fields (the rest no change) renaming `fileName`. */
export function example(fileName: string, spec: Partial<MultiRenameSpec>): RenameExample {
  return { fileName, spec: { ...DEFAULT_SPEC, ...spec } }
}

/** A rendered example as pieces, the marked ones set apart; empty pieces dropped. */
export function examplePieces(rendered: string): ExamplePiece[] {
  const pieces: ExamplePiece[] = []
  let rest = rendered
  while (rest !== '') {
    const start = rest.indexOf(MARK_START)
    if (start === -1) {
      pieces.push({ text: rest, marked: false })
      break
    }
    const end = rest.indexOf(MARK_END, start)
    const stop = end === -1 ? rest.length : end
    pieces.push({ text: rest.slice(0, start), marked: false }, { text: rest.slice(start + 1, stop), marked: true })
    rest = rest.slice(stop + 1)
  }
  return pieces.filter((piece) => piece.text !== '')
}

/** Renders every example in one call, keyed as asked; one the engine couldn't render is left out. */
export async function renderExamples(asked: ReadonlyMap<string, RenameExample>): Promise<Map<string, string>> {
  const keys = [...asked.keys()]
  const rendered = await renderMultiRenameExamples([...asked.values()])
  const byKey = new Map<string, string>()
  keys.forEach((key, i) => {
    const text = rendered[i]
    if (text !== null) byKey.set(key, text)
  })
  return byKey
}
