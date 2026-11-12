/**
 * The F4 guard: a row with no real file behind it is refused on screen before any
 * launch, and a real row reports whether an app was asked to open it.
 *
 * Which app, and everything said around the launch, is `#lib/text-editor`'s job and
 * is pinned in `open-file-in-editor.test.ts`; here it's a mock.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const m = vi.hoisted(() => ({
  openFileInEditor: vi.fn<(path: string) => Promise<boolean>>(),
  addToast: vi.fn(),
  liveShortcuts: vi.fn<(commandId: string) => string[]>(),
}))

vi.mock('#lib/text-editor/open-file-in-editor.js', () => ({ openFileInEditor: m.openFileInEditor }))
vi.mock(import('#lib/shortcuts/shortcuts-store.js'), async (importOriginal) => ({
  ...(await importOriginal()),
  getLiveShortcuts: m.liveShortcuts,
}))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: m.addToast }))
// An empty volume list: the guard classifies the ids below off their shape alone.
vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: () => [] }))

import { openInEditorOrExplain } from './editor-open'

const { openFileInEditor, addToast } = m

beforeEach(() => {
  vi.clearAllMocks()
  openFileInEditor.mockResolvedValue(true)
  m.liveShortcuts.mockImplementation((commandId) => (commandId === 'file.view' ? ['F3'] : []))
})

describe('openInEditorOrExplain: the guard', () => {
  it('refuses a phone’s file with a toast naming View’s live key, and launches nothing', async () => {
    m.liveShortcuts.mockImplementation((commandId) => (commandId === 'file.view' ? ['Ctrl+Shift+V'] : []))

    await expect(openInEditorOrExplain('adb-r58m', 'adb://R58M/sdcard/notes.txt')).resolves.toBe('refusedNotOnThisMac')

    expect(openFileInEditor).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledExactlyOnceWith(
      'Cmdr can’t open this file in an editor yet, because it isn’t a regular file on your computer. Press Ctrl+Shift+V to view it.',
      { level: 'info' },
    )
  })

  it('names the View command once no key runs it', async () => {
    m.liveShortcuts.mockReturnValue([])

    await openInEditorOrExplain('adb-r58m', 'adb://R58M/sdcard/notes.txt')

    expect(addToast).toHaveBeenCalledExactlyOnceWith(
      'Cmdr can’t open this file in an editor yet, because it isn’t a regular file on your computer. View it with the View command instead.',
      { level: 'info' },
    )
  })

  it('refuses a file inside an archive, whose path looks like a plain one', async () => {
    await expect(openInEditorOrExplain('root', '/Users/test_user_10/photos.zip/notes.txt')).resolves.toBe(
      'refusedNotOnThisMac',
    )

    expect(openFileInEditor).not.toHaveBeenCalled()
  })
})

describe('openInEditorOrExplain: a real file', () => {
  it('reports opened when an app was asked to open it', async () => {
    await expect(openInEditorOrExplain('root', '/Users/test_user_10/notes.txt')).resolves.toBe('opened')

    expect(openFileInEditor).toHaveBeenCalledExactlyOnceWith('/Users/test_user_10/notes.txt')
    expect(addToast).not.toHaveBeenCalled()
  })

  it('reports launchFailed when the launch never started', async () => {
    openFileInEditor.mockResolvedValue(false)

    await expect(openInEditorOrExplain('root', '/Users/test_user_10/notes.txt')).resolves.toBe('launchFailed')
  })
})
