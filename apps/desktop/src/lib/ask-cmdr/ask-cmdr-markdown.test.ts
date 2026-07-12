/**
 * The XSS boundary for untrusted model text. These tests pin that injection-shaped model
 * output is neutralized (no executable HTML, no attacker-controlled links) while genuine
 * markdown-lite (bold, italic, inline code, lists) still renders.
 */

import { describe, it, expect } from 'vitest'
import { escapeForMarkdownLite, renderAssistantMarkdown } from './ask-cmdr-markdown'

describe('escapeForMarkdownLite', () => {
  it('neutralizes raw HTML tags', () => {
    const out = escapeForMarkdownLite('<img src=x onerror=alert(1)>')
    expect(out).not.toContain('<img')
    expect(out).toContain('&lt;img')
  })

  it('neutralizes link/image syntax so no <a>/<img> can form', () => {
    expect(escapeForMarkdownLite('[click](javascript:alert(1))')).not.toContain('[')
    expect(escapeForMarkdownLite('![x](data:text/html,evil)')).not.toContain('[')
  })

  it('leaves markdown formatting characters intact', () => {
    expect(escapeForMarkdownLite('**bold** _italic_ `code`')).toBe('**bold** _italic_ `code`')
  })

  it('encodes ampersands first', () => {
    expect(escapeForMarkdownLite('a & b')).toBe('a &amp; b')
  })
})

describe('renderAssistantMarkdown', () => {
  it('an injection-shaped model string produces no executable HTML', () => {
    const html = renderAssistantMarkdown('Hi <script>alert(1)</script> [x](javascript:alert(2))')
    // No raw tag survives, and the link never forms (so no attacker-controlled href),
    // even though the literal "javascript:" remains as inert visible text.
    expect(html).not.toContain('<script')
    expect(html).not.toContain('<a ')
    expect(html).not.toContain('href')
    expect(html).toContain('&lt;script')
  })

  it('renders genuine markdown-lite from the model', () => {
    expect(renderAssistantMarkdown('This is **important**.')).toContain('<strong>important</strong>')
    expect(renderAssistantMarkdown('Use `ls` to list.')).toContain('<code>ls</code>')
  })
})
