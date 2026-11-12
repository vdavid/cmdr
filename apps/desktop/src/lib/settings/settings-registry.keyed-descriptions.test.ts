/**
 * A setting description that names a command's key shows that command's live key, and says the
 * same without a key once none runs it (`descriptionWithKey`). Never an empty slot or a Mac glyph
 * off macOS: the key comes from the store's live reader, already in this OS's display form.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { withEveryCapability } from '#lib/platform/test-platform.js'

const liveKey = new Map<string, string>()

vi.mock('#lib/shortcuts/reactive-shortcuts.svelte.js', () => ({
  getFirstShortcutReactive: (commandId: string) => liveKey.get(commandId),
}))

import { getSettingDefinition } from './settings-registry'

withEveryCapability()

beforeEach(() => {
  liveKey.clear()
})

const description = (id: string): string => getSettingDefinition(id)?.description ?? ''

describe('descriptions that name a key', () => {
  it('fill in the paste key, and drop it when no key pastes', () => {
    liveKey.set('edit.paste', 'Ctrl+V')
    expect(description('fileOperations.pasteClipboardAsFile')).toBe(
      'What Ctrl+V does in a folder when the clipboard holds text, an image, or a PDF instead of copied files.',
    )
    liveKey.clear()
    expect(description('fileOperations.pasteClipboardAsFile')).toBe(
      'What pasting does in a folder when the clipboard holds text, an image, or a PDF instead of copied files.',
    )
  })

  it('fill in Reopen closed tab’s key, and drop the parenthesis when it has none', () => {
    liveKey.set('tab.reopen', '⌘⇧T')
    expect(description('fileExplorer.tabs.closedTabHistorySize')).toContain('“Reopen closed tab” (⌘⇧T). Higher')
    liveKey.clear()
    expect(description('fileExplorer.tabs.closedTabHistorySize')).toContain('“Reopen closed tab”. Higher')
  })

  it('fill in the Quick Look key, and say it without one', () => {
    liveKey.set('file.quickLook', '⇧Space')
    expect(description('fileExplorer.suppressQuickLookHint')).toContain('and ⇧Space for Quick Look')
    liveKey.clear()
    expect(description('fileExplorer.suppressQuickLookHint')).toContain(
      'Cmdr uses Space to toggle file selection, where Finder uses it for Quick Look.',
    )
  })

  it('leave the global hotkey’s registry text keyless: its row shows the live binding itself', () => {
    expect(description('behavior.fileSystemWatching.globalGoToLatestShortcut.enabled')).toBe(
      'Jump to your most recent download from any app with a system-wide shortcut. Requires Full Disk Access.',
    )
  })
})
