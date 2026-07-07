/**
 * Tests for the paste-as-file info toast body: it renders the "Pasted clipboard
 * <kind> as <filename>" message and a Settings button that self-dismisses and
 * deep-links to Behavior > Navigation & file ops.
 */

import { describe, it, expect, vi, beforeEach, beforeAll, afterAll } from 'vitest'
import { mount, tick } from 'svelte'
import { _setLocaleForTests } from '$lib/intl/locale'
import PasteClipboardToastContent from './PasteClipboardToastContent.svelte'

const { dismissToast, openSettingsWindow } = vi.hoisted(() => ({
  dismissToast: vi.fn((..._args: unknown[]) => undefined),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))

// `$lib/ui/toast` also exports `addToast` (imported transitively by
// paste-clipboard-as-file.ts); provide it so the module graph resolves.
vi.mock('$lib/ui/toast', () => ({ dismissToast, addToast: vi.fn() }))
vi.mock('$lib/settings/settings-window', () => ({ openSettingsWindow }))
vi.mock('$lib/settings', () => ({ getSetting: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({
  pasteClipboardAsFile: vi.fn(),
  findFileIndex: vi.fn(),
  onDirectoryDiff: vi.fn(),
}))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: vi.fn() }))
// The message itself resolves through the REAL `$lib/intl` (golden output).

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})
beforeEach(() => {
  dismissToast.mockClear()
  openSettingsWindow.mockClear()
})

async function mountToast(props: { filename: string; kind: 'text' | 'image' | 'pdf'; toastId: string }) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(PasteClipboardToastContent, { target, props })
  await tick()
  return target
}

describe('PasteClipboardToastContent', () => {
  it('renders the kind-specific message with the filename', async () => {
    const target = await mountToast({ filename: 'pasted.png', kind: 'image', toastId: 't-1' })
    expect(target.textContent).toContain('Pasted clipboard image as pasted.png')
    target.remove()
  })

  it('renders a Settings action button', async () => {
    const target = await mountToast({ filename: 'pasted.txt', kind: 'text', toastId: 't-2' })
    const btn = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Settings')
    expect(btn).toBeDefined()
    target.remove()
  })

  it('Settings dismisses the toast and deep-links to Behavior > Navigation & file ops', async () => {
    const target = await mountToast({ filename: 'pasted.pdf', kind: 'pdf', toastId: 't-3' })
    const btn = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Settings')
    btn?.click()
    await tick()
    expect(dismissToast).toHaveBeenCalledWith('t-3')
    expect(openSettingsWindow).toHaveBeenCalledWith(['Behavior', 'Navigation & file ops'])
    target.remove()
  })
})
