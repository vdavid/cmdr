import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewerToolbar from './ViewerToolbar.svelte'
import type { EncodingChoice, FileEncoding } from '$lib/ipc/bindings'

const choices: EncodingChoice[] = [
  { encoding: 'utf8', label: 'UTF-8', group: 'unicode' },
  { encoding: 'utf16Le', label: 'UTF-16 LE', group: 'unicode' },
  { encoding: 'windows1252', label: 'Western (Windows-1252)', group: 'western' },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

interface MountOpts {
  fileName?: string
  currentEncoding?: FileEncoding
  detectedEncoding?: FileEncoding
  isIndexing?: boolean
  tailMode?: boolean
  onViewModeChange?: (mode: 'text') => void
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
      viewMode: 'text',
      currentEncoding: opts.currentEncoding ?? 'utf8',
      detectedEncoding: opts.detectedEncoding ?? 'utf8',
      encodingChoices: choices,
      isIndexing: opts.isIndexing ?? false,
      tailMode: opts.tailMode ?? false,
      onViewModeChange: opts.onViewModeChange ?? (() => {}),
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
    expect(target.querySelector('select.view-mode-picker')).not.toBeNull()
    expect(target.querySelector('select.encoding-picker')).not.toBeNull()
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

    const select = target.querySelector('select.encoding-picker') as HTMLSelectElement
    select.value = 'utf16Le'
    select.dispatchEvent(new Event('change', { bubbles: true }))
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
    const select = target.querySelector('select.encoding-picker') as HTMLSelectElement
    expect(select.disabled).toBe(true)

    void unmount(instance)
  })
})
