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
})
