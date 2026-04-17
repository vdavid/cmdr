/**
 * Tier 3 a11y tests for `AboutWindow.svelte`.
 *
 * About dialog shows app name, version, license info, and a few
 * external links. The license description varies with cached license
 * status (personal/commercial/expired). Tests cover the three
 * meaningful variants.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import AboutWindow from './AboutWindow.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let mockCachedStatus: unknown = null

vi.mock('./licensing-store.svelte', () => ({
  getCachedStatus: () => mockCachedStatus,
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

// `@tauri-apps/api/app` is dynamically imported on mount — stub it so
// getVersion() resolves without crashing.
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn(() => Promise.resolve('1.0.0')),
}))

describe('AboutWindow a11y', () => {
  beforeEach(() => {
    mockCachedStatus = null
  })

  it('personal license (no status) has no a11y violations', async () => {
    mockCachedStatus = null
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AboutWindow, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('commercial perpetual license has no a11y violations', async () => {
    mockCachedStatus = {
      type: 'commercial',
      licenseType: 'commercial_perpetual',
      organizationName: 'Acme Corp',
      expiresAt: null,
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AboutWindow, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('commercial subscription license has no a11y violations', async () => {
    mockCachedStatus = {
      type: 'commercial',
      licenseType: 'commercial_subscription',
      organizationName: 'Acme Corp',
      expiresAt: '2027-01-01',
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AboutWindow, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('expired license has no a11y violations', async () => {
    mockCachedStatus = {
      type: 'expired',
      expiredAt: '2025-12-01',
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AboutWindow, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
