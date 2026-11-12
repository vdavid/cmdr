import { describe, it, expect, vi, beforeEach } from 'vitest'
import { flushSync } from 'svelte'
import { claimMenuMnemonics, suspendMenuMnemonicsWhile } from './menu-mnemonics-capture.svelte'
import { setMenuMnemonicsSuspended } from '#lib/tauri-commands/index.js'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/tauri-commands/index.js')>()),
  setMenuMnemonicsSuspended: vi.fn(() => Promise.resolve()),
}))

function pushes(): boolean[] {
  return vi.mocked(setMenuMnemonicsSuspended).mock.calls.map(([suspended]) => suspended)
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('a shortcut recorder takes the menu bar mnemonics while it listens', () => {
  it('gives them back only when the last recorder lets go', () => {
    const first = claimMenuMnemonics()
    const second = claimMenuMnemonics()
    first()
    first()
    expect(pushes()).toEqual([true])

    second()
    expect(pushes()).toEqual([true, false])
  })

  it('follows the capturing flag, and an unmount mid-capture gives them back', () => {
    let capturing = $state(false)
    const dispose = $effect.root(() => {
      suspendMenuMnemonicsWhile(() => capturing)
    })
    flushSync()
    expect(pushes()).toEqual([])

    capturing = true
    flushSync()
    capturing = false
    flushSync()
    capturing = true
    flushSync()
    expect(pushes()).toEqual([true, false, true])

    dispose()
    expect(pushes()).toEqual([true, false, true, false])
  })
})
