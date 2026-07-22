/**
 * The event-seeded preview's guarantees, which are all about honesty:
 *
 * - it emits the REAL `index-freshness-changed` event rather than poking the
 *   dialog open, so the shipping trigger path is what runs;
 * - it names a drive that's actually in the volume store, because the dialog
 *   falls back to printing the raw volume id for one that isn't;
 * - it clears the one-shot on EVERY trigger, or the row would work once per
 *   machine (the dialog stamps the flag the moment it shows);
 * - with no drive to name it opens nothing and says so, rather than showing a
 *   plausible-looking screen with a synthetic id in the body copy.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { VolumeInfo } from '$lib/file-explorer/types'

let settings: Record<string, unknown>
let volumes: VolumeInfo[]
const emitted: { volumeId: string; freshness: string }[] = []
const toasts: string[] = []

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => settings[id],
  setSetting: (id: string, value: unknown) => {
    settings[id] = value
  },
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => volumes,
}))

// The real wrapper would round-trip through the Tauri backend; what matters
// here is the payload it's handed. The listeners are for `drive-index-manager`,
// which this module pulls in for `isDriveRow`.
vi.mock('$lib/tauri-commands/indexing', () => ({
  emitIndexFreshnessChanged: (payload: { volumeId: string; freshness: string }) => {
    emitted.push(payload)
    return Promise.resolve()
  },
  getVolumeIndexStatusById: vi.fn(() => Promise.resolve({ status: 'ok', data: null })),
  onIndexFreshnessChanged: vi.fn(() => Promise.resolve(() => {})),
  onIndexScanStarted: vi.fn(() => Promise.resolve(() => {})),
  onIndexScanComplete: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/ui/toast/toast-store.svelte', () => ({
  addToast: (content: string) => {
    toasts.push(content)
    return 'toast-id'
  },
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

// Deliberately NOT mocked: `drive-index-prefs` (over the mocked settings store,
// so the reset provably clears what the dialog reads) and `gallery-state`.
import { hasShownFirstStaleDialog, markFirstStaleDialogShown } from '$lib/indexing/drive-index-prefs'
import { isGalleryDialogOpen, openGalleryDialog, closeGalleryDialog } from './gallery-state.svelte'
import { openStaleDrivePreview } from './stale-drive-preview'

function volume(overrides: Partial<VolumeInfo> & { id: string }): VolumeInfo {
  return {
    name: `Volume ${overrides.id}`,
    path: `/Volumes/${overrides.id}`,
    category: 'attached_volume',
    isEjectable: true,
    ...overrides,
  }
}

beforeEach(() => {
  settings = { 'indexing.staleNotify': true, 'indexing.firstStaleDialogShown': false }
  volumes = [volume({ id: 'root', name: 'Macintosh HD', category: 'main_volume', isEjectable: false })]
  emitted.length = 0
  toasts.length = 0
  closeGalleryDialog()
})

describe('openStaleDrivePreview', () => {
  it('emits the real stale event for a drive that’s in the volume store', async () => {
    volumes.push(volume({ id: 'disk4s1', name: 'Field recordings' }))

    const outcome = await openStaleDrivePreview('default')

    expect(outcome).toEqual({ kind: 'emitted', volumeId: 'disk4s1' })
    expect(emitted).toEqual([{ volumeId: 'disk4s1', freshness: 'stale' }])
    expect(toasts).toEqual([])
  })

  it('turns the notify setting back on, since the dialog’s own button turns it off', async () => {
    volumes.push(volume({ id: 'disk4s1' }))
    settings['indexing.staleNotify'] = false

    await openStaleDrivePreview('default')

    expect(settings['indexing.staleNotify']).toBe(true)
    expect(emitted).toHaveLength(1)
  })

  it('clears the one-shot before every trigger, so the row repeats', async () => {
    volumes.push(volume({ id: 'disk4s1' }))
    // Stands in for the dialog having already fired once on this machine.
    markFirstStaleDialogShown()

    await openStaleDrivePreview('default')
    expect(hasShownFirstStaleDialog()).toBe(false)

    // And again after the dialog stamped it a second time.
    markFirstStaleDialogShown()
    await openStaleDrivePreview('default')
    expect(hasShownFirstStaleDialog()).toBe(false)
    expect(emitted).toHaveLength(2)
  })

  it('opens nothing and says why when no drive can be named', async () => {
    const outcome = await openStaleDrivePreview('default')

    expect(outcome).toEqual({ kind: 'no-drive' })
    expect(emitted).toEqual([])
    expect(toasts).toHaveLength(1)
    // Nothing real is written either: a preview that can't run shouldn't leave
    // the machine's settings changed.
    expect(settings['indexing.firstStaleDialogShown']).toBe(false)
  })

  it('never names a volume the shipping dialog couldn’t name', async () => {
    volumes.push(
      volume({ id: 'home', name: 'Home', category: 'favorite' }),
      // Journaled local storage: a cloud folder's index can't go stale, so
      // naming one would put a scenario on screen the app can't produce.
      volume({ id: 'cloud-dropbox', name: 'Dropbox', category: 'cloud_drive' }),
      // The synthetic switcher rows, and a mounted .dmg we never index.
      volume({ id: 'network', name: 'Network', category: 'network' }),
      volume({ id: 'search-results', name: 'Search results', category: 'network' }),
      volume({ id: 'disk9s1', name: 'Installer', isDiskImage: true }),
    )

    expect(await openStaleDrivePreview('default')).toEqual({ kind: 'no-drive' })
    expect(emitted).toEqual([])
  })

  it('names an MTP device when no disk or share is attached', async () => {
    volumes.push(volume({ id: 'mtp-pixel-9', name: 'Pixel 9', category: 'mobile_device' }))

    expect(await openStaleDrivePreview('default')).toEqual({ kind: 'emitted', volumeId: 'mtp-pixel-9' })
  })

  it('does nothing for a state id the row doesn’t advertise', async () => {
    volumes.push(volume({ id: 'disk4s1' }))

    expect(await openStaleDrivePreview('no-such-state')).toEqual({ kind: 'unknown-state' })
    expect(emitted).toEqual([])
    expect(toasts).toEqual([])
  })

  it('drops whatever the gallery was previewing, so nothing sits behind it', async () => {
    volumes.push(volume({ id: 'disk4s1' }))
    openGalleryDialog('alert', 'short')

    await openStaleDrivePreview('default')

    expect(isGalleryDialogOpen()).toBe(false)
  })
})
