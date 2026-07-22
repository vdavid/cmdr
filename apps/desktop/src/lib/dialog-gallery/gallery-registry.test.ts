/**
 * The gallery's honesty invariants. The `dialog-gallery-coverage` Go check owns
 * "every registered dialog has a row"; these cover what a row must say once it's
 * there.
 */

import { describe, expect, it } from 'vitest'
import { DIALOG_GALLERY_ENTRIES, UNREGISTERED_OVERLAY_ENTRIES } from './gallery-registry'
import { SOFT_DIALOG_REGISTRY } from '$lib/ui/dialog-registry'

describe('dialog gallery registry', () => {
  it('gives every non-ready entry a non-empty reason', () => {
    const silent = DIALOG_GALLERY_ENTRIES.filter((entry) => entry.status !== 'ready' && !entry.reason.trim())
    expect(silent.map((entry) => entry.dialogId)).toEqual([])
  })

  it('gives every ready entry at least one state, and no non-ready entry any', () => {
    const readyWithoutStates = DIALOG_GALLERY_ENTRIES.filter(
      (entry) => entry.status === 'ready' && entry.states.length === 0,
    )
    expect(readyWithoutStates.map((entry) => entry.dialogId)).toEqual([])

    const blockedWithStates = DIALOG_GALLERY_ENTRIES.filter(
      (entry) => entry.status !== 'ready' && entry.states.length > 0,
    )
    expect(blockedWithStates.map((entry) => entry.dialogId)).toEqual([])
  })

  it('keeps state ids unique within an entry', () => {
    for (const entry of DIALOG_GALLERY_ENTRIES) {
      const ids = entry.states.map((state) => state.id)
      expect(new Set(ids).size, `duplicate state id in ${entry.dialogId}`).toBe(ids.length)
    }
  })

  it('lists each registered dialog exactly once', () => {
    const ids = DIALOG_GALLERY_ENTRIES.map((entry) => entry.dialogId)
    expect(new Set(ids).size).toBe(ids.length)
    expect(ids.length).toBe(SOFT_DIALOG_REGISTRY.length)
  })

  it('keeps unregistered overlays out of the soft dialog registry', () => {
    const registeredIds = new Set<string>(SOFT_DIALOG_REGISTRY.map((dialog) => dialog.id))
    const misfiled = UNREGISTERED_OVERLAY_ENTRIES.filter((overlay) => registeredIds.has(overlay.overlayId))
    expect(misfiled.map((overlay) => overlay.overlayId)).toEqual([])
    for (const overlay of UNREGISTERED_OVERLAY_ENTRIES) {
      expect(overlay.reason.trim(), `missing reason on ${overlay.overlayId}`).not.toBe('')
    }
  })
})
