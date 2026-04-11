import { describe, it, expect } from 'vitest'
import { renderErrorMarkdown } from './error-pane-utils'

describe('renderErrorMarkdown', () => {
  it('renders bold text as <strong>', () => {
    const result = renderErrorMarkdown('This is **bold** text')
    expect(result).toContain('<strong>bold</strong>')
  })

  it('renders links as <a>', () => {
    const result = renderErrorMarkdown('Visit [example](https://example.com)')
    expect(result).toContain('<a href="https://example.com">')
    expect(result).toContain('example</a>')
  })

  it('renders bullet lists as <li>', () => {
    const result = renderErrorMarkdown('- Item one\n- Item two')
    expect(result).toContain('<li>')
    expect(result).toContain('Item one')
    expect(result).toContain('Item two')
  })

  it('renders inline code with backticks', () => {
    const result = renderErrorMarkdown('The error code is `ETIMEDOUT`')
    expect(result).toContain('<code>ETIMEDOUT</code>')
  })

  it('handles plain text without markdown', () => {
    const result = renderErrorMarkdown('No markdown here')
    expect(result).toContain('No markdown here')
  })
})
