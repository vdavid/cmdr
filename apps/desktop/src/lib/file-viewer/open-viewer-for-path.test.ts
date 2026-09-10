/**
 * A viewer opened from a bare path (MCP's `dialog open file-viewer`) must open
 * against the volume that holds the path. Pre-fix it always opened against
 * `root`, so a phone's or server's file answered `notFound`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const { openFileViewer, resolvePathVolume } = vi.hoisted(() => ({
  openFileViewer: vi.fn(() => Promise.resolve()),
  resolvePathVolume: vi.fn(),
}))
vi.mock('./open-viewer', () => ({ openFileViewer }))
vi.mock('$lib/tauri-commands', () => ({ resolvePathVolume }))

import { openFileViewerForPath } from './open-viewer-for-path'

describe('openFileViewerForPath', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('opens a phone file against the volume that holds it', async () => {
    resolvePathVolume.mockResolvedValue({ volume: { id: 'adb-phone', path: 'adb://R58M' }, timedOut: false })

    await openFileViewerForPath('adb://R58M/sdcard/notes.txt')

    expect(resolvePathVolume).toHaveBeenCalledWith('adb://R58M/sdcard/notes.txt')
    expect(openFileViewer).toHaveBeenCalledWith('adb://R58M/sdcard/notes.txt', 'adb-phone')
  })

  it('opens on the local drive when no volume claims the path', async () => {
    resolvePathVolume.mockResolvedValue({ volume: null, timedOut: true })

    await openFileViewerForPath('/Users/ada/notes.txt')

    expect(openFileViewer).toHaveBeenCalledWith('/Users/ada/notes.txt', 'root')
  })
})
