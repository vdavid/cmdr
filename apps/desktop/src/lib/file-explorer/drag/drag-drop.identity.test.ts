/**
 * Unit tests for the self-drag IDENTITY lifecycle in `drag-drop.ts`: the
 * `{ sourceVolumeId, sourcePaths, startedAt }` record that lets the drop handler
 * build a transfer request from app state instead of the lossy pasteboard
 * round-trip. The actual `perform*Drag` recording (and the controller's
 * consumption of the record) is covered end-to-end in
 * `pane/drag-drop-controller.svelte.test.ts`; this file pins the record/get/clear
 * primitives and the `cancelDragTracking` clear so the lifecycle can't silently
 * regress.
 *
 * The heavy Tauri/path imports are stubbed so the pure module state is testable
 * without a running app.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('@tauri-apps/api/path', () => ({ tempDir: vi.fn(), join: vi.fn() }))
vi.mock('$lib/icon-cache', () => ({ getCachedIcon: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({
  startSelectionDrag: vi.fn(),
  startDragPaths: vi.fn(),
  prepareSelfDragOverlay: vi.fn(),
  clearSelfDragOverlay: vi.fn(),
  setSelfDragResolvedOperation: vi.fn(),
  getPathsAtIndices: vi.fn(),
}))
vi.mock('$lib/settings/settings-store', () => ({ getSetting: vi.fn(() => 5) }))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('./drag-image-renderer', () => ({ renderDragImage: vi.fn() }))

import { recordSelfDragIdentity, getSelfDragIdentity, clearSelfDragIdentity, cancelDragTracking } from './drag-drop'

describe('self-drag identity lifecycle', () => {
  beforeEach(() => {
    clearSelfDragIdentity()
  })

  it('starts with no recorded identity', () => {
    expect(getSelfDragIdentity()).toBeNull()
  })

  it('records the source volume id and paths verbatim (volume-relative for MTP/SMB)', () => {
    recordSelfDragIdentity('mtp-dev:65537', ['/photos/sunset.jpg', '/photos/moon.jpg'])
    const identity = getSelfDragIdentity()
    expect(identity).not.toBeNull()
    expect(identity?.sourceVolumeId).toBe('mtp-dev:65537')
    expect(identity?.sourcePaths).toEqual(['/photos/sunset.jpg', '/photos/moon.jpg'])
    expect(typeof identity?.startedAt).toBe('number')
  })

  it('records a local source volume id with absolute paths', () => {
    recordSelfDragIdentity('root', ['/Users/me/file.txt'])
    expect(getSelfDragIdentity()?.sourceVolumeId).toBe('root')
  })

  it('a later record overwrites the earlier one (one drag in flight at a time)', () => {
    recordSelfDragIdentity('root', ['/a.txt'])
    recordSelfDragIdentity('smb-server-share', ['/dir/b.txt'])
    expect(getSelfDragIdentity()?.sourceVolumeId).toBe('smb-server-share')
    expect(getSelfDragIdentity()?.sourcePaths).toEqual(['/dir/b.txt'])
  })

  it('clearSelfDragIdentity forgets the record', () => {
    recordSelfDragIdentity('root', ['/a.txt'])
    clearSelfDragIdentity()
    expect(getSelfDragIdentity()).toBeNull()
  })

  it('cancelDragTracking clears the recorded identity (ESC / mouseup-before-threshold termination)', () => {
    recordSelfDragIdentity('mtp-dev:65537', ['/photos/x.jpg'])
    cancelDragTracking()
    expect(getSelfDragIdentity()).toBeNull()
  })
})
