/**
 * en-US parity net for the file-explorer message catalog.
 *
 * Pins the EXACT English the file-explorer area produced BEFORE it moved into
 * the message catalog, so the reviewer can trust "current users see no change".
 * Two layers:
 *
 * 1. A bulk check that EVERY `fileExplorer.*` key whose value is a plain string
 *    (no ICU placeholder / plural / select) renders, under en-US, byte-for-byte
 *    equal to its catalog value with the doubled apostrophes (`''`) collapsed to
 *    a single `'`. This catches a missed apostrophe-double or a stray ICU
 *    metacharacter across the whole area without hand-listing every static key.
 * 2. Golden assertions for the interpolating / plural / select keys, where the
 *    rendered output can't be derived from the template by a simple transform.
 *    Each golden is the historic English the literal produced.
 *
 * The locale is pinned via the `lib/intl` chokepoint (`_setLocaleForTests`), not
 * by mutating `Intl` globals. Mirrors the transfer pilot's
 * `file-operations/transfer/transfer-complete-toast.test.ts`.
 */
import { afterAll, beforeAll, describe, expect, it } from 'vitest'

import { _setLocaleForTests } from '$lib/intl/locale'
import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'
import fileExplorerCatalog from '$lib/intl/messages/en/fileExplorer.json'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

// True when a value uses an ICU feature that needs params to render (a `{name}`
// placeholder, a `plural`/`select`, or a rich-text `<tag>`); those are covered
// by the golden assertions below, not the bulk static check.
function isStatic(value: string): boolean {
  return !/[{<]/.test(value)
}

describe('en-US parity: static fileExplorer keys render to their catalog English', () => {
  const entries = Object.entries(fileExplorerCatalog as Record<string, unknown>).filter(
    ([key, value]) => !key.startsWith('@') && typeof value === 'string' && isStatic(value),
  ) as [MessageKey, string][]

  it('covers a meaningful number of static keys', () => {
    // Guard against the filter silently matching nothing (e.g. a JSON-shape change).
    expect(entries.length).toBeGreaterThan(50)
  })

  for (const [key, value] of entries) {
    it(key, () => {
      // The catalog doubles every apostrophe for ICU; the rendered string carries a single one.
      expect(tString(key)).toBe(value.replace(/''/g, "'"))
    })
  }
})

describe('en-US parity: interpolating / plural / select fileExplorer keys (golden)', () => {
  it('dead-wired rename-conflict copy', () => {
    expect(tString('fileExplorer.renameConflict.description', { name: 'report.pdf' })).toBe(
      '"report.pdf" already exists in this folder. What would you like to do?',
    )
    expect(tString('fileExplorer.renameConflict.yours', { name: 'report.pdf' })).toBe('report.pdf (yours)')
    expect(tString('fileExplorer.renameConflict.existing', { name: 'report.pdf' })).toBe('report.pdf (existing)')
  })

  it('dead-wired extension-change copy', () => {
    expect(tString('fileExplorer.extensionChange.description', { oldExt: 'txt', newExt: 'md' })).toBe(
      'Are you sure you want to change the extension from ".txt" to ".md"? Your file may open in a different app next time you open it.',
    )
    expect(tString('fileExplorer.extensionChange.keepOld', { oldExt: 'txt' })).toBe('Keep .txt')
    expect(tString('fileExplorer.extensionChange.useNew', { newExt: 'md' })).toBe('Use .md')
  })

  it('size-column placeholders keep their literal angle brackets (ICU-escaped < )', () => {
    expect(tString('fileExplorer.dirSize.dirPlaceholder')).toBe('<dir>')
    expect(tString('fileExplorer.dirSize.noPerms')).toBe('<no perms>')
  })

  it('navigation favorite tooltip (path + reorder, with a real newline)', () => {
    expect(tString('fileExplorer.navigation.favoriteTooltip', { path: '~/Documents', reorder: '⌥↑ / ⌥↓' })).toBe(
      '~/Documents\nDrag to reorder, or ⌥↑ / ⌥↓. Right-click to rename or remove.',
    )
  })

  it('navigation network volume names', () => {
    expect(tString('fileExplorer.navigation.networkVolume')).toBe('Network')
    expect(tString('fileExplorer.navigation.networkVolumeDisabled')).toBe('Network (disabled)')
  })

  it('navigation eject labels', () => {
    expect(tString('fileExplorer.navigation.ejectVolumeAriaLabel', { name: 'Macintosh HD' })).toBe('Eject Macintosh HD')
  })

  it('navigation USB speed line', () => {
    expect(tString('fileExplorer.navigation.usbSpeed', { label: 'USB 3.2 Gen 2', mbps: '1000' })).toBe(
      'USB 3.2 Gen 2 (Max. 1000 MB/s)',
    )
  })

  it('navigation saved-password dialog body keeps its apostrophes', () => {
    expect(tString('fileExplorer.navigation.useSavedPasswordMessage', { displayName: 'mynas' })).toBe(
      'Cmdr can reuse the password macOS already saved for "mynas". You\'ll see a system prompt asking to allow Keychain access. That\'s expected, so click Allow.',
    )
  })

  it('reused pane toasts the navigation layer now calls', () => {
    expect(tString('fileExplorer.pane.connectedDirectlyToast')).toBe('Connected directly for faster access')
    expect(tString('fileExplorer.pane.directConnectionFailedToast', { message: 'timed out' })).toBe(
      'Direct connection failed: timed out',
    )
    expect(tString('fileExplorer.pane.ejectFailedToast', { volumeName: 'Backup', message: 'busy' })).toBe(
      "Couldn't eject Backup: busy",
    )
  })
})
