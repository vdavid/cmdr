/**
 * Escaper unit tests, ported verbatim from the Rust `markdown.rs` tests. The
 * escaper is the XSS-load-bearing security boundary: it must encode markdown
 * specials in runtime params so `snarkdown` doesn't render them as formatting.
 */

import { describe, expect, it } from 'vitest'
import { escapeMarkdown } from './markdown-escape'

describe('escapeMarkdown', () => {
  it('passes through plain text unchanged', () => {
    expect(escapeMarkdown('plain text')).toBe('plain text')
  })

  it('encodes markdown specials as HTML entities', () => {
    // snarkdown doesn't parse `\_` as an escape so we use entities; the browser
    // decodes them at render time.
    expect(escapeMarkdown('STATUS_DELETE_PENDING')).toBe('STATUS&#95;DELETE&#95;PENDING')
    expect(escapeMarkdown('**bold**')).toBe('&#42;&#42;bold&#42;&#42;')
    expect(escapeMarkdown('[link](url)')).toBe('&#91;link&#93;&#40;url&#41;')
    expect(escapeMarkdown('a `code` span')).toBe('a &#96;code&#96; span')
  })

  it('neutralizes preexisting entities by encoding every &', () => {
    expect(escapeMarkdown('a & b')).toBe('a &amp; b')
    expect(escapeMarkdown('&lt;script&gt;')).toBe('&amp;lt;script&amp;gt;')
  })

  it('leaves line-start chars alone', () => {
    // `.`, `-`, `+`, `#`, `|` only have markdown meaning at line start, and
    // runtime values land mid-sentence.
    expect(escapeMarkdown('Sync.com')).toBe('Sync.com')
    expect(escapeMarkdown('a-dashed-path')).toBe('a-dashed-path')
    expect(escapeMarkdown('photo.jpg')).toBe('photo.jpg')
  })

  it('encodes underscores in a path but leaves dots and slashes', () => {
    expect(escapeMarkdown('/Volumes/naspi/_todo_pics/file.jpg')).toBe('/Volumes/naspi/&#95;todo&#95;pics/file.jpg')
  })

  it('encodes < and > and tilde', () => {
    expect(escapeMarkdown('<a>')).toBe('&lt;a&gt;')
    expect(escapeMarkdown('~strike~')).toBe('&#126;strike&#126;')
  })
})
