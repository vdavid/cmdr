/**
 * The one deep link back to the "Show in Finder" switch.
 *
 * The section path and the anchor are the load-bearing part: the card renders
 * that id, and a drift here lands the reader in Settings with nothing scrolled
 * into view and no sign anything went wrong.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  openSettingsWindow: vi.fn(),
  warn: vi.fn(),
}))

vi.mock('$lib/settings/settings-window', () => ({ openSettingsWindow: mocks.openSettingsWindow }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: mocks.warn, info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { openSettingsToRevealHandler, REVEAL_HANDLER_ANCHOR_ID } from './reveal-settings-link'

beforeEach(() => {
  vi.clearAllMocks()
  mocks.openSettingsWindow.mockResolvedValue(undefined)
})

describe('openSettingsToRevealHandler', () => {
  it('lands on the card the reveal switch lives in, under its own surface', async () => {
    await openSettingsToRevealHandler()

    expect(mocks.openSettingsWindow).toHaveBeenCalledWith(
      'reveal-toast',
      ['Behavior', 'Navigation & file ops'],
      REVEAL_HANDLER_ANCHOR_ID,
    )
  })

  /**
   * The caller is a toast that has already said its piece, so a window that
   * won't open is a log line, ❌ never a throw out of a button handler.
   */
  it('swallows a window that will not open', async () => {
    mocks.openSettingsWindow.mockRejectedValue(new Error('no window'))

    await expect(openSettingsToRevealHandler()).resolves.toBeUndefined()
    expect(mocks.warn).toHaveBeenCalledOnce()
  })
})
