/**
 * Base-locale (en) parity net for the low-disk-space i18n migration.
 *
 * The persistent warning toast and the macOS notification copy moved from
 * hardcoded English into the `lowDiskSpace.*` catalog. Behavior-preserving MOVE:
 * every rendered en string must be byte-identical to the pre-migration copy. A
 * future copy edit lands in the catalog AND here together, never silently.
 */

import { beforeEach, describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { withPlatform } from '#lib/platform/test-platform.js'

// These goldens pin the English a Mac shows, so they read the macOS branch of platform-variant copy.
beforeEach(() => withPlatform('macos'))

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('lowDiskSpace catalog parity (en)', () => {
  it('resolves the in-app toast copy', () => {
    expect(tString('lowDiskSpace.toast.message', { freeText: '4.2 GB', percentText: '3.1' })).toBe(
      'Your startup disk is running low on space: 4.2 GB free (3.1%).',
    )
    expect(tString('lowDiskSpace.toast.disable')).toBe('Disable these notifications')
    expect(tString('lowDiskSpace.toast.closeTooltip')).toBe('Dismiss')
  })

  it('resolves the macOS notification copy', () => {
    expect(tString('lowDiskSpace.notification.title')).toBe('Low disk space')
    expect(tString('lowDiskSpace.notification.body', { freeText: '4.2 GB', percentText: '3.1' })).toBe(
      '4.2 GB free (3.1%) on your startup disk.',
    )
  })
})
