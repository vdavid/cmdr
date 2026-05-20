import { describe, it, expect, vi, beforeEach } from 'vitest'

const { getSettingMock, addToastMock, getToastsMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn<(id: string) => boolean>(),
  addToastMock: vi.fn(),
  getToastsMock: vi.fn<() => Array<{ id: string }>>(() => []),
}))

vi.mock('$lib/settings', () => ({
  getSetting: getSettingMock,
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
  getToasts: getToastsMock,
}))

// Stub the toast content component import — Svelte component imports in jsdom
// don't need to evaluate the template here; we only assert that the imported
// reference is what `addToast` receives.
vi.mock('./QuickLookHintToastContent.svelte', () => ({
  default: { __toastContent: 'QuickLookHintToastContent' },
}))

import { maybeShowQuickLookHint, QUICK_LOOK_HINT_TOAST_ID } from './quick-look-hint'
import QuickLookHintToastContent from './QuickLookHintToastContent.svelte'

describe('maybeShowQuickLookHint', () => {
  beforeEach(() => {
    getSettingMock.mockReset()
    addToastMock.mockReset()
    getToastsMock.mockReset()
    // Defaults that match a freshly-installed app: not suppressed, no toasts on screen.
    getSettingMock.mockReturnValue(false)
    getToastsMock.mockReturnValue([])
  })

  it('shows the toast on first Space press', () => {
    maybeShowQuickLookHint()

    expect(getSettingMock).toHaveBeenCalledWith('fileExplorer.suppressQuickLookHint')
    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content, options] = addToastMock.mock.calls[0] as [unknown, Record<string, unknown>]
    expect(content).toBe(QuickLookHintToastContent)
    expect(options).toMatchObject({
      id: QUICK_LOOK_HINT_TOAST_ID,
      level: 'info',
      dismissal: 'persistent',
    })
  })

  it('shows the toast again on a later Space press once a previous instance has been dismissed', () => {
    // First press: toast appears.
    maybeShowQuickLookHint()
    expect(addToastMock).toHaveBeenCalledTimes(1)

    // User dismissed it via X — `getToasts()` returns an empty array again.
    // Second press: toast appears again. The hint keeps reminding until the
    // user opts out via "Don't show again."
    maybeShowQuickLookHint()
    expect(addToastMock).toHaveBeenCalledTimes(2)
  })

  it('does nothing while the toast is currently visible', () => {
    getToastsMock.mockReturnValue([{ id: QUICK_LOOK_HINT_TOAST_ID }])

    maybeShowQuickLookHint()

    // Toast already on screen — don't re-add or replace it.
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('does nothing when the user has suppressed the hint permanently', () => {
    getSettingMock.mockReturnValue(true)

    maybeShowQuickLookHint()

    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('checks the suppress setting before checking toast visibility', () => {
    // Order matters: if the user has opted out, we shouldn't even bother
    // walking the toast list. This is a sanity test on the gate ordering.
    getSettingMock.mockReturnValue(true)
    getToastsMock.mockReturnValue([])

    maybeShowQuickLookHint()

    expect(getSettingMock).toHaveBeenCalledWith('fileExplorer.suppressQuickLookHint')
    expect(getToastsMock).not.toHaveBeenCalled()
    expect(addToastMock).not.toHaveBeenCalled()
  })
})
