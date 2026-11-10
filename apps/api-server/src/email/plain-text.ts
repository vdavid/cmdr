/**
 * The plain-text alternative for an email that has only HTML, derived from that HTML.
 *
 * Without one SMTP2Go still sends a `text/plain` part, but an empty one: a plain-text client (or a
 * notification preview that reads that part) shows a blank mail, and rspamd scores the mismatch
 * (`R_PARTS_DIFFER`). Our emails are a closed vocabulary (`layout.ts` and the templates beside it),
 * so a small converter covers them; it is not a general HTML-to-text engine.
 */

import { escapeHtml } from './layout'

/** Stands in for a typed line break inside a pre-wrap element while whitespace collapses around it. */
const KEPT_BREAK = '\u0002'
/** Stands in for a table-cell boundary until each line is laid out. */
const CELL = '\u0001'
/** A line-level block boundary (`div`, `tr`, `li`). */
const LINE = '\u0003'
/** A paragraph-level block boundary (`p`, headings, lists, tables). */
const PARAGRAPH = '\u0004'

const NAMED_ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: ' ',
  middot: '·',
  ndash: '–',
  mdash: '—',
  hellip: '…',
}

/** One pass, so `&amp;lt;` comes back as `&lt;`, the text the person actually wrote. */
function decodeEntities(text: string): string {
  return text.replace(/&(#\d+|#x[0-9a-f]+|[a-z]+);/gi, (whole: string, name: string) => {
    if (name.startsWith('#x') || name.startsWith('#X')) return String.fromCodePoint(parseInt(name.slice(2), 16))
    if (name.startsWith('#')) return String.fromCodePoint(parseInt(name.slice(1), 10))
    return NAMED_ENTITIES[name.toLowerCase()] ?? whole
  })
}

function stripTags(html: string): string {
  return html.replace(/<[^>]*>/g, '')
}

/** `label (address)`, or the bare address when the label already is it (a `mailto:` included). */
function linkText(href: string, labelHtml: string): string {
  const address = decodeEntities(href)
  const label = decodeEntities(stripTags(labelHtml)).trim()
  const target = address.startsWith('mailto:') ? address.slice('mailto:'.length) : address
  if (!label || label === target) return target
  return `${label} (${target})`
}

/** One output line: table cells joined with a bar, the empty trailing ones dropped. */
function layOutLine(line: string): string {
  if (!line.includes(CELL)) return line.trim()
  const cells = line.split(CELL).map((cell) => cell.trim())
  while (cells.length && cells[cells.length - 1] === '') cells.pop()
  return cells.join(' | ')
}

/** The readable text of one of our emails, paragraphs apart, links spelled out. */
export function htmlToText(html: string): string {
  const text = html
    .replace(/<head[\s\S]*?<\/head>/gi, '')
    .replace(/<style[\s\S]*?<\/style>/gi, '')
    // A person's message keeps its line breaks (the cards render it `white-space: pre-wrap`).
    .replace(
      /(<div [^>]*white-space: pre-wrap[^>]*>)([\s\S]*?)(<\/div>)/gi,
      (_whole, open: string, inner: string, close: string) => `${open}${inner.replace(/\r?\n/g, KEPT_BREAK)}${close}`,
    )
    // Before the tags go, so a label isn't torn from its address. Re-escaped, because the whole
    // text is stripped and decoded once more at the end.
    .replace(/<a\s[^>]*href="([^"]*)"[^>]*>([\s\S]*?)<\/a>/gi, (_whole, href: string, label: string) =>
      escapeHtml(linkText(href, label)),
    )
    // HTML's own rule: outside pre-wrap, any run of whitespace is one space.
    .replace(/\s+/g, ' ')
    .replace(/<br\s*\/?>/gi, KEPT_BREAK)
    .replace(/<li(\s[^>]*)?>/gi, `${LINE}- `)
    .replace(/<\/(td|th)>/gi, CELL)
    .replace(/<\/?(h[1-6]|p|ul|ol|table)(\s[^>]*)?>/gi, PARAGRAPH)
    .replace(/<\/?(div|tr|li|thead|tbody)(\s[^>]*)?>/gi, LINE)
    // Nested blocks stack their boundaries (a card's `</div>` right after its prose's): a run is
    // one break, a blank line when any block in it is a paragraph.
    .replace(new RegExp(`[ ${LINE}${PARAGRAPH}]*[${LINE}${PARAGRAPH}][ ${LINE}${PARAGRAPH}]*`, 'g'), (run: string) =>
      run.includes(PARAGRAPH) ? '\n\n' : '\n',
    )

  return decodeEntities(stripTags(text))
    .replaceAll(KEPT_BREAK, '\n')
    .split('\n')
    .map(layOutLine)
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}
