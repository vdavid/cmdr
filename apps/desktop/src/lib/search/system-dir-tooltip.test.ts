/**
 * The exclude-list tooltip is HTML, so a directory name that looks like markup must
 * arrive as text. A folder literally named `<script>` is legal on every filesystem Cmdr
 * reads.
 */

import { describe, it, expect } from 'vitest'
import { buildSystemDirExcludeTooltip, escapeHtml } from './system-dir-tooltip'

describe('escapeHtml', () => {
  it('escapes the five characters that could become markup', () => {
    expect(escapeHtml(`<a href="x">&'`)).toBe('&lt;a href=&quot;x&quot;&gt;&amp;&#39;')
  })

  it('escapes the ampersand first, so an escape is never double-escaped', () => {
    expect(escapeHtml('&lt;')).toBe('&amp;lt;')
  })
})

describe('buildSystemDirExcludeTooltip', () => {
  it('lists every excluded name, with no truncation', () => {
    const dirs = Array.from({ length: 40 }, (_, i) => `dir-${String(i)}`)
    const html = buildSystemDirExcludeTooltip(dirs, 'Hidden folders')
    expect(html).toContain('Hidden folders')
    expect(html).toContain('dir-0')
    expect(html).toContain('dir-39')
    expect(html.match(/<div style="font-family/g)?.length).toBe(40)
  })

  it('renders a markup-shaped folder name as text', () => {
    const html = buildSystemDirExcludeTooltip(['<script>'], 'Heading')
    expect(html).toContain('&lt;script&gt;')
    expect(html).not.toContain('<script>')
  })

  it('still renders its heading when the list is empty', () => {
    expect(buildSystemDirExcludeTooltip([], 'Heading')).toContain('Heading')
  })
})
