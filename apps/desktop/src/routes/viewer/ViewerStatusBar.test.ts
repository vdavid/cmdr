import { describe, it, expect, afterEach, beforeEach, onTestFinished, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import { withPlatform } from '#lib/platform/test-platform.js'

import ViewerStatusBar from './ViewerStatusBar.svelte'
import type { MediaDimensions, ViewerContentKind } from '#lib/ipc/bindings.js'
import type { ViewerDisplayMode } from './viewer-view-mode'
import { _setLocaleForTests } from '#lib/intl/locale.js'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
}))

// The viewer's two remappable keys, as the live chips read them; `null` = turned off.
const viewerKeys = vi.hoisted((): { wrap: string | null; tail: string | null } => ({ wrap: 'W', tail: 'F' }))
function bindingsOf(id: string): string[] {
  const key = id === 'viewer.toggleWordWrap' ? viewerKeys.wrap : id === 'viewer.toggleTail' ? viewerKeys.tail : null
  return key === null ? [] : [key]
}
vi.mock('#lib/shortcuts/reactive-shortcuts.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/shortcuts/reactive-shortcuts.svelte.js')>()),
  commandHasShortcut: (id: string) => bindingsOf(id).length > 0,
  getLiveShortcutsReactive: bindingsOf,
  shortcutAsText: (id: string) => bindingsOf(id)[0],
}))

beforeEach(() => {
  document.body.innerHTML = ''
  // Pin the locale so the dimension thousands separator is deterministic.
  _setLocaleForTests('en-US')
})

afterEach(() => {
  _setLocaleForTests(null)
})

interface MountOpts {
  fileName?: string
  kind?: ViewerContentKind
  mode?: ViewerDisplayMode
  mediaDimensions?: MediaDimensions | null
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
      kind: opts.kind ?? 'text',
      mode: opts.mode ?? (opts.kind === 'image' || opts.kind === 'pdf' ? 'media' : 'text'),
      mediaDimensions: opts.mediaDimensions ?? null,
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
  it('shows the byte view mode and size without text line metadata', async () => {
    const { target, instance } = mountStatusBar({ mode: 'hex', totalLines: 42 })
    await tick()

    expect(target.querySelector('.backend-badge')?.textContent).toBe('Hex')
    expect(target.querySelector('.status-bar')?.textContent).not.toContain('42 lines')

    void unmount(instance)
  })
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

    expect(target.querySelector('.backend-badge')?.textContent).toBe('streaming, indexing…')

    void unmount(instance)
  })

  it('shows the plain streaming badge when byteSeek and not indexing', async () => {
    const { target, instance } = mountStatusBar({ currentMode: 'byteSeek', isIndexing: false })
    await tick()

    expect(target.querySelector('.backend-badge')?.textContent).toBe('streaming')

    void unmount(instance)
  })

  // The text-mode hint names the keys the user has, not the defaults (#441), each as a key cap.
  describe('text-mode key hint', () => {
    afterEach(() => {
      viewerKeys.wrap = 'W'
      viewerKeys.tail = 'F'
    })

    function hintParts(target: HTMLElement): { text: string; keys: string[] }[] {
      return Array.from(target.querySelectorAll('.hint-part'), (part) => ({
        text: part.textContent,
        keys: Array.from(part.querySelectorAll('kbd'), (k) => k.textContent),
      }))
    }

    it('lists wrap, tail, and search, each key in a chip', async () => {
      const { target, instance } = mountStatusBar()
      await tick()
      expect(hintParts(target)).toEqual([
        { text: 'W wrap', keys: ['W'] },
        { text: 'F tail', keys: ['F'] },
        { text: 'Ctrl+F search', keys: ['Ctrl+F'] },
      ])
      void unmount(instance)
    })

    it('shows a rebinding', async () => {
      viewerKeys.wrap = '⌃W'
      const { target, instance } = mountStatusBar()
      await tick()
      expect(hintParts(target)[0]).toEqual({ text: '⌃W wrap', keys: ['⌃W'] })
      void unmount(instance)
    })

    it('drops the part of a key turned off', async () => {
      viewerKeys.wrap = null
      const { target, instance } = mountStatusBar()
      await tick()
      expect(hintParts(target).map((p) => p.text)).toEqual(['F tail', 'Ctrl+F search'])
      void unmount(instance)
    })
  })

  it('adds a wrap badge when wordWrap is on', async () => {
    const { target, instance } = mountStatusBar({ wordWrap: true })
    await tick()

    const badges = Array.from(target.querySelectorAll('.backend-badge')).map((b) => b.textContent)
    expect(badges).toContain('wrap')

    void unmount(instance)
  })

  describe('media mode', () => {
    it('shows the kind badge, dimensions, and file size for an image, not line / backend info', async () => {
      const { target, instance } = mountStatusBar({
        kind: 'image',
        mediaDimensions: { width: 1920, height: 1080 },
        totalLines: null,
      })
      await tick()

      const text = target.querySelector('.status-bar')?.textContent ?? ''
      expect(text).toContain('Image')
      expect(text).toContain('1,920 × 1,080')
      // No backend badges (in memory / streaming / etc.) and no line count.
      expect(text).not.toContain('in memory')
      expect(text).not.toMatch(/\bline\b/)

      void unmount(instance)
    })

    it('omits dimensions when they are unknown (HEIC / SVG / PDF)', async () => {
      const { target, instance } = mountStatusBar({ kind: 'pdf', mediaDimensions: null, totalLines: null })
      await tick()

      const text = target.querySelector('.status-bar')?.textContent ?? ''
      expect(text).toContain('PDF')
      expect(text).not.toContain('×')

      void unmount(instance)
    })
  })
})

describe('ViewerStatusBar search hint', () => {
  it.each([
    ['macos', '⌘F search'],
    ['windows', 'Ctrl+F search'],
    ['linux', 'Ctrl+F search'],
  ] as const)('names %s’s own search key (`viewer.search`)', async (platform, hint) => {
    onTestFinished(withPlatform(platform))
    const { target, instance } = mountStatusBar()
    await tick()

    expect(target.querySelector('.shortcut-hint')?.textContent).toContain(hint)

    void unmount(instance)
  })
})
