/**
 * The setting F4 reads and the deep link every text-editor toast's "Open settings"
 * lands on. The settings store and the Settings window are mocked.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

// Untyped on purpose: a typed stub of `openSettingsWindow` would declare two same-typed
// positional string parameters, which `cmdr/no-confusable-callback-params` rightly
// refuses. The assertion below names the arguments it expects.
const m = vi.hoisted(() => ({
  openSettingsWindow: vi.fn(),
  warn: vi.fn(),
  settings: new Map<string, unknown>(),
}))

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => m.settings.get(id),
  setSetting: (id: string, value: unknown) => {
    m.settings.set(id, value)
  },
}))

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: m.openSettingsWindow,
  settingAnchorId: (id: string) => `setting-${id}`,
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: m.warn, info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { getTextEditorChoice, openSettingsToTextEditor, setTextEditorChoice } from './text-editor-setting'

const APP_KEY = 'behavior.textEditorApp'

beforeEach(() => {
  vi.clearAllMocks()
  m.settings.clear()
  m.openSettingsWindow.mockResolvedValue(undefined)
})

describe('the stored choice', () => {
  it('reads an empty string as the system default', () => {
    m.settings.set(APP_KEY, '')

    expect(getTextEditorChoice()).toBe('system')
  })

  it('reads back what was written', () => {
    setTextEditorChoice('/Applications/Sublime Text.app')

    expect(getTextEditorChoice()).toBe('/Applications/Sublime Text.app')
  })
})

describe('openSettingsToTextEditor', () => {
  it('opens Navigation & file ops at the text editor row, under its own surface', async () => {
    await openSettingsToTextEditor()

    expect(m.openSettingsWindow).toHaveBeenCalledExactlyOnceWith(
      'text-editor-toast',
      ['Behavior', 'Navigation & file ops'],
      `setting-${APP_KEY}`,
    )
  })

  it('logs a window that won’t open instead of throwing out of a toast button', async () => {
    m.openSettingsWindow.mockRejectedValue(new Error('no window'))

    await expect(openSettingsToTextEditor()).resolves.toBeUndefined()

    expect(m.warn).toHaveBeenCalledOnce()
  })
})
