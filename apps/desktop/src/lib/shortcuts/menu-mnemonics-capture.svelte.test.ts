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

// A reload (or HMR) mid-capture resets the claim count, but the mask is a property of the GTK
// window, so it would stay on with nobody left to lift it.
describe('a fresh load gives the mnemonics back', () => {
  async function loadIn(label: string): Promise<boolean[]> {
    vi.resetModules()
    vi.doMock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label }) }))
    const tauriCommands = await import('#lib/tauri-commands/index.js')
    vi.mocked(tauriCommands.setMenuMnemonicsSuspended).mockClear()
    await import('./menu-mnemonics-capture.svelte')
    const calls = vi.mocked(tauriCommands.setMenuMnemonicsSuspended).mock.calls.map(([suspended]) => suspended)
    vi.doUnmock('@tauri-apps/api/window')
    return calls
  }

  it('in a window other than the main one', async () => {
    expect(await loadIn('settings')).toEqual([false])
  })

  it('but never in the main window, whose dialog gate is its one writer', async () => {
    expect(await loadIn('main')).toEqual([])
  })
})
