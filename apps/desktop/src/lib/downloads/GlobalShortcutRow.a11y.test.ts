import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { getGlobalRevealBindingMock } = vi.hoisted(() => ({
  getGlobalRevealBindingMock: vi.fn<() => string>(),
}))

vi.mock('./global-shortcut-setting', () => ({
  getGlobalRevealBinding: getGlobalRevealBindingMock,
  setGlobalRevealBinding: vi.fn(),
  GLOBAL_REVEAL_BINDING_KEY: 'behavior.fileSystemWatching.globalRevealShortcut.binding',
  GLOBAL_REVEAL_ENABLED_KEY: 'behavior.fileSystemWatching.globalRevealShortcut.enabled',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => true),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: { setGlobalRevealShortcut: vi.fn() },
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

import GlobalShortcutRow from './GlobalShortcutRow.svelte'

describe('GlobalShortcutRow a11y', () => {
  beforeEach(() => {
    getGlobalRevealBindingMock.mockReset()
  })

  it('renders the default (unmodified) state with no a11y violations', async () => {
    getGlobalRevealBindingMock.mockReturnValue('⌃⌥⌘J')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutRow, { target })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the modified state (with reset button) with no a11y violations', async () => {
    getGlobalRevealBindingMock.mockReturnValue('⌃⌥⌘K')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutRow, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
