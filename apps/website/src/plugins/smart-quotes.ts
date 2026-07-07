/**
 * Astro integration that converts straight quotes and apostrophes to
 * typographic (curly) versions in rendered HTML. Skips content inside
 * <script>, <style>, <code>, <pre>, and <kbd> tags.
 *
 * Works alongside remark-smartypants (which handles markdown content).
 * This integration catches .astro template text that remark doesn't reach,
 * whether the quotes arrive literal (set:html) or HTML-entity-encoded
 * (Astro's `{text}` interpolation emits `&quot;` / `&#39;`).
 *
 * It runs at `astro:build:done` over the emitted HTML files (not the hast
 * tree) on purpose: that's the one layer that sees BOTH markdown output and
 * .astro template text plus `{interpolation}`. It only ever rewrites text
 * nodes: tag names, attributes, and the interiors of skip-tags are copied
 * through verbatim. The tag scanner is deliberately hand-rolled (rather than
 * a full parse/reserialize) so it can't introduce non-visual HTML drift.
 */

import type { AstroIntegration } from 'astro'
import { readFile, readdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

/** Tags whose text content must never be transformed. */
const SKIP_TAGS = new Set(['script', 'style', 'code', 'pre', 'kbd'])

/**
 * Raw-text elements: their content is opaque CDATA-style text that ends at the first `</name>` (HTML
 * spec), so a `<style`/`</style>` substring inside (e.g. an SVG data URI in CSS, or `<` in a script
 * string) is literal text, NOT structure. These must use first-close matching, never depth-counting.
 */
const RAW_TEXT_TAGS = new Set(['script', 'style'])

/** Replace straight quotes/apostrophes with curly equivalents in plain text. */
function convertQuotes(text: string): string {
  return (
    text
      // Astro's `{text}` interpolation HTML-escapes quotes (`&quot;`) and apostrophes (`&#39;`),
      // whereas `set:html` and markdown leave them literal. Decode these quote entities first so
      // both render paths get curled by the rules below. Browsers render the entity and the literal
      // identically, so this is display-safe, and we never touch `&amp;`.
      .replace(/&quot;|&#0*34;|&#x0*22;/gi, '"')
      .replace(/&apos;|&#0*39;|&#x0*27;/gi, "'")
      // Apostrophes inside words: don't, it's, we're
      .replace(/(\w)'(\w)/g, '$1\u2019$2')
      // Opening double quotes (after whitespace or start)
      .replace(/(^|[\s([{])"(\S)/gm, '$1\u201C$2')
      // Closing double quotes (before whitespace, punctuation, or end)
      .replace(/(\S)"([\s.,;:!?)\]}]|$)/gm, '$1\u201D$2')
      // Opening single quotes (after whitespace or start)
      .replace(/(^|[\s([{])'(\S)/gm, '$1\u2018$2')
      // Closing single quotes (before whitespace, punctuation, or end)
      .replace(/(\S)'([\s.,;:!?)\]}]|$)/gm, '$1\u2019$2')
  )
}

/**
 * If `rawTag` (a full `<…>` token) opens a skip-tag that encloses content, return its lowercased
 * name; otherwise null. Closing tags (`</code>`) and self-closing tags (`<code/>`, `<code />`) have
 * no interior to skip, so they return null and let normal scanning continue.
 */
function openingSkipTagName(rawTag: string): string | null {
  const match = /^<([a-zA-Z][a-zA-Z0-9]*)/.exec(rawTag)
  if (!match) return null
  const name = match[1].toLowerCase()
  if (!SKIP_TAGS.has(name)) return null
  if (/\/\s*>$/.test(rawTag)) return null
  return name
}

/** Index just past the first `</name>` at or after `from`, or `html.length` if none. */
function findFirstClose(html: string, from: number, name: string): number {
  const lower = html.toLowerCase()
  let i = from
  while (i < html.length) {
    const at = lower.indexOf(`</${name}`, i)
    if (at === -1) return html.length
    // The name must be delimited (`</style>`, `</style >`, `</style/`), not a longer name
    // (`</styles>`), to match the HTML tokenizer.
    const after = html[at + 2 + name.length]
    if (after === undefined || after === '>' || after === '/' || /\s/.test(after)) {
      const gt = html.indexOf('>', at)
      return gt === -1 ? html.length : gt + 1
    }
    i = at + 2 + name.length
  }
  return html.length
}

/**
 * From `from`, find the index just past the matching `</name>` close tag. Raw-text elements
 * (`<script>`, `<style>`) close at their first `</name>` per the HTML spec; other skip-tags
 * (`<code>`, `<pre>`, `<kbd>`) honor nesting of the same tag name. Matching is case-insensitive.
 * Returns `html.length` when the tag is never closed, so an unterminated skip-tag swallows the rest
 * of the file rather than leaking its interior to the quote-curling path.
 */
function findSkipTagEnd(html: string, from: number, name: string): number {
  if (RAW_TEXT_TAGS.has(name)) return findFirstClose(html, from, name)

  let depth = 1
  let i = from
  while (i < html.length) {
    const lt = html.indexOf('<', i)
    if (lt === -1) return html.length
    const gt = html.indexOf('>', lt)
    if (gt === -1) return html.length
    const rawTag = html.slice(lt, gt + 1)
    const match = /^<(\/?)\s*([a-zA-Z][a-zA-Z0-9]*)/.exec(rawTag)
    if (match && match[2].toLowerCase() === name) {
      if (match[1] === '/') {
        depth--
        if (depth === 0) return gt + 1
      } else if (!/\/\s*>$/.test(rawTag)) {
        depth++
      }
    }
    i = gt + 1
  }
  return html.length
}

/**
 * Walk the HTML string, applying convertQuotes only to text nodes outside of skip-tags and HTML
 * tags. Everything between `<` and `>` (tag names, attributes) and every skip-tag interior is copied
 * through untouched.
 */
function transformHtml(html: string): string {
  let result = ''
  let i = 0

  while (i < html.length) {
    const lt = html.indexOf('<', i)
    if (lt === -1) {
      // Trailing text run.
      result += convertQuotes(html.slice(i))
      break
    }

    // Text before this tag.
    if (lt > i) result += convertQuotes(html.slice(i, lt))

    const gt = html.indexOf('>', lt)
    if (gt === -1) {
      // Unclosed tag at EOF: copy verbatim, never curl tag internals.
      result += html.slice(lt)
      break
    }

    const rawTag = html.slice(lt, gt + 1)
    result += rawTag
    i = gt + 1

    const skipName = openingSkipTagName(rawTag)
    if (skipName) {
      const end = findSkipTagEnd(html, i, skipName)
      result += html.slice(i, end)
      i = end
    }
  }

  return result
}

/** Collect HTML files recursively using readdir. */
async function findHtmlFiles(dir: string): Promise<string[]> {
  const files: string[] = []

  async function walk(currentDir: string): Promise<void> {
    const entries = await readdir(currentDir, { withFileTypes: true })
    for (const entry of entries) {
      const fullPath = join(currentDir, entry.name)
      if (entry.isDirectory()) {
        await walk(fullPath)
      } else if (entry.name.endsWith('.html')) {
        files.push(fullPath)
      }
    }
  }

  await walk(dir)
  return files
}

/** Astro integration that post-processes HTML files after build. */
export function smartQuotesIntegration(): AstroIntegration {
  return {
    name: 'smart-quotes',
    hooks: {
      'astro:build:done': async ({ dir }) => {
        const distPath = dir.pathname
        const htmlFiles = await findHtmlFiles(distPath)

        await Promise.all(
          htmlFiles.map(async (file) => {
            const html = await readFile(file, 'utf-8')
            const transformed = transformHtml(html)
            if (transformed !== html) {
              await writeFile(file, transformed)
            }
          }),
        )
      },
    },
  }
}
