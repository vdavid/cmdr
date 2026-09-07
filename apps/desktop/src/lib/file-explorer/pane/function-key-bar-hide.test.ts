/**
 * Tests for `handleFunctionKeyBarHideRequested`: the "Hide function key bar"
 * context-menu handler.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { setSettingMock, addToastMock } = vi.hoisted(() => ({
  setSettingMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
}))

vi.mock('$lib/settings', () => ({
  setSetting: setSettingMock,
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
}))

// The toast component import returns an opaque module ref; we only assert
// the same reference reaches `addToast`.
vi.mock('./FunctionKeyBarHiddenToastContent.svelte', () => ({
  default: { __toastContent: 'FunctionKeyBarHiddenToastContent' },
}))

import { handleFunctionKeyBarHideRequested, FUNCTION_KEY_BAR_HIDDEN_TOAST_ID } from './function-key-bar-hide'
import FunctionKeyBarHiddenToastContent from './FunctionKeyBarHiddenToastContent.svelte'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('handleFunctionKeyBarHideRequested', () => {
  it('turns off the setting and raises the self-dismissing INFO toast', () => {
    handleFunctionKeyBarHideRequested()

    expect(setSettingMock).toHaveBeenCalledExactlyOnceWith('appearance.showFunctionKeyBar', false)
    expect(addToastMock).toHaveBeenCalledExactlyOnceWith(FunctionKeyBarHiddenToastContent, {
      id: FUNCTION_KEY_BAR_HIDDEN_TOAST_ID,
      level: 'info',
    })
  })
})
