/**
 * The once-ever toast for a key that ran its command instead of typing into the quick
 * filter. It pins that the toast comes up ONCE (stamped on the way up), hides by itself
 * after 10 seconds, and names the key and the window the way a person reads them.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  addToast: vi.fn(),
  getSetting: vi.fn(),
  setSetting: vi.fn(),
}))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: mocks.addToast }))
vi.mock('#lib/settings/index.js', () => ({ getSetting: mocks.getSetting, setSetting: mocks.setSetting }))
vi.mock('./QuickFilterTypingHintToastContent.svelte', () => ({ default: {} }))

import { QUICK_FILTER_TYPING_HINT_TOAST_ID, maybeShowQuickFilterTypingHint } from './quick-filter-typing-hint'

let seen: boolean

beforeEach(() => {
  vi.clearAllMocks()
  seen = false
  mocks.getSetting.mockImplementation(() => seen)
  mocks.setSetting.mockImplementation((id: string, value: unknown) => {
    if (id === 'fileExplorer.quickFilterTypingHintSeen') seen = value as boolean
  })
})

describe('the quick filter typing hint', () => {
  it('comes up once, stamped on the way up, and hides by itself after 10 seconds', () => {
    maybeShowQuickFilterTypingHint('*', 1500)
    maybeShowQuickFilterTypingHint('-', 1500)

    expect(mocks.addToast).toHaveBeenCalledTimes(1)
    expect(mocks.addToast.mock.calls[0][1]).toMatchObject({
      id: QUICK_FILTER_TYPING_HINT_TOAST_ID,
      dismissal: 'transient',
      timeoutMs: 10_000,
      props: { keyLabel: '*', seconds: 1.5, secondsText: '1.5' },
    })
    expect(mocks.setSetting).toHaveBeenCalledWith('fileExplorer.quickFilterTypingHintSeen', true)
  })

  it('names Space by its key name, since a bare space shows as nothing', () => {
    maybeShowQuickFilterTypingHint(' ', 2000)
    expect(mocks.addToast.mock.calls[0][1]).toMatchObject({ props: { keyLabel: 'Space', secondsText: '2' } })
  })

  it('stays down once seen', () => {
    seen = true
    maybeShowQuickFilterTypingHint('*', 1500)
    expect(mocks.addToast).not.toHaveBeenCalled()
  })
})
