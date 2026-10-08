/**
 * The once-ever toast the first quick filter raises. It pins that the toast
 * comes up ONCE (stamped on the way up) and what its two choices do: Jump
 * switches the typing mode, Settings deep-links to that row.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
  getSetting: vi.fn(),
  setSetting: vi.fn(),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: mocks.addToast, dismissToast: mocks.dismissToast }))
vi.mock('$lib/settings', () => ({ getSetting: mocks.getSetting, setSetting: mocks.setSetting }))
vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: mocks.openSettingsWindow,
  settingAnchorId: (id: string) => `anchor-${id}`,
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ info: vi.fn(), warn: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('./QuickFilterIntroToastContent.svelte', () => ({ default: {} }))

import { maybeShowQuickFilterIntro } from './quick-filter-intro'
import { QUICK_FILTER_INTRO_TOAST_ID, openTypingModeSettings, switchToJumpMode } from './quick-filter-intro-actions'

let seen: boolean

beforeEach(() => {
  vi.clearAllMocks()
  seen = false
  mocks.getSetting.mockImplementation(() => seen)
  mocks.setSetting.mockImplementation((id: string, value: unknown) => {
    if (id === 'fileExplorer.quickFilterIntroSeen') seen = value as boolean
  })
})

describe('the first quick filter toast', () => {
  it('comes up once, stamped on the way up, and waits to be read', () => {
    maybeShowQuickFilterIntro()
    maybeShowQuickFilterIntro()

    expect(mocks.addToast).toHaveBeenCalledTimes(1)
    expect(mocks.addToast.mock.calls[0][1]).toMatchObject({ dismissal: 'persistent', id: QUICK_FILTER_INTRO_TOAST_ID })
    expect(mocks.setSetting).toHaveBeenCalledWith('fileExplorer.quickFilterIntroSeen', true)
  })

  it('switches typing to Jump and closes', () => {
    switchToJumpMode()

    expect(mocks.setSetting).toHaveBeenCalledWith('fileExplorer.typeToJump.mode', 'jump')
    expect(mocks.dismissToast).toHaveBeenCalledWith(QUICK_FILTER_INTRO_TOAST_ID)
  })

  it('opens Settings at the typing-mode row', async () => {
    await openTypingModeSettings()

    expect(mocks.openSettingsWindow).toHaveBeenCalledWith(
      'quick-filter-toast',
      ['Appearance', 'Listing'],
      'anchor-fileExplorer.typeToJump.mode',
    )
  })
})
