/**
 * Tier 3 a11y tests for `VolumeBreadcrumb.svelte`.
 *
 * The volume selector breadcrumb + dropdown. Only the closed state is
 * audited here — the open dropdown uses lots of CSS positioning that
 * axe doesn't reason about correctly in jsdom. Volume-store and Tauri
 * IPC are stubbed.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: { id: 'root', path: '/' } })),
  upgradeToSmbVolume: vi.fn(() => Promise.resolve({ status: 'success' })),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'ext', name: 'External', path: '/Volumes/External', category: 'attached_volume', isEjectable: true },
  ],
  getVolumesTimedOut: () => false,
  isVolumesRefreshing: () => false,
  isVolumeRetryFailed: () => false,
  requestVolumeRefresh: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(() => 'toast-id'),
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: (n: number) => `${String(n)} B`,
}))

describe('VolumeBreadcrumb a11y', () => {
  it('closed breadcrumb (local volume) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(VolumeBreadcrumb, {
      target,
      props: {
        volumeId: 'root',
        currentPath: '/Users/test',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('closed breadcrumb (network virtual volume) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(VolumeBreadcrumb, {
      target,
      props: {
        volumeId: 'network',
        currentPath: 'smb://',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
