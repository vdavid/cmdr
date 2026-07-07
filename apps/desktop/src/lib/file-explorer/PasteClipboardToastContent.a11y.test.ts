/**
 * Tier 3 a11y tests for `PasteClipboardToastContent.svelte`.
 *
 * Compact toast body shown after pasting clipboard content as a file: a message
 * line plus a "Settings" button. Modeled on `CrashReportToastContent.a11y.test.ts`.
 */

import { describe, it, vi, beforeAll, afterAll } from 'vitest'
import { mount, tick } from 'svelte'
import { _setLocaleForTests } from '$lib/intl/locale'
import PasteClipboardToastContent from './PasteClipboardToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({ dismissToast: vi.fn(), addToast: vi.fn() }))
vi.mock('$lib/settings/settings-window', () => ({ openSettingsWindow: vi.fn(() => Promise.resolve()) }))
vi.mock('$lib/settings', () => ({ getSetting: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({
  pasteClipboardAsFile: vi.fn(),
  findFileIndex: vi.fn(),
  onDirectoryDiff: vi.fn(),
}))
vi.mock('$lib/file-operations/mkdir/new-folder-operations', () => ({ moveCursorToNewFolder: vi.fn() }))

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('PasteClipboardToastContent a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PasteClipboardToastContent, { target, props: { filename: 'pasted.txt', kind: 'text', toastId: 't-a11y' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
