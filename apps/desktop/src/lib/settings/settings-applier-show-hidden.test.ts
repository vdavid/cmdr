/**
 * The settings-applier's native-menu wiring for `listing.showHiddenFiles`.
 *
 * "Show hidden files" is reachable from four places (the Settings row, `⌘⇧.`,
 * the command palette, and the View menu's own CheckMenuItem), and they must
 * never disagree. The applier is the ONE place that pushes the setting onto the
 * menu, so every one of those origins converges here: whoever writes the
 * setting, the check mark follows.
 *
 * Startup is deliberately NOT a push: Rust builds that menu item checked from
 * the same persisted key (`settings/loader.rs`), so a startup sync would be a
 * second source of truth for the same fact.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const syncMenuShowHidden = vi.fn<(checked: boolean) => Promise<void>>(() => Promise.resolve())

/** The change listener the applier registers, captured so the test can fire it. */
let changeListener: ((change: { id: string; value: unknown }) => void) | undefined

// Stub every Tauri wrapper to a no-op promise (the applier fires a batch of
// fire-and-forget backend pushes at startup), keeping a real spy on the one
// seam under test.
vi.mock('$lib/tauri-commands', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const noop = () => Promise.resolve()
  const stubbed: Record<string, unknown> = {}
  for (const key of Object.keys(actual)) {
    stubbed[key] = typeof actual[key] === 'function' ? noop : actual[key]
  }
  stubbed.syncMenuShowHidden = (checked: boolean) => syncMenuShowHidden(checked)
  return stubbed
})

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/settings')>()
  return {
    ...actual,
    initializeSettings: vi.fn().mockResolvedValue(undefined),
    onSettingChange: (listener: (change: { id: string; value: unknown }) => void) => {
      changeListener = listener
      return () => {
        changeListener = undefined
      }
    },
  }
})

import { initSettingsApplier, cleanupSettingsApplier } from './settings-applier'

beforeEach(() => {
  syncMenuShowHidden.mockClear()
  changeListener = undefined
})

afterEach(() => {
  cleanupSettingsApplier()
})

describe('settings-applier: listing.showHiddenFiles', () => {
  it('pushes the new value to the native menu when the setting turns on', async () => {
    await initSettingsApplier()
    syncMenuShowHidden.mockClear()
    changeListener?.({ id: 'listing.showHiddenFiles', value: true })
    expect(syncMenuShowHidden).toHaveBeenCalledExactlyOnceWith(true)
  })

  it('pushes the new value to the native menu when the setting turns off', async () => {
    await initSettingsApplier()
    syncMenuShowHidden.mockClear()
    changeListener?.({ id: 'listing.showHiddenFiles', value: false })
    expect(syncMenuShowHidden).toHaveBeenCalledExactlyOnceWith(false)
  })

  it('does not push at startup (Rust builds the item from the same persisted key)', async () => {
    await initSettingsApplier()
    expect(syncMenuShowHidden).not.toHaveBeenCalled()
  })

  it('leaves the menu alone for an unrelated setting', async () => {
    await initSettingsApplier()
    syncMenuShowHidden.mockClear()
    changeListener?.({ id: 'listing.stripedRows', value: true })
    expect(syncMenuShowHidden).not.toHaveBeenCalled()
  })
})
