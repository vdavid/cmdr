/**
 * Markdown escaping for friendly-error copy: the XSS-load-bearing security boundary.
 *
 * Friendly-error explanations and suggestions are composed on the frontend from
 * trusted template literals plus untrusted runtime params (paths, OS messages,
 * device names, free-form provider text). The composed string is rendered by
 * `snarkdown` and `{@html}`-injected (see `error-pane-utils.ts`), so EVERY
 * interpolated runtime value MUST pass through `escapeMarkdown` before reaching
 * the template. Template literals are the only trusted markdown; params are
 * never trusted.
 *
 * This is a verbatim port of the Rust `escape()` (formerly in
 * `friendly_error/markdown.rs`): the same HTML-entity set, the same line-start
 * carve-outs.
 *
 * Why entities and not CommonMark `\` escapes: snarkdown is a tiny non-CommonMark
 * parser. It does NOT honor backslash escapes (emitting `STATUS\_DELETE\_PENDING`
 * would render visibly with backslashes). HTML entities sidestep snarkdown
 * entirely (it doesn't recognize them), and the browser decodes them when the
 * result is `{@html}`-injected.
 *
 * `&` is encoded first so any preexisting entity-like text is neutralized.
 */

/**
 * snarkdown characters meaningful regardless of position in a line. We
 * intentionally do NOT escape `.`, `-`, `+`, `#`, `|`: they only have markdown
 * meaning at the start of a line, and over-escaping them shows up as ugly
 * entities mid-sentence. Runtime values land mid-sentence in our templates, so
 * line-start chars stay innocuous.
 */
function isMdSpecial(c: string): boolean {
  return (
    c === '\\' ||
    c === '`' ||
    c === '*' ||
    c === '_' ||
    c === '[' ||
    c === ']' ||
    c === '(' ||
    c === ')' ||
    c === '!' ||
    c === '<' ||
    c === '>' ||
    c === '~'
  )
}

/**
 * Encode markdown-meaningful characters as HTML numeric entities so they pass
 * through snarkdown without triggering formatting, then render as the original
 * characters in the browser. The `&` is encoded first.
 */
// eslint-disable-next-line complexity -- flat per-character escape scan; the branch count is the HTML-entity set, not nested logic. Keeping it as one function preserves the 1:1 port of the Rust escaper (the XSS boundary).
export function escapeMarkdown(s: string): string {
  let needsEscape = false
  for (const c of s) {
    if (c === '&' || isMdSpecial(c)) {
      needsEscape = true
      break
    }
  }
  if (!needsEscape) return s

  let out = ''
  for (const c of s) {
    switch (c) {
      case '&':
        out += '&amp;'
        break
      case '<':
        out += '&lt;'
        break
      case '>':
        out += '&gt;'
        break
      case '\\':
        out += '&#92;'
        break
      case '`':
        out += '&#96;'
        break
      case '*':
        out += '&#42;'
        break
      case '_':
        out += '&#95;'
        break
      case '[':
        out += '&#91;'
        break
      case ']':
        out += '&#93;'
        break
      case '(':
        out += '&#40;'
        break
      case ')':
        out += '&#41;'
        break
      case '!':
        out += '&#33;'
        break
      case '~':
        out += '&#126;'
        break
      default:
        out += c
    }
  }
  return out
}
