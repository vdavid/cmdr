import { describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import RawByteView from './RawByteView.svelte'
import { viewerGetBytes } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  viewerGetBytes: vi.fn(() => Promise.resolve([0, 0x41, 0x0a, 0xff])),
}))

describe('RawByteView', () => {
  it('renders the original byte sequence in hex and character columns', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(RawByteView, {
      target,
      props: {
        sessionId: 'raw-test',
        fileName: 'sample.bin',
        totalBytes: 4,
        mode: 'hex',
        initialOffset: 0,
        onOffsetChange: () => {},
      },
    })

    await vi.waitFor(async () => {
      await tick()
      expect(target.querySelector('.raw-hex')?.textContent.trimEnd()).toBe('00 41 0A FF')
    })
    expect(target.querySelector('.raw-characters')?.textContent).toBe('·A·ÿ')
    expect(viewerGetBytes).toHaveBeenCalledWith('raw-test', 0, 16)

    await unmount(instance)
  })

  it('keeps the previous rows on screen while the next chunk is still loading', async () => {
    const getBytes = vi.mocked(viewerGetBytes)
    getBytes.mockImplementation((_session, offset, count) =>
      Promise.resolve(Array.from({ length: count }, (_, i) => (offset + i) % 256)),
    )
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(RawByteView, {
      target,
      props: {
        sessionId: 'raw-flicker',
        fileName: 'big.bin',
        totalBytes: 16 * 200,
        mode: 'hex',
        initialOffset: 0,
        onOffsetChange: () => {},
      },
    })
    await vi.waitFor(async () => {
      await tick()
      expect(target.querySelectorAll('.raw-row').length).toBeGreaterThan(0)
    })

    getBytes.mockImplementation(() => new Promise(() => {}))
    const view = target.querySelector<HTMLElement>('.raw-byte-view')
    if (!view) throw new Error('no raw view')
    view.scrollTop = 20 * 50
    view.dispatchEvent(new Event('scroll'))
    await tick()

    expect(target.querySelectorAll('.raw-row').length).toBeGreaterThan(0)
    await unmount(instance)
  })

  it('steps whole rows by wheel and keys when the file is too big for a natural scrollbar', async () => {
    vi.mocked(viewerGetBytes).mockImplementation(() => Promise.resolve([]))
    const offsets: number[] = []
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(RawByteView, {
      target,
      props: {
        sessionId: 'raw-huge',
        fileName: 'huge.bin',
        totalBytes: 50_000_000_000,
        mode: 'hex',
        initialOffset: 0,
        onOffsetChange: (offset: number) => offsets.push(offset),
      },
    })
    await tick()
    const view = target.querySelector<HTMLElement>('.raw-byte-view')
    if (!view) throw new Error('no raw view')

    view.dispatchEvent(new WheelEvent('wheel', { deltaY: 20, deltaMode: 0, cancelable: true }))
    await tick()
    expect(offsets[offsets.length - 1]).toBe(16)

    view.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', cancelable: true, bubbles: true }))
    await tick()
    expect(offsets[offsets.length - 1]).toBe(32)

    await unmount(instance)
  })
})
