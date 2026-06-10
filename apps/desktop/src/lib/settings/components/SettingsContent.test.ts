/**
 * Search-gating tests for `SettingsContent.svelte`: which sections render for a
 * given global `searchQuery`. The Keyboard shortcuts section must show for any
 * query matching a registry command — including `showInPalette: false` ones
 * like "Open command palette" (regression: palette-only search hid it).
 *
 * Same global mocks as `SettingsContent.a11y.test.ts`: child sections pull
 * heavy state, so the Tauri boundaries are stubbed.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import SettingsContent from './SettingsContent.svelte'

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => undefined),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  openAppearanceSettings: vi.fn(() => Promise.resolve()),
  invoke: vi.fn(() => Promise.resolve(null)),
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

let target: HTMLElement
let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  target = document.createElement('div')
  document.body.appendChild(target)
})

afterEach(() => {
  if (component) {
    void unmount(component)
    component = null
  }
  target.remove()
})

async function render(searchQuery: string): Promise<void> {
  component = mount(SettingsContent, {
    target,
    props: { searchQuery, selectedSection: ['Appearance'] },
  })
  await tick()
}

function keyboardShortcutsSection(): HTMLElement | null {
  return target.querySelector('[data-section-id="keyboard-shortcuts"]')
}

describe('SettingsContent search gating for Keyboard shortcuts', () => {
  it('shows the section for a query matching a non-palette command ("palette")', async () => {
    await render('palette')
    expect(keyboardShortcutsSection()).not.toBeNull()
  })

  it('shows the section for a query matching a palette-visible command ("boarding")', async () => {
    await render('boarding')
    expect(keyboardShortcutsSection()).not.toBeNull()
  })

  it('hides the section when no command matches', async () => {
    await render('xyzzynonexistent')
    expect(keyboardShortcutsSection()).toBeNull()
  })
})
