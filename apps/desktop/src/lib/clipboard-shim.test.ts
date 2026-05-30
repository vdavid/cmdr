import { describe, it, expect, vi, beforeEach } from 'vitest'
import { installClipboardShimIfE2e, _resetClipboardShimForTests } from './clipboard-shim'

const { isE2eModeSpy } = vi.hoisted(() => ({
  isE2eModeSpy: vi.fn<() => Promise<boolean>>(),
}))
vi.mock('$lib/tauri-commands', () => ({ isE2eMode: isE2eModeSpy }))

describe('installClipboardShimIfE2e', () => {
  let realWriteText: ReturnType<typeof vi.fn>
  let realReadText: ReturnType<typeof vi.fn>

  beforeEach(() => {
    _resetClipboardShimForTests()
    isE2eModeSpy.mockReset()
    realWriteText = vi.fn(() => Promise.resolve())
    realReadText = vi.fn(() => Promise.resolve('from-os'))
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: realWriteText, readText: realReadText },
      configurable: true,
    })
  })

  it('routes writeText/readText through an in-memory store under E2E, never the OS clipboard', async () => {
    isE2eModeSpy.mockResolvedValue(true)
    await installClipboardShimIfE2e()

    await navigator.clipboard.writeText('AAAA')
    expect(realWriteText).not.toHaveBeenCalled()
    await expect(navigator.clipboard.readText()).resolves.toBe('AAAA')
    expect(realReadText).not.toHaveBeenCalled()
  })

  it('leaves the real clipboard untouched when not under E2E', async () => {
    isE2eModeSpy.mockResolvedValue(false)
    await installClipboardShimIfE2e()

    await navigator.clipboard.writeText('hello')
    expect(realWriteText).toHaveBeenCalledWith('hello')
  })
})
