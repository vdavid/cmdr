/**
 * Per-volume LIVE scan-progress tracking in the drive index manager: a settled
 * `index-scan-progress` event updates the right volume's count (keyed by
 * `volumeId`), a different volume's event doesn't bleed across, and the scan's
 * end (complete, or freshness flipping away from `scanning`) clears it.
 *
 * Each `on*` listener's callback is captured at subscribe time so a test can
 * drive it synchronously, simulating a backend event.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type {
  IndexScanProgressEvent,
  IndexScanStartedEvent,
  IndexScanCompleteEvent,
  IndexFreshnessChangedEvent,
} from '$lib/ipc/bindings'

// Captured callbacks, one per event kind. The manager registers exactly one
// listener per kind in its constructor.
let onProgress: ((p: IndexScanProgressEvent) => void) | undefined
let onStarted: ((p: IndexScanStartedEvent) => void) | undefined
let onComplete: ((p: IndexScanCompleteEvent) => void) | undefined
let onFreshness: ((p: IndexFreshnessChangedEvent) => void) | undefined

const noopUnlisten = vi.fn()

vi.mock('$lib/tauri-commands/indexing', () => ({
  onIndexScanProgress: (cb: (p: IndexScanProgressEvent) => void) => {
    onProgress = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexScanStarted: (cb: (p: IndexScanStartedEvent) => void) => {
    onStarted = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexScanComplete: (cb: (p: IndexScanCompleteEvent) => void) => {
    onComplete = cb
    return Promise.resolve(noopUnlisten)
  },
  onIndexFreshnessChanged: (cb: (p: IndexFreshnessChangedEvent) => void) => {
    onFreshness = cb
    return Promise.resolve(noopUnlisten)
  },
}))

// The manager also refetches status on each event; stub the IPC to a no-op so
// those calls don't reach a real backend.
vi.mock('$lib/ipc/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/ipc/bindings')>()
  return {
    ...actual,
    commands: {
      ...actual.commands,
      getVolumeIndexStatusById: vi.fn().mockResolvedValue({ status: 'error', error: 'stubbed' }),
    },
  }
})

import { createDriveIndexManager, isDriveRow } from './drive-index-manager.svelte'
import type { VolumeInfo } from '../types'

/** Minimal `VolumeInfo` for `isDriveRow`, which reads only `category`, `id`, and `isDiskImage`. */
function vol(over: Partial<VolumeInfo>): VolumeInfo {
  return {
    id: 'vol-1',
    name: 'Volume',
    path: '/Volumes/Volume',
    category: 'attached_volume',
    icon: null,
    isEjectable: false,
    fsType: 'apfs',
    supportsTrash: true,
    isReadOnly: false,
    isDiskImage: false,
    smbConnectionState: null,
    usbSpeed: null,
    ...over,
  } as VolumeInfo
}

/**
 * Build a manager and wait a microtask so the async `on*` registrations resolve
 * and populate the captured callbacks.
 */
async function makeManager() {
  const mgr = createDriveIndexManager()
  await Promise.resolve()
  await Promise.resolve()
  return mgr
}

beforeEach(() => {
  onProgress = onStarted = onComplete = onFreshness = undefined
  vi.clearAllMocks()
})

function progress(volumeId: string, entriesScanned: number): IndexScanProgressEvent {
  return { volumeId, entriesScanned, dirsFound: 0, bytesScanned: 0 }
}

describe('drive index manager — per-volume scan progress', () => {
  it('records a progress event under the right volume id', async () => {
    const mgr = await makeManager()
    onProgress?.(progress('smb-a', 12_345))

    expect(mgr.getScanProgress('smb-a')?.entriesScanned).toBe(12_345)
  })

  it("does not bleed one volume's progress into another", async () => {
    const mgr = await makeManager()
    onProgress?.(progress('smb-a', 100))
    onProgress?.(progress('mtp-phone', 7))

    expect(mgr.getScanProgress('smb-a')?.entriesScanned).toBe(100)
    expect(mgr.getScanProgress('mtp-phone')?.entriesScanned).toBe(7)
  })

  it('returns undefined for a volume that never reported progress', async () => {
    const mgr = await makeManager()
    expect(mgr.getScanProgress('smb-a')).toBeUndefined()
  })

  it('seeds a start time on scan-started and keeps it across progress ticks', async () => {
    const mgr = await makeManager()
    onStarted?.({ volumeId: 'smb-a', priorTotalEntries: null, priorScanDurationMs: null, volumeUsedBytes: null })
    const started = mgr.getScanProgress('smb-a')
    expect(started?.entriesScanned).toBe(0)
    expect(started?.scanStartedAt).toBeGreaterThan(0)

    const startedAt = started?.scanStartedAt
    onProgress?.(progress('smb-a', 50))
    expect(mgr.getScanProgress('smb-a')?.scanStartedAt).toBe(startedAt)
    expect(mgr.getScanProgress('smb-a')?.entriesScanned).toBe(50)
  })

  it('clears progress for the right volume on scan-complete only', async () => {
    const mgr = await makeManager()
    onProgress?.(progress('smb-a', 100))
    onProgress?.(progress('mtp-phone', 7))

    onComplete?.({ volumeId: 'smb-a', totalEntries: 100, totalDirs: 10, durationMs: 5_000 })

    expect(mgr.getScanProgress('smb-a')).toBeUndefined()
    expect(mgr.getScanProgress('mtp-phone')?.entriesScanned).toBe(7)
  })

  it('clears progress when freshness flips away from scanning, but not while still scanning', async () => {
    const mgr = await makeManager()
    onProgress?.(progress('smb-a', 100))

    onFreshness?.({ volumeId: 'smb-a', freshness: 'scanning' })
    expect(mgr.getScanProgress('smb-a')?.entriesScanned).toBe(100)

    onFreshness?.({ volumeId: 'smb-a', freshness: 'fresh' })
    expect(mgr.getScanProgress('smb-a')).toBeUndefined()
  })
})

describe('isDriveRow — index-affordance eligibility', () => {
  it('treats a regular attached volume as a drive row', () => {
    expect(isDriveRow(vol({ category: 'attached_volume' }))).toBe(true)
  })

  it('excludes mounted disk images (no index badge, prompt, or status fetch)', () => {
    expect(isDriveRow(vol({ category: 'attached_volume', isDiskImage: true }))).toBe(false)
  })

  it('still excludes favorites and the synthetic network / search-results rows', () => {
    expect(isDriveRow(vol({ category: 'favorite' }))).toBe(false)
    expect(isDriveRow(vol({ id: 'network' }))).toBe(false)
    expect(isDriveRow(vol({ id: 'search-results' }))).toBe(false)
  })
})
