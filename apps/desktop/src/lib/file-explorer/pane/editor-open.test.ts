/**
 * The F4 guard: a row with no real file behind it is refused on screen before any
 * launch, and a real row reports whether an app was asked to open it.
 *
 * Which app, and everything said around the launch, is `$lib/text-editor`'s job and
 * is pinned in `open-file-in-editor.test.ts`; here it's a mock.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const m = vi.hoisted(() => ({
  openFileInEditor: vi.fn<(path: string) => Promise<boolean>>(),
  addToast: vi.fn(),
}))

vi.mock('$lib/text-editor/open-file-in-editor', () => ({ openFileInEditor: m.openFileInEditor }))
vi.mock('$lib/ui/toast', () => ({ addToast: m.addToast }))
// An empty volume list: the guard classifies the ids below off their shape alone.
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

import { openInEditorOrExplain } from './editor-open'

const { openFileInEditor, addToast } = m

beforeEach(() => {
  vi.clearAllMocks()
  openFileInEditor.mockResolvedValue(true)
})

describe('openInEditorOrExplain: the guard', () => {
  it('refuses a phone’s file with a toast and launches nothing', async () => {
    await expect(openInEditorOrExplain('adb-r58m', 'adb://R58M/sdcard/notes.txt')).resolves.toBe('refusedNotOnThisMac')

    expect(openFileInEditor).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledOnce()
  })

  it('refuses a file inside an archive, whose path looks like a plain one', async () => {
    await expect(openInEditorOrExplain('root', '/Users/dave/photos.zip/notes.txt')).resolves.toBe('refusedNotOnThisMac')

    expect(openFileInEditor).not.toHaveBeenCalled()
  })
})

describe('openInEditorOrExplain: a real file', () => {
  it('reports opened when an app was asked to open it', async () => {
    await expect(openInEditorOrExplain('root', '/Users/dave/notes.txt')).resolves.toBe('opened')

    expect(openFileInEditor).toHaveBeenCalledExactlyOnceWith('/Users/dave/notes.txt')
    expect(addToast).not.toHaveBeenCalled()
  })

  it('reports launchFailed when the launch never started', async () => {
    openFileInEditor.mockResolvedValue(false)

    await expect(openInEditorOrExplain('root', '/Users/dave/notes.txt')).resolves.toBe('launchFailed')
  })
})
