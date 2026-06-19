/**
 * Tests for the FE-owned drive-indexing prefs: per-drive silences and the
 * one-time stale-dialog flag, stored as hidden settings. The settings store is
 * mocked so the JSON-array plumbing is exercised without disk I/O.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

let store: Record<string, unknown>

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => store[id],
  setSetting: (id: string, value: unknown) => {
    store[id] = value
  },
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

import {
  getSilencedDrives,
  isDriveSilenced,
  silenceDrive,
  clearSilencedDrives,
  hasSilencedDrives,
  hasShownFirstStaleDialog,
  markFirstStaleDialogShown,
} from './drive-index-prefs'

beforeEach(() => {
  store = {
    'indexing.silencedDrives': '[]',
    'indexing.firstStaleDialogShown': false,
  }
})

describe('silenced drives', () => {
  it('starts empty', () => {
    expect(getSilencedDrives()).toEqual([])
    expect(hasSilencedDrives()).toBe(false)
  })

  it('silences a drive idempotently', () => {
    silenceDrive('smb-a')
    silenceDrive('smb-a')
    expect(getSilencedDrives()).toEqual(['smb-a'])
    expect(isDriveSilenced('smb-a')).toBe(true)
    expect(isDriveSilenced('smb-b')).toBe(false)
    expect(hasSilencedDrives()).toBe(true)
  })

  it('clears all silences', () => {
    silenceDrive('smb-a')
    silenceDrive('smb-b')
    clearSilencedDrives()
    expect(getSilencedDrives()).toEqual([])
    expect(hasSilencedDrives()).toBe(false)
  })

  it('tolerates a corrupt stored value', () => {
    store['indexing.silencedDrives'] = 'not json'
    expect(getSilencedDrives()).toEqual([])
    // And a non-array JSON value.
    store['indexing.silencedDrives'] = '{"a":1}'
    expect(getSilencedDrives()).toEqual([])
  })

  it('drops non-string entries from a malformed array', () => {
    store['indexing.silencedDrives'] = '["smb-a", 42, null]'
    expect(getSilencedDrives()).toEqual(['smb-a'])
  })
})

describe('first stale dialog flag', () => {
  it('reads and writes the one-shot', () => {
    expect(hasShownFirstStaleDialog()).toBe(false)
    markFirstStaleDialogShown()
    expect(hasShownFirstStaleDialog()).toBe(true)
  })
})
