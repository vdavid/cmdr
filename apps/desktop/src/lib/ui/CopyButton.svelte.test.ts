/**
 * What `CopyButton` puts on the clipboard, and the moment of feedback after: the WHOLE
 * text (the line beside it may show only part of it), then a check that goes back to
 * the copy glyph.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import CopyButton from './CopyButton.svelte'
import { tString } from '#lib/intl/messages.svelte.js'

const copyToClipboard = vi.fn<(text: string) => Promise<void>>()

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  copyToClipboard: (text: string) => copyToClipboard(text),
}))

describe('CopyButton', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    copyToClipboard.mockReset()
    copyToClipboard.mockResolvedValue(undefined)
  })

  afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
  })

  function press(text: string): HTMLButtonElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CopyButton, { target, props: { text, label: 'Copy the full list of folders' } })
    flushSync()
    const button = target.querySelector('button')
    if (!button) throw new Error('no button')
    button.click()
    return button
  }

  it('copies the whole text, every line of it', async () => {
    press('/a\n/b\n/c')
    await tick()
    expect(copyToClipboard).toHaveBeenCalledWith('/a\n/b\n/c')
  })

  it('says it copied, then goes back to saying what it copies', async () => {
    const button = press('/a')
    await vi.waitFor(() => {
      expect(button.getAttribute('aria-label')).toBe(tString('ui.copyButton.copied'))
    })
    vi.advanceTimersByTime(2000)
    flushSync()
    expect(button.getAttribute('aria-label')).toBe('Copy the full list of folders')
  })

  it('falls back to the web clipboard when the backend is not there', async () => {
    copyToClipboard.mockRejectedValue(new Error('no backend'))
    const writeText = vi.fn<(text: string) => Promise<void>>().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    press('/a')
    await vi.waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('/a')
    })
  })
})
