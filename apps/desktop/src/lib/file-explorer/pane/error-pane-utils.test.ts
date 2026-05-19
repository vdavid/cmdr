import { describe, it, expect } from 'vitest'
import { renderErrorMarkdown } from './error-pane-utils'
import type { Markdown } from '$lib/ipc/bindings'

// The `Markdown` brand only exists at compile time. In production every
// `Markdown` value crosses the wire from a backend `md!(...)` site, where
// runtime args are escaped. Test fixtures forge the brand because we're
// exercising the renderer's snarkdown wrapper, not the round-trip.
const md = (s: string): Markdown => s as Markdown

describe('renderErrorMarkdown', () => {
  it('renders bold text as <strong>', () => {
    const result = renderErrorMarkdown(md('This is **bold** text'))
    expect(result).toContain('<strong>bold</strong>')
  })

  it('renders links as <a>', () => {
    const result = renderErrorMarkdown(md('Visit [example](https://example.com)'))
    expect(result).toContain('<a href="https://example.com">')
    expect(result).toContain('example</a>')
  })

  it('renders bullet lists as <li>', () => {
    const result = renderErrorMarkdown(md('- Item one\n- Item two'))
    expect(result).toContain('<li>')
    expect(result).toContain('Item one')
    expect(result).toContain('Item two')
  })

  it('renders inline code with backticks', () => {
    const result = renderErrorMarkdown(md('The error code is `ETIMEDOUT`'))
    expect(result).toContain('<code>ETIMEDOUT</code>')
  })

  it('handles plain text without markdown', () => {
    const result = renderErrorMarkdown(md('No markdown here'))
    expect(result).toContain('No markdown here')
  })
})
