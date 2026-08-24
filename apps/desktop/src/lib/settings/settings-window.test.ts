/**
 * Unit tests for the pure anchor-id helpers shared between
 * `openShortcutCustomization` (deep-link writer) and `KeyboardShortcutsSection`
 * (the row that renders the anchor) / the settings page (the arrival reader),
 * plus the `settings_opened` analytics every open path funnels through.
 *
 * `settings-window.ts` statically imports Tauri window APIs at module scope, so
 * those are mocked here; the anchor helpers touch none of them.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { getByLabel, trackEvent } = vi.hoisted(() => ({
  getByLabel: vi.fn<(label: string) => Promise<unknown>>(),
  trackEvent: vi.fn(() => Promise.resolve()),
}))

vi.mock('@tauri-apps/api/webviewWindow', () => ({ WebviewWindow: Object.assign(vi.fn(), { getByLabel }) }))
vi.mock('@tauri-apps/api/dpi', () => ({ LogicalPosition: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ emitTo: () => Promise.resolve() }))
vi.mock('@tauri-apps/api/window', () => ({ Effect: {}, EffectState: {} }))
vi.mock('$lib/tauri-commands', () => ({ getShouldReduceTransparency: () => Promise.resolve(false), trackEvent }))

import { shortcutAnchorId, commandIdFromShortcutAnchor, openSettingsWindow } from './settings-window'

describe('shortcutAnchorId / commandIdFromShortcutAnchor', () => {
  it('builds the `shortcut-<id>` anchor convention', () => {
    expect(shortcutAnchorId('downloads.goToLatest')).toBe('shortcut-downloads.goToLatest')
    expect(shortcutAnchorId('file.quickLook')).toBe('shortcut-file.quickLook')
  })

  it('round-trips: anchor → command id', () => {
    for (const id of ['file.quickLook', 'downloads.goToLatest', 'nav.back', 'sort.byName']) {
      expect(commandIdFromShortcutAnchor(shortcutAnchorId(id))).toBe(id)
    }
  })

  it('preserves a command id containing a dot (no eager split)', () => {
    expect(commandIdFromShortcutAnchor('shortcut-a.b.c')).toBe('a.b.c')
  })

  it('returns null for non-shortcut anchors', () => {
    expect(commandIdFromShortcutAnchor('settings-downloads-notifications')).toBeNull()
    expect(commandIdFromShortcutAnchor('appearance-colors-and-formats')).toBeNull()
    expect(commandIdFromShortcutAnchor('')).toBeNull()
  })

  it('treats a bare `shortcut-` (no command id) as an empty-id match, not null', () => {
    // The prefix is present, so it parses to the empty string. A caller that
    // builds anchors only via `shortcutAnchorId` never produces this, but the
    // contract is "prefix present → not null".
    expect(commandIdFromShortcutAnchor('shortcut-')).toBe('')
  })
})

describe('openSettingsWindow analytics', () => {
  beforeEach(() => {
    trackEvent.mockClear()
    // Pretend Settings is already open: `openSettingsWindow` takes its early
    // return and never reaches the window-creation path (which needs far more
    // of Tauri than this suite mocks).
    getByLabel.mockResolvedValue({})
  })

  it('reports `settings_opened` once, tagged with the surface that asked', async () => {
    await openSettingsWindow('crash-toast')
    expect(trackEvent).toHaveBeenCalledTimes(1)
    expect(trackEvent).toHaveBeenCalledWith('settings_opened', { surface: 'crash-toast' })
  })

  it('fires for a deep-link open too, and never leaks the section into the props', async () => {
    await openSettingsWindow('paste-toast', ['Behavior', 'Navigation & file ops'])
    expect(trackEvent).toHaveBeenCalledWith('settings_opened', { surface: 'paste-toast' })
  })

  it('counts a re-open of an already-open Settings window', async () => {
    // The event answers "how often does someone go to Settings, and from where",
    // so a second visit in one session is a second data point.
    await openSettingsWindow('command')
    await openSettingsWindow('enter-menu')
    expect(trackEvent).toHaveBeenCalledTimes(2)
  })
})
