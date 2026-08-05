/**
 * Tests for the copied-path toast body: the confirmation sentence plus the path
 * that just landed on the clipboard, rendered verbatim and set to wrap rather
 * than overflow the toast.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { mount, tick } from 'svelte'
import { _setLocaleForTests } from '$lib/intl/locale'
import CopiedPathToastContent from './CopiedPathToastContent.svelte'

// The message resolves through the REAL `$lib/intl` (golden output).

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

async function mountToast(path: string) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(CopiedPathToastContent, { target, props: { path } })
  await tick()
  return target
}

describe('CopiedPathToastContent', () => {
  it('renders the confirmation sentence and the path', async () => {
    const target = await mountToast('/Users/test/Downloads')
    expect(target.textContent).toContain("Copied the path, it's now on your clipboard:")
    expect(target.querySelector('.path')?.textContent).toBe('/Users/test/Downloads')
    target.remove()
  })

  it('renders a path with spaces and non-ASCII characters verbatim', async () => {
    const path = '/Volumes/naspi/papers/Rymdskottkärra AB/2026 — kvitton/räkning #7.pdf'
    const target = await mountToast(path)
    expect(target.querySelector('.path')?.textContent).toBe(path)
    target.remove()
  })

  it('renders a very long path in full, leaving the wrap to CSS', async () => {
    // The toast shows the whole path and lets `overflow-wrap: anywhere` break it
    // across lines. Nothing here may truncate or ellipsize it in markup: the user
    // reads this to confirm WHAT landed on the clipboard.
    const path = `/Users/test/${'a'.repeat(300)}/file.txt`
    const target = await mountToast(path)
    expect(target.querySelector('.path')?.textContent).toBe(path)
    target.remove()
  })
})
