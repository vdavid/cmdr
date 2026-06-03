import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { getGlobalGoToLatestBindingMock } = vi.hoisted(() => ({
  getGlobalGoToLatestBindingMock: vi.fn<() => string>(),
}))

vi.mock('./global-shortcut-setting', () => ({
  getGlobalGoToLatestBinding: getGlobalGoToLatestBindingMock,
  setGlobalGoToLatestBinding: vi.fn(),
  GLOBAL_GO_TO_LATEST_BINDING_KEY: 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding',
  GLOBAL_GO_TO_LATEST_ENABLED_KEY: 'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => true),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: { setGlobalGoToLatestShortcut: vi.fn() },
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

import GlobalShortcutRow from './GlobalShortcutRow.svelte'

describe('GlobalShortcutRow a11y', () => {
  beforeEach(() => {
    getGlobalGoToLatestBindingMock.mockReset()
  })

  it('renders the default (unmodified) state with no a11y violations', async () => {
    getGlobalGoToLatestBindingMock.mockReturnValue('⌃⌥⌘J')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutRow, { target })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the modified state (with reset button) with no a11y violations', async () => {
    getGlobalGoToLatestBindingMock.mockReturnValue('⌃⌥⌘K')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutRow, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
