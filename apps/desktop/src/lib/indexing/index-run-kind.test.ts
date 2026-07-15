/**
 * Unit tests for `isNetworkIndexRun`, the predicate that keys the per-volume
 * checklist SHAPE on the volume's `category` (not `volumeId !== root`). This is
 * the whole point of the category switch: a non-root LOCAL drive (a USB stick /
 * SD card, category `attached_volume`) must get the LOCAL checklist — with the
 * Save-the-file-list and Catch-up steps — not the network one a `!== root` test
 * would have handed it.
 */
import { describe, it, expect } from 'vitest'
import type { LocationCategory } from '$lib/ipc/bindings'
import type { VolumeInfo } from '$lib/file-explorer/types'
import { ROOT_VOLUME_ID } from './index-state.svelte'
import { isNetworkIndexRun } from './index-run-kind'
import { deriveSteps } from './indexing-steps'

/** A minimal volume-list entry with the fields the predicate reads. */
function vol(id: string, category: LocationCategory): VolumeInfo {
  return { id, name: id, path: `/Volumes/${id}`, category, isEjectable: true }
}

describe('isNetworkIndexRun', () => {
  it('treats the boot disk (root) as local even before the volume list hydrates', () => {
    expect(isNetworkIndexRun(ROOT_VOLUME_ID, [])).toBe(false)
  })

  it('treats a non-root local drive (attached_volume USB/SD) as LOCAL', () => {
    const volumes = [vol('volumesnoname', 'attached_volume')]
    expect(isNetworkIndexRun('volumesnoname', volumes)).toBe(false)
  })

  it('treats the main volume and cloud drives as local', () => {
    const volumes = [vol('main', 'main_volume'), vol('dropbox', 'cloud_drive')]
    expect(isNetworkIndexRun('main', volumes)).toBe(false)
    expect(isNetworkIndexRun('dropbox', volumes)).toBe(false)
  })

  it('treats an SMB share (network) as network', () => {
    const volumes = [vol('smb-nas', 'network')]
    expect(isNetworkIndexRun('smb-nas', volumes)).toBe(true)
  })

  it('treats an MTP device (mobile_device) as network', () => {
    const volumes = [vol('mtp-phone', 'mobile_device')]
    expect(isNetworkIndexRun('mtp-phone', volumes)).toBe(true)
  })

  it('falls back to local when the volume is not in the list', () => {
    expect(isNetworkIndexRun('gone-mid-scan', [])).toBe(false)
  })
})

describe('category drives the derived checklist shape', () => {
  it('gives a non-root local drive the full local checklist (Save + Catch-up present)', () => {
    const volumes = [vol('volumesnoname', 'attached_volume')]
    const isNetwork = isNetworkIndexRun('volumesnoname', volumes)
    const steps = deriveSteps({
      runKind: isNetwork ? 'network' : 'local',
      phase: 'scanning',
      aggregationSubPhase: undefined,
    })
    const kinds = steps.map((s) => s.kind)
    expect(kinds).toEqual(['findFiles', 'saveFileList', 'computeFolderSizes', 'catchUp'])
  })

  it('gives a network drive the network checklist (no Save, no Catch-up)', () => {
    const volumes = [vol('smb-nas', 'network')]
    const isNetwork = isNetworkIndexRun('smb-nas', volumes)
    const steps = deriveSteps({
      runKind: isNetwork ? 'network' : 'local',
      phase: 'scanning',
      aggregationSubPhase: undefined,
    })
    expect(steps.map((s) => s.kind)).toEqual(['findFiles', 'computeFolderSizes'])
  })

  it('gives an MTP device the network checklist too', () => {
    const volumes = [vol('mtp-phone', 'mobile_device')]
    const isNetwork = isNetworkIndexRun('mtp-phone', volumes)
    const steps = deriveSteps({
      runKind: isNetwork ? 'network' : 'local',
      phase: 'scanning',
      aggregationSubPhase: undefined,
    })
    expect(steps.map((s) => s.kind)).toEqual(['findFiles', 'computeFolderSizes'])
  })
})
