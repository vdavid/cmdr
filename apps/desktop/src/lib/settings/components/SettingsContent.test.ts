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
  openPrivacySettings: vi.fn(() => Promise.resolve()),
  invoke: vi.fn(() => Promise.resolve(null)),
  listen: vi.fn(() => Promise.resolve(() => {})),
  // FileSystemWatchingSection now reaches these IPCs through the wrapper layer.
  downloadsWatcherStatus: vi.fn(() =>
    Promise.resolve({ status: 'ok', data: { running: true, downloadsDir: '/d', fdaPending: false } }),
  ),
  recheckDownloadsWatcherGate: vi.fn(() => Promise.resolve({ status: 'ok', data: null })),
  setGlobalGoToLatestShortcut: vi.fn(() =>
    Promise.resolve({ status: 'ok', data: { status: 'registered', binding: '', enabled: true } }),
  ),
  getIndexStatus: vi.fn(() => Promise.resolve({ status: 'ok', data: { dbFileSize: 1024 } })),
  clearDriveIndex: vi.fn(() => Promise.resolve({ status: 'ok', data: null })),
}))

// `FileSystemWatchingSection` (rendered when an "index size" search keeps it
// visible) calls a handful of backend IPCs on mount. Stub them so the section
// mounts without a Tauri runtime.
vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    downloadsWatcherStatus: vi.fn(() =>
      Promise.resolve({ status: 'ok', data: { running: true, downloadsDir: '/d', fdaPending: false } }),
    ),
    recheckDownloadsWatcherGate: vi.fn(() => Promise.resolve({ status: 'ok', data: null })),
    setGlobalGoToLatestShortcut: vi.fn(() =>
      Promise.resolve({ status: 'ok', data: { status: 'registered', binding: '', enabled: true } }),
    ),
    getIndexStatus: vi.fn(() => Promise.resolve({ status: 'ok', data: { dbFileSize: 1024 } })),
    clearDriveIndex: vi.fn(() => Promise.resolve({ status: 'ok', data: null })),
  },
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

function fileSystemWatchingSection(): HTMLElement | null {
  return target.querySelector('[data-section-id="behavior-file-system-watching"]')
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

describe('SettingsContent search gating for the index-size row', () => {
  it('keeps File system watching visible and shows the Drive-indexing card for "index size"', async () => {
    // Pre-fix this showed a blank pane: "index size" is a hand-rendered action
    // row, not a registry setting, so `sectionHasMatchingSettings` matched
    // nothing and hid the whole section. The hidden `indexing.indexSize` anchor
    // makes the section match again.
    await render('index size')
    const section = fileSystemWatchingSection()
    if (!section) throw new Error('File system watching section not rendered')
    // The Drive-indexing card renders (its label is the indexing toggle label).
    const labels = Array.from(section.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
    expect(labels).toContain('Drive indexing')
  })
})
