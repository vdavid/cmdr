/**
 * Tests for the drive index manager. It owns FRESHNESS only now (the dot color +
 * last-scan facts for the menu/footer): live scan progress moved to `index-state`
 * (the single live-activity source). So we verify it refetches a volume's status
 * on the indexing events it subscribes to, and that `isDriveRow` gates which rows
 * carry a badge at all.
 *
 * Each `on*` listener's callback is captured at subscribe time so a test can
 * drive it synchronously, simulating a backend event.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type {
  IndexScanStartedEvent,
  IndexScanCompleteEvent,
  IndexFreshnessChangedEvent,
  VolumeIndexStatus,
} from '$lib/ipc/bindings'

// Captured callbacks, one per event kind. The manager registers exactly one
// listener per kind in its constructor.
let onStarted: ((p: IndexScanStartedEvent) => void) | undefined
let onComplete: ((p: IndexScanCompleteEvent) => void) | undefined
let onFreshness: ((p: IndexFreshnessChangedEvent) => void) | undefined

const noopUnlisten = vi.fn()

vi.mock('$lib/tauri-commands/indexing', () => ({
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
  getVolumeIndexStatusById,
}))

// The manager refetches status on each event; stub the IPC to return a status so
// the refetch populates `statusMap`. `vi.hoisted` so the mock factory (hoisted to
// the top of the file) can reference the spy.
const { getVolumeIndexStatusById } = vi.hoisted(() => ({ getVolumeIndexStatusById: vi.fn() }))
vi.mock('$lib/ipc/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/ipc/bindings')>()
  return {
    ...actual,
    commands: { ...actual.commands, getVolumeIndexStatusById },
  }
})

import { createDriveIndexManager, isDriveRow } from './drive-index-manager.svelte'
import type { VolumeInfo } from '../types'

function status(volumeId: string, freshness: VolumeIndexStatus['freshness']): VolumeIndexStatus {
  return { volumeId, enabled: true, freshness, scanCompletedAt: null, scanDurationMs: null }
}

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

/** Build a manager and wait a microtask so the async `on*` registrations resolve. */
async function makeManager() {
  const mgr = createDriveIndexManager()
  await Promise.resolve()
  await Promise.resolve()
  return mgr
}

beforeEach(() => {
  onStarted = onComplete = onFreshness = undefined
  vi.clearAllMocks()
  getVolumeIndexStatusById.mockResolvedValue({ status: 'error', error: 'stubbed' })
})

describe('drive index manager — freshness status', () => {
  it('refetches and stores a volume status on a freshness change', async () => {
    getVolumeIndexStatusById.mockResolvedValue({ status: 'ok', data: status('smb-a', 'stale') })
    const mgr = await makeManager()

    onFreshness?.({ volumeId: 'smb-a', freshness: 'stale' })
    await Promise.resolve()
    await Promise.resolve()

    expect(getVolumeIndexStatusById).toHaveBeenCalledWith('smb-a')
    expect(mgr.statusMap.get('smb-a')?.freshness).toBe('stale')
  })

  it('refetches on scan start and scan complete (keeps the dot + footer facts in sync)', async () => {
    const mgr = await makeManager()
    getVolumeIndexStatusById.mockResolvedValue({ status: 'ok', data: status('smb-a', 'scanning') })
    onStarted?.({ volumeId: 'smb-a', priorTotalEntries: null, priorScanDurationMs: null, volumeUsedBytes: null })
    await Promise.resolve()
    await Promise.resolve()
    expect(mgr.statusMap.get('smb-a')?.freshness).toBe('scanning')

    getVolumeIndexStatusById.mockResolvedValue({ status: 'ok', data: status('smb-a', 'fresh') })
    onComplete?.({ volumeId: 'smb-a', totalEntries: 100, totalDirs: 10, durationMs: 5_000 })
    await Promise.resolve()
    await Promise.resolve()
    expect(mgr.statusMap.get('smb-a')?.freshness).toBe('fresh')
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
