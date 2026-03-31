/**
 * Astro integration that converts straight quotes and apostrophes to
 * typographic (curly) versions in rendered HTML. Skips content inside
 * <script>, <style>, <code>, <pre>, and <kbd> tags.
 *
 * Works alongside remark-smartypants (which handles markdown content).
 * This integration catches .astro template text that remark doesn't reach.
 */

import { readFile, readdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

/** Replace straight quotes/apostrophes with curly equivalents in plain text. */
function convertQuotes(text) {
  return (
    text
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

/** Tags whose text content should not be transformed. */
const skipTagPattern = /^<(script|style|code|pre|kbd)[\s>]/i

/**
 * Walk the HTML string, applying convertQuotes only to text nodes outside
 * of skip-tags and HTML attributes.
 */
function transformHtml(html) {
  let result = ''
  let i = 0

  while (i < html.length) {
    if (html[i] === '<') {
      const tagEnd = html.indexOf('>', i)
      if (tagEnd === -1) {
        result += html.substring(i)
        break
      }

      const tag = html.substring(i, tagEnd + 1)
      result += tag

      // If this opens a skip-tag, copy everything until the matching close tag
      const skipMatch = tag.match(skipTagPattern)
      if (skipMatch) {
        const closeTag = `</${skipMatch[1]}`
        const closeIdx = html.toLowerCase().indexOf(closeTag.toLowerCase(), tagEnd + 1)
        if (closeIdx !== -1) {
          const endOfClose = html.indexOf('>', closeIdx) + 1
          result += html.substring(tagEnd + 1, endOfClose)
          i = endOfClose
          continue
        }
      }

      i = tagEnd + 1
    } else {
      // Text content — find the next tag
      const nextTag = html.indexOf('<', i)
      const text = nextTag === -1 ? html.substring(i) : html.substring(i, nextTag)
      result += convertQuotes(text)
      i = nextTag === -1 ? html.length : nextTag
    }
  }

  return result
}

/** Collect HTML files recursively using readdir. */
async function findHtmlFiles(dir) {
  const files = []

  async function walk(currentDir) {
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
export function smartQuotesIntegration() {
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
