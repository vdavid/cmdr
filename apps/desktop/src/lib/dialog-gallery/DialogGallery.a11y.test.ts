/**
 * Tier 3 a11y tests for `DialogGallery.svelte`.
 *
 * The harness itself renders no chrome: it either renders nothing, or renders a
 * real dialog with fixture props. Both are covered here, so a fixture that would
 * break the dialog it feeds (an empty title, say) can't land silently.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import DialogGallery from './DialogGallery.svelte'
import { alertFixtures } from './fixtures/alert'
import { closeGalleryDialog, openGalleryDialog } from './gallery-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

// The store is module-level, so a harness left mounted would react to the next
// test's writes and leave a stale dialog in the DOM for axe to scan.
let mounted: Record<string, unknown> | undefined

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = undefined
  closeGalleryDialog()
})

function mountGallery(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(DialogGallery, { target })
  return target
}

describe('DialogGallery a11y', () => {
  it('renders nothing, with no violations, while no preview is open', async () => {
    const target = mountGallery()
    await tick()
    await expectNoA11yViolations(target)
  })

  for (const stateId of Object.keys(alertFixtures)) {
    it(`alert / ${stateId} has no a11y violations`, async () => {
      openGalleryDialog('alert', stateId)
      const target = mountGallery()
      await tick()
      await expectNoA11yViolations(target)
    })
  }
})
