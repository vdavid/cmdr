/**
 * Tier 3 a11y + behavior tests for `MediaIndexScope.svelte` (which folders image
 * indexing may cover).
 *
 * The load-bearing assertion is the slider's visibility: it exists only in the automatic
 * scope, because in the narrow one it has no effect at all and showing it would promise a
 * control that does nothing.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let scope = 'chosen'
const setSetting = vi.fn<(id: string, value: unknown) => void>()

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => (id === 'mediaIndex.scope' ? scope : 0),
  setSetting: (id: string, value: unknown) => {
    setSetting(id, value)
  },
  getSettingDefinition: () => ({
    label: 'Which folders to index',
    constraints: {
      options: [
        { value: 'chosen', label: 'Only folders I choose' },
        { value: 'importance', label: 'Automatically, by folder importance' },
      ],
    },
  }),
  onSpecificSettingChange: () => () => {},
}))

// `vi.hoisted` so the mock factory below (hoisted above this file's statements) sees it.
const { reclaimPreview } = vi.hoisted(() => ({
  reclaimPreview: vi.fn(() =>
    Promise.resolve({ totalStored: 0, coveredStored: 0, doomedCount: 0, estimatedBytes: 0, pending: false }),
  ),
}))

// The slider and reclaim children are real (these tests assert WHETHER they render, not
// what — each has its own suite), so stub the IPC and stores they reach for.
vi.mock('$lib/tauri-commands', () => ({
  mediaIndexCoveredCount: () => Promise.resolve({ folders: 0, images: 0, pending: false }),
  mediaIndexVolumeState: () => Promise.resolve(null),
  mediaIndexReclaimPreview: () => reclaimPreview(),
}))

vi.mock('$lib/media-index/enabled-volumes', () => ({ getEnabledMediaIndexVolumeIds: () => ['root'] }))
vi.mock('$lib/media-index/network-volume-prefs', () => ({ getNetworkOptInVolumes: () => [] }))
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))
vi.mock('$lib/indexing', () => ({ ROOT_VOLUME_ID: 'root' }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

const { default: MediaIndexScope } = await import('./MediaIndexScope.svelte')

function mountScope(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexScope, { target })
  flushSync()
  return target
}

describe('MediaIndexScope', () => {
  beforeEach(() => {
    scope = 'chosen'
  })

  afterEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('offers both scopes and hides the slider in the narrow one', async () => {
    const target = mountScope()
    const text = target.textContent
    expect(text).toContain('Only folders I choose')
    expect(text).toContain('Automatically, by folder importance')
    expect(target.querySelector('.mi-slider')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('keeps the reclaim offer reachable in the narrow scope', async () => {
    // The reclaim offer normally rides inside the slider. Narrowing is exactly when there
    // are leftover rows to free, so losing the offer with the slider would strand the disk
    // space; this component hosts its own instance instead.
    const target = mountScope()
    await vi.waitFor(() => {
      expect(reclaimPreview).toHaveBeenCalled()
    })
    // And not twice: the automatic scope's instance lives inside the slider.
    expect(target.querySelectorAll('.mi-slider').length).toBe(0)
  })

  it('shows the importance slider in the automatic scope', async () => {
    scope = 'importance'
    const target = mountScope()
    expect(target.querySelector('.mi-slider')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('commits the picked scope', async () => {
    const target = mountScope()
    const automatic = target.querySelector('input[value="importance"]') as HTMLInputElement
    automatic.click()
    await vi.waitFor(() => {
      expect(setSetting).toHaveBeenCalledWith('mediaIndex.scope', 'importance')
    })
  })
})
