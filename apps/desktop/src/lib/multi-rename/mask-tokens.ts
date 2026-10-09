/**
 * Placeholder tokens in a rename mask, for the inline editors of `MaskInput`.
 *
 * The scan mirrors the backend's mask parser (`src-tauri/src/multi_rename/mask.rs`): `[[` is a literal
 * bracket, a placeholder runs from `[` to the next `]`, and an unclosed `[` ends the scan (the backend
 * rejects the mask from there). Positions are UTF-16 offsets, so they index the text field directly.
 * Pure.
 */

/** One `[…]` in the mask: `from` is the `[`, `to` is just past the `]`. */
export interface PlaceholderSpan {
  from: number
  to: number
  /** The text between the brackets, `C10:3` for `[C10:3]`. */
  inner: string
}

/**
 * A kind of placeholder that has an editor: how to read one and how to write it back. Method syntax
 * on purpose, so a kind of any value type fits `MaskTokenGrammar<unknown>`.
 */
export interface MaskTokenGrammar<V> {
  id: string
  /** The value of a placeholder of this kind, or `null` when `inner` isn't one. */
  parse(inner: string): V | null
  /** The placeholder's text for `value`, without the brackets, in its shortest form. */
  format(value: V): string
}

/** A placeholder some kind in `K` can edit, with its value. */
export type MaskToken<K> = K extends MaskTokenGrammar<infer V> ? { span: PlaceholderSpan; kind: K; value: V } : never

export function scanPlaceholders(mask: string): PlaceholderSpan[] {
  const spans: PlaceholderSpan[] = []
  let i = 0
  while (i < mask.length) {
    if (mask[i] !== '[') {
      i += 1
      continue
    }
    if (mask[i + 1] === '[') {
      i += 2
      continue
    }
    const close = mask.indexOf(']', i + 1)
    if (close === -1) break
    spans.push({ from: i, to: close + 1, inner: mask.slice(i + 1, close) })
    i = close + 1
  }
  return spans
}

/** Every placeholder in `mask` that one of `kinds` reads, in text order. */
export function findTokens<K extends MaskTokenGrammar<unknown>>(mask: string, kinds: readonly K[]): MaskToken<K>[] {
  const tokens: MaskToken<K>[] = []
  for (const span of scanPlaceholders(mask)) {
    for (const kind of kinds) {
      const value = kind.parse(span.inner)
      if (value === null) continue
      tokens.push({ span, kind, value } as MaskToken<K>)
      break
    }
  }
  return tokens
}

/** The token the caret is inside of, or right after. */
export function tokenAtCaret<T extends { span: PlaceholderSpan }>(tokens: readonly T[], caret: number): T | undefined {
  return tokens.find((token) => caret > token.span.from && caret <= token.span.to)
}

/** `mask` with the placeholder at `span` rewritten to `[inner]`, and the caret just past it. */
export function replaceToken(mask: string, span: PlaceholderSpan, inner: string): { mask: string; caret: number } {
  const token = `[${inner}]`
  return { mask: mask.slice(0, span.from) + token + mask.slice(span.to), caret: span.from + token.length }
}
