import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewerStatusBar from './ViewerStatusBar.svelte'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
}))

beforeEach(() => {
  document.body.innerHTML = ''
})

interface MountOpts {
  fileName?: string
  totalLines?: number | null
  totalBytes?: number
  currentMode?: 'fullLoad' | 'byteSeek' | 'lineIndex'
  isIndexing?: boolean
  wordWrap?: boolean
}

function mountStatusBar(opts: MountOpts = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(ViewerStatusBar, {
    target,
    props: {
      fileName: opts.fileName ?? 'example.txt',
      totalLines: opts.totalLines === undefined ? 42 : opts.totalLines,
      totalBytes: opts.totalBytes ?? 1024,
      currentMode: opts.currentMode ?? 'fullLoad',
      isIndexing: opts.isIndexing ?? false,
      wordWrap: opts.wordWrap ?? false,
      indexingTimeoutSecs: 5,
    },
  })
  return { target, instance }
}

describe('ViewerStatusBar', () => {
  it('renders the file name, line count, and the in-memory badge', async () => {
    const { target, instance } = mountStatusBar({ fileName: 'log.txt', totalLines: 3, currentMode: 'fullLoad' })
    await tick()

    const bar = target.querySelector('.status-bar')
    expect(bar).not.toBeNull()
    expect(bar?.textContent).toContain('log.txt')
    expect(bar?.textContent).toContain('3 lines')
    expect(target.querySelector('.backend-badge')?.textContent).toBe('in memory')

    void unmount(instance)
  })

  it('uses the singular "line" for a one-line file', async () => {
    const { target, instance } = mountStatusBar({ totalLines: 1 })
    await tick()

    expect(target.querySelector('.status-bar')?.textContent).toContain('1 line')
    expect(target.querySelector('.status-bar')?.textContent).not.toContain('1 lines')

    void unmount(instance)
  })

  it('omits the line count when totalLines is null', async () => {
    const { target, instance } = mountStatusBar({ totalLines: null })
    await tick()

    expect(target.querySelector('.status-bar')?.textContent).not.toMatch(/\bline\b/)

    void unmount(instance)
  })

  it('shows the indexed badge in lineIndex mode', async () => {
    const { target, instance } = mountStatusBar({ currentMode: 'lineIndex' })
    await tick()

    expect(target.querySelector('.backend-badge')?.textContent).toBe('indexed')

    void unmount(instance)
  })

  it('shows the streaming-indexing badge when byteSeek and isIndexing', async () => {
    const { target, instance } = mountStatusBar({ currentMode: 'byteSeek', isIndexing: true })
    await tick()

    expect(target.querySelector('.backend-badge')?.textContent).toBe('streaming, indexing...')

    void unmount(instance)
  })

  it('shows the plain streaming badge when byteSeek and not indexing', async () => {
    const { target, instance } = mountStatusBar({ currentMode: 'byteSeek', isIndexing: false })
    await tick()

    expect(target.querySelector('.backend-badge')?.textContent).toBe('streaming')

    void unmount(instance)
  })

  it('adds a wrap badge when wordWrap is on', async () => {
    const { target, instance } = mountStatusBar({ wordWrap: true })
    await tick()

    const badges = Array.from(target.querySelectorAll('.backend-badge')).map((b) => b.textContent)
    expect(badges).toContain('wrap')

    void unmount(instance)
  })
})
