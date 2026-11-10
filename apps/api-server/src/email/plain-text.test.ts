import { describe, expect, it } from 'vitest'
import { htmlToText } from './plain-text'
import { CARD_PROSE_STYLE, documentShell, signoffParagraph } from './layout'

describe('htmlToText', () => {
  it('keeps the text of a whole email and drops the head and the template indentation', () => {
    const html = documentShell(
      `    <h2 style="color: #111827;">Cmdr digest: 1 error in the past 3h</h2>

    <h3 style="color: #111827;">Error reports</h3>
    ${signoffParagraph('The Cmdr API server sends this.')}`,
      'font-family: sans-serif;',
    )

    expect(htmlToText(html)).toBe(
      'Cmdr digest: 1 error in the past 3h\n\nError reports\n\nThe Cmdr API server sends this.',
    )
  })

  it('keeps the line breaks a person typed into a pre-wrap card, and collapses template whitespace elsewhere', () => {
    const html = `<div style="padding: 4px;">
        ERR-7YG5T &middot; auto-sent
    </div>
    <div style="${CARD_PROSE_STYLE}">Hi David,
Cmdr fails to move files.</div>`

    expect(htmlToText(html)).toBe('ERR-7YG5T · auto-sent\nHi David,\nCmdr fails to move files.')
  })

  it('writes a link as its label and its address, or just the address when the label is the address', () => {
    expect(htmlToText('<p>On the board: <a href="https://x.test/issues/1" style="c">#1</a></p>')).toBe(
      'On the board: #1 (https://x.test/issues/1)',
    )
    expect(htmlToText('<p><a href="https://x.test/a?b=1&amp;c=2">https://x.test/a?b=1&amp;c=2</a></p>')).toBe(
      'https://x.test/a?b=1&c=2',
    )
    expect(htmlToText('<p>Reply to <a href="mailto:a@b.test">a@b.test</a></p>')).toBe('Reply to a@b.test')
  })

  it('lays out lists as dashes and table rows as one line each', () => {
    const html = `<ul><li>one</li>
      <li>two</li></ul>
    <table>
        <tr>
            <th>Table</th>
            <th>Row count</th>
        </tr>
        <tr>
            <td>heartbeat</td>
            <td>12</td>
        </tr>
    </table>`

    expect(htmlToText(html)).toBe('- one\n- two\n\nTable | Row count\nheartbeat | 12')
  })

  it('decodes each entity once, so escaped text comes back as the person wrote it', () => {
    expect(htmlToText('<p>&lt;script&gt; &amp;lt; &quot;a&quot; it&#39;s &#8211; &#x2014;</p>')).toBe(
      '<script> &lt; "a" it\'s – —',
    )
  })
})
