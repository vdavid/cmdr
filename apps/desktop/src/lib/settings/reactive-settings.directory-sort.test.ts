/**
 * The directory sort mode every listing hands the backend comparator (#291).
 *
 * Two settings feed it: "Show folders first" decides whether folders lead at all,
 * and "Sort folders" decides how they sort among themselves while they do.
 * Folders mixed in with files sort by the column like any file, so "Sort
 * directories" has nothing to say then.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const { stored, listeners } = vi.hoisted(() => ({
  stored: new Map<string, unknown>(),
  listeners: [] as ((change: { id: string; value: unknown }) => void)[],
}))

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => stored.get(id),
  onSettingChange: (listener: (change: { id: string; value: unknown }) => void) => {
    listeners.push(listener)
    return () => {}
  },
  initializeSettings: () => Promise.resolve(),
  densityMappings: {},
}))
vi.mock('$lib/icon-cache', () => ({ clearExtensionIconCache: () => Promise.resolve() }))

import { cleanupReactiveSettings, getDirectorySortMode, initReactiveSettings } from './reactive-settings.svelte'

function changeSetting(id: string, value: unknown): void {
  for (const listener of listeners) listener({ id, value })
}

describe('getDirectorySortMode', () => {
  beforeEach(async () => {
    stored.clear()
    listeners.length = 0
    stored.set('listing.foldersFirst', true)
    stored.set('listing.directorySortMode', 'alwaysByName')
    await initReactiveSettings()
  })

  afterEach(() => {
    cleanupReactiveSettings()
  })

  it('passes the "Sort folders" choice through while folders lead (the default)', () => {
    expect(getDirectorySortMode()).toBe('alwaysByName')
  })

  it('mixes folders in with files once "Show folders first" goes off, and back when it comes on', () => {
    changeSetting('listing.foldersFirst', false)
    expect(getDirectorySortMode()).toBe('mixedWithFiles')

    // "Sort folders" changing meanwhile is remembered, not applied.
    changeSetting('listing.directorySortMode', 'likeFiles')
    expect(getDirectorySortMode()).toBe('mixedWithFiles')

    changeSetting('listing.foldersFirst', true)
    expect(getDirectorySortMode()).toBe('likeFiles')
  })
})
