/**
 * The wow-moment measurement: it fires once, on the first window of rows that
 * actually shows a folder size, and never again for the rest of the launch.
 *
 * The "once" half is the part worth pinning: rows are re-fetched on every scroll
 * and re-enriched on every `index-dir-updated`, so a hook that fired per pass
 * would report hundreds of times a session and make the number meaningless.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FileEntry } from '$lib/file-explorer/types'

const trackEvent = vi.fn()
vi.mock('$lib/tauri-commands', () => ({
  trackEvent: (name: string, props: Record<string, unknown>) => trackEvent(name, props),
}))
vi.mock('./index-state.svelte', () => ({
  isVolumeCoveredInPhases: () => true,
}))

import { noteRenderedFolderSizes, resetFirstSizeTimingForTest, secondsBucket } from './first-size-timing'

function dir(recursiveSize: number | null): FileEntry {
  return { name: 'folder', path: '/x/folder', isDirectory: true, size: 0, recursiveSize } as unknown as FileEntry
}

function file(): FileEntry {
  return { name: 'note.txt', path: '/x/note.txt', isDirectory: false, size: 12 } as unknown as FileEntry
}

describe('noteRenderedFolderSizes', () => {
  beforeEach(() => {
    trackEvent.mockClear()
    resetFirstSizeTimingForTest()
  })

  it('reports the first window that shows a real folder size', () => {
    noteRenderedFolderSizes([dir(4096), file()], 'root')

    expect(trackEvent).toHaveBeenCalledTimes(1)
    expect(trackEvent.mock.calls[0][0]).toBe('first_folder_size_shown')
    expect(trackEvent.mock.calls[0][1]).toMatchObject({ covering: true })
    expect(String(trackEvent.mock.calls[0][1].seconds_bucket)).not.toBe('')
  })

  it('says nothing while every folder is still a placeholder', () => {
    noteRenderedFolderSizes([dir(null), file()], 'root')

    expect(trackEvent).not.toHaveBeenCalled()
  })

  it('reports once per launch, however many windows follow', () => {
    noteRenderedFolderSizes([dir(4096)], 'root')
    noteRenderedFolderSizes([dir(8192)], 'root')
    noteRenderedFolderSizes([dir(1024)], 'root')

    expect(trackEvent).toHaveBeenCalledTimes(1)
  })

  it('ignores a file with a size: the claim is about FOLDER sizes', () => {
    noteRenderedFolderSizes([file()], 'root')

    expect(trackEvent).not.toHaveBeenCalled()
  })
})

describe('secondsBucket', () => {
  it('is fine at the short end, where the claim lives', () => {
    expect(secondsBucket(0)).toBe('<5s')
    expect(secondsBucket(4_999)).toBe('<5s')
    expect(secondsBucket(5_000)).toBe('5-15s')
    expect(secondsBucket(59_999)).toBe('15-60s')
    expect(secondsBucket(60_000)).toBe('1-5m')
    expect(secondsBucket(300_000)).toBe('5m+')
  })
})
