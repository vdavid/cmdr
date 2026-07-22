/**
 * Tier 3 a11y tests for `ImageIndexDriveBadge.svelte`: the labeled, non-focusable
 * image-index status dot must have no axe violations in each state (off / indexing /
 * done), and must render nothing when the drive has no qualifying images.
 * Mirrors `DriveIndexBadge.a11y.test.ts`.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import type { MediaIndexVolumeState } from '$lib/tauri-commands'
import type { VolumeEnrichActivity } from '$lib/indexing/media-enrich-state.svelte'

// The dot reads the master toggle and this volume's live enrichment activity; mock both
// so we can drive each visible state deterministically.
let masterEnabled = true
let activity: VolumeEnrichActivity | undefined
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getMediaIndexEnabled: () => masterEnabled,
}))
vi.mock('$lib/indexing/media-enrich-state.svelte', () => ({
  getVolumeEnrichActivity: () => activity,
}))

import ImageIndexDriveBadge from './ImageIndexDriveBadge.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

/** A complete `MediaIndexVolumeState` with `enabled` and count overrides. */
function makeState(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
  return {
    enabled: true,
    indexing: false,
    enrichedCount: 0,
    qualifyingCount: 50,
    networkOptIn: false,
    alwaysIndexed: false,
    paused: false,
    waitingForImportance: false,
    coveredQualifyingCount: 50,
    keptCount: null,
    ...overrides,
  }
}

async function mountBadge(volumeState: MediaIndexVolumeState) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ImageIndexDriveBadge, {
    target,
    props: { volumeId: 'vol-test', volumeState },
  })
  await tick()
  return target
}

beforeEach(() => {
  masterEnabled = true
  activity = undefined
})

describe('ImageIndexDriveBadge a11y', () => {
  it('the gray (off) dot has no violations', async () => {
    masterEnabled = false
    const target = await mountBadge(makeState())
    expect(target.querySelector('.image-index-drive-badge-off')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the yellow (indexing) dot has no violations', async () => {
    const target = await mountBadge(makeState({ enrichedCount: 12 }))
    expect(target.querySelector('.image-index-drive-badge-indexing')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the green (done) dot has no violations', async () => {
    const target = await mountBadge(makeState({ enrichedCount: 50 }))
    expect(target.querySelector('.image-index-drive-badge-done')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('renders nothing when the drive has no qualifying images', async () => {
    const target = await mountBadge(makeState({ qualifyingCount: 0, coveredQualifyingCount: 0 }))
    expect(target.querySelector('.image-index-drive-badge')).toBeNull()
  })
})
