/**
 * Unit tests for the deep-link arrival seam shared between the settings page
 * (writer) and `KeyboardShortcutsSection` (reader).
 *
 * Covers both halves: the highlight state (set / get / clear) and the
 * filter-reset registration lifecycle (register → reset calls it,
 * unregister → reset no-ops, and unregistering a stale callback is a no-op so
 * a remount can't clobber a freshly-registered resetter).
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'

import {
  getPendingShortcutHighlight,
  setPendingShortcutHighlight,
  clearPendingShortcutHighlight,
  registerShortcutFilterReset,
  unregisterShortcutFilterReset,
  resetShortcutFilters,
} from './pending-shortcut-highlight.svelte'

describe('pending shortcut highlight', () => {
  beforeEach(() => {
    clearPendingShortcutHighlight()
    // No public read for the registered resetter; clearing is via unregister.
    // Each test registers what it needs.
  })

  it('starts with nothing pending', () => {
    expect(getPendingShortcutHighlight()).toBeNull()
  })

  it('round-trips a command id through set → get', () => {
    setPendingShortcutHighlight('downloads.goToLatest')
    expect(getPendingShortcutHighlight()).toBe('downloads.goToLatest')
  })

  it('clears back to null', () => {
    setPendingShortcutHighlight('file.quickLook')
    clearPendingShortcutHighlight()
    expect(getPendingShortcutHighlight()).toBeNull()
  })

  it('overwrites a pending id with a newer one', () => {
    setPendingShortcutHighlight('nav.back')
    setPendingShortcutHighlight('sort.byName')
    expect(getPendingShortcutHighlight()).toBe('sort.byName')
  })
})

describe('shortcut filter reset registration', () => {
  it('no-ops when no resetter is registered', () => {
    // Whatever a prior test registered, the unregister below clears it.
    const noop = vi.fn()
    registerShortcutFilterReset(noop)
    unregisterShortcutFilterReset(noop)
    expect(() => {
      resetShortcutFilters()
    }).not.toThrow()
  })

  it('calls the registered resetter', () => {
    const reset = vi.fn()
    registerShortcutFilterReset(reset)
    resetShortcutFilters()
    expect(reset).toHaveBeenCalledTimes(1)
    unregisterShortcutFilterReset(reset)
  })

  it('stops calling the resetter after it unregisters', () => {
    const reset = vi.fn()
    registerShortcutFilterReset(reset)
    unregisterShortcutFilterReset(reset)
    resetShortcutFilters()
    expect(reset).not.toHaveBeenCalled()
  })

  it('keeps the latest registration when an older callback unregisters', () => {
    // Remount race: the old section's onunmount fires AFTER the new section
    // registered. Unregistering the stale callback must not clear the live one.
    const oldReset = vi.fn()
    const newReset = vi.fn()
    registerShortcutFilterReset(oldReset)
    registerShortcutFilterReset(newReset)
    unregisterShortcutFilterReset(oldReset)
    resetShortcutFilters()
    expect(newReset).toHaveBeenCalledTimes(1)
    expect(oldReset).not.toHaveBeenCalled()
    unregisterShortcutFilterReset(newReset)
  })
})
