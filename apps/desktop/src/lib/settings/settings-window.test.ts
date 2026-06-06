/**
 * Unit tests for the pure anchor-id helpers shared between
 * `openShortcutCustomization` (deep-link writer) and `KeyboardShortcutsSection`
 * (the row that renders the anchor) / the settings page (the arrival reader).
 *
 * `settings-window.ts` statically imports Tauri window APIs at module scope, so
 * those are mocked here; the functions under test touch none of them.
 */

import { describe, it, expect, vi } from 'vitest'

vi.mock('@tauri-apps/api/webviewWindow', () => ({ WebviewWindow: vi.fn() }))
vi.mock('@tauri-apps/api/dpi', () => ({ LogicalPosition: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ emitTo: () => Promise.resolve() }))
vi.mock('@tauri-apps/api/window', () => ({ Effect: {}, EffectState: {} }))

import { shortcutAnchorId, commandIdFromShortcutAnchor } from './settings-window'

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
