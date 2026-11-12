/**
 * The ready toast's hint names the New folder key the user has right now, and reads as a
 * whole sentence without one once they removed it.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const newFolderKey = vi.hoisted(() => ({ value: undefined as string | undefined }))
// The sentence asks whether a key runs New folder, and its chip reads the key itself.
vi.mock('#lib/shortcuts/reactive-shortcuts.svelte.js', () => {
  const keys = (commandId: string) =>
    commandId === 'file.newFolder' && newFolderKey.value !== undefined ? [newFolderKey.value] : []
  return {
    commandHasShortcut: (commandId: string) => keys(commandId).length > 0,
    getLiveShortcutsReactive: keys,
  }
})
vi.mock('#lib/tauri-commands/index.js', () => ({
  cancelAiDownload: vi.fn(() => Promise.resolve()),
  getAiModelInfo: vi.fn(() => Promise.resolve({ sizeBytes: 2_000_000_000 })),
  getAiStatus: vi.fn(() => Promise.resolve('available')),
  startAiDownload: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/settings/index.js', () => ({
  getSetting: vi.fn(() => 'local'),
  setSetting: vi.fn(),
}))

import AiToastContent from './AiToastContent.svelte'
import { getAiState, resetForTesting } from './ai-state.svelte'

async function readyDescription(): Promise<string | null | undefined> {
  getAiState().notificationState = 'ready'
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AiToastContent, { target, props: {} })
  await tick()
  return target.querySelector('.ai-description')?.textContent.trim()
}

describe('the AI ready toast', () => {
  beforeEach(() => {
    resetForTesting()
  })

  it('shows the live New folder key as a key cap', async () => {
    newFolderKey.value = 'Ctrl+Shift+N'
    expect(await readyDescription()).toBe(
      'Try creating a new folder (Ctrl+Shift+N) to see AI-powered name suggestions.',
    )
    expect(document.body.querySelector('.ai-description kbd, .ai-description .shortcut-chip')).not.toBeNull()
  })

  it('says the same without a key once no key runs New folder', async () => {
    newFolderKey.value = undefined
    expect(await readyDescription()).toBe('Try creating a new folder to see AI-powered name suggestions.')
  })
})
