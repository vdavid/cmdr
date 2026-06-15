import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewerToolbar from './ViewerToolbar.svelte'
import type { EncodingChoice, FileEncoding, ViewerContentKind } from '$lib/ipc/bindings'

const choices: EncodingChoice[] = [
  { encoding: 'utf8', label: 'UTF-8', group: 'unicode' },
  { encoding: 'utf16Le', label: 'UTF-16 LE', group: 'unicode' },
  { encoding: 'windows1252', label: 'Western (Windows-1252)', group: 'western' },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

/** The toolbar renders two `ui/Select`s; find a trigger by its aria-label. */
function triggerByLabel(target: HTMLElement, label: string): HTMLButtonElement | null {
  return (
    Array.from(target.querySelectorAll<HTMLButtonElement>('.select-trigger')).find(
      (t) => t.getAttribute('aria-label') === label,
    ) ?? null
  )
}

/** The Ark `Select` for `label` renders every option in the DOM even closed. */
function optionByValue(target: HTMLElement, label: string, value: string): HTMLElement | undefined {
  const trigger = triggerByLabel(target, label)
  const root = trigger?.closest('[data-part="root"]')
  return Array.from(root?.querySelectorAll<HTMLElement>(`[data-part="item"][data-value="${value}"]`) ?? [])[0]
}

interface MountOpts {
  fileName?: string
  kind?: ViewerContentKind
  lastMediaKind?: ViewerContentKind | null
  currentEncoding?: FileEncoding
  detectedEncoding?: FileEncoding
  isIndexing?: boolean
  tailMode?: boolean
  onViewAsText?: () => void
  onViewAsMedia?: () => void
  onEncodingChange?: (encoding: FileEncoding) => void
  onToggleTail?: () => void
}

function mountToolbar(opts: MountOpts = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(ViewerToolbar, {
    target,
    props: {
      fileName: opts.fileName ?? 'example.txt',
      kind: opts.kind ?? 'text',
      lastMediaKind: opts.lastMediaKind ?? null,
      currentEncoding: opts.currentEncoding ?? 'utf8',
      detectedEncoding: opts.detectedEncoding ?? 'utf8',
      encodingChoices: choices,
      isIndexing: opts.isIndexing ?? false,
      tailMode: opts.tailMode ?? false,
      onViewAsText: opts.onViewAsText ?? (() => {}),
      onViewAsMedia: opts.onViewAsMedia ?? (() => {}),
      onEncodingChange: opts.onEncodingChange ?? (() => {}),
      onToggleTail: opts.onToggleTail ?? (() => {}),
    },
  })
  return { target, instance }
}

describe('ViewerToolbar', () => {
  it('renders the file name, pickers, and tail toggle', async () => {
    const { target, instance } = mountToolbar({ fileName: 'notes.md' })
    await tick()

    const header = target.querySelector('header.viewer-toolbar')
    expect(header).not.toBeNull()
    expect(target.querySelector('.viewer-toolbar-title')?.textContent).toBe('notes.md')
    expect(triggerByLabel(target, 'View mode')).not.toBeNull()
    expect(triggerByLabel(target, 'Encoding')).not.toBeNull()
    const toggle = target.querySelector('.viewer-toolbar-toggle')
    expect(toggle).not.toBeNull()
    expect(toggle?.getAttribute('role')).toBe('switch')

    void unmount(instance)
  })

  it('preserves the data-tauri-drag-region attribute on the header so window dragging works', async () => {
    const { target, instance } = mountToolbar()
    await tick()

    const header = target.querySelector('header.viewer-toolbar')
    expect(header?.hasAttribute('data-tauri-drag-region')).toBe(true)

    void unmount(instance)
  })

  it('reflects tail mode via aria-checked and the active class', async () => {
    const { target, instance } = mountToolbar({ tailMode: true })
    await tick()

    const toggle = target.querySelector('.viewer-toolbar-toggle') as HTMLButtonElement
    expect(toggle.getAttribute('aria-checked')).toBe('true')
    expect(toggle.classList.contains('active')).toBe(true)

    void unmount(instance)
  })

  it('calls onToggleTail when the tail button is clicked', async () => {
    const onToggleTail = vi.fn()
    const { target, instance } = mountToolbar({ onToggleTail })
    await tick()

    const toggle = target.querySelector('.viewer-toolbar-toggle') as HTMLButtonElement
    toggle.click()
    await tick()

    expect(onToggleTail).toHaveBeenCalledTimes(1)

    void unmount(instance)
  })

  it('calls onEncodingChange when the user picks a different encoding', async () => {
    const onEncodingChange = vi.fn()
    const { target, instance } = mountToolbar({ onEncodingChange })
    await tick()

    // Open the encoding listbox, then pick UTF-16 LE (Ark routes selection only
    // while the content is open).
    triggerByLabel(target, 'Encoding')?.click()
    await tick()
    optionByValue(target, 'Encoding', 'utf16Le')?.click()
    await tick()

    expect(onEncodingChange).toHaveBeenCalledWith('utf16Le')

    void unmount(instance)
  })

  it('shows the reindexing indicator and disables the encoding picker while indexing', async () => {
    const { target, instance } = mountToolbar({ isIndexing: true })
    await tick()

    const indicator = target.querySelector('.viewer-toolbar-indexing')
    expect(indicator).not.toBeNull()
    expect(indicator?.getAttribute('role')).toBe('status')
    expect(triggerByLabel(target, 'Encoding')?.hasAttribute('data-disabled')).toBe(true)

    void unmount(instance)
  })

  it('keeps the text-only controls (encoding, tail) present but disabled in media mode', async () => {
    const { target, instance } = mountToolbar({ kind: 'image' })
    await tick()

    // The toolbar stays consistent across modes: same controls, same places. Encoding
    // and tail are text-only, so in media mode they render disabled rather than hidden.
    expect(triggerByLabel(target, 'View mode')).not.toBeNull()
    expect(triggerByLabel(target, 'Encoding')?.hasAttribute('data-disabled')).toBe(true)
    const tail = target.querySelector<HTMLButtonElement>('.viewer-toolbar-toggle')
    expect(tail).not.toBeNull()
    expect(tail?.disabled).toBe(true)

    void unmount(instance)
  })

  it('calls onViewAsText when the user picks "View as text" on a media file', async () => {
    const onViewAsText = vi.fn()
    const { target, instance } = mountToolbar({ kind: 'pdf', onViewAsText })
    await tick()

    triggerByLabel(target, 'View mode')?.click()
    await tick()
    optionByValue(target, 'View mode', 'viewAsText')?.click()
    await tick()

    expect(onViewAsText).toHaveBeenCalledTimes(1)

    void unmount(instance)
  })

  it('calls onViewAsMedia when the user picks "View as image" while reading a media file as text', async () => {
    const onViewAsMedia = vi.fn()
    const { target, instance } = mountToolbar({ kind: 'text', lastMediaKind: 'image', onViewAsMedia })
    await tick()

    triggerByLabel(target, 'View mode')?.click()
    await tick()
    optionByValue(target, 'View mode', 'viewAsMedia')?.click()
    await tick()

    expect(onViewAsMedia).toHaveBeenCalledTimes(1)

    void unmount(instance)
  })
})
