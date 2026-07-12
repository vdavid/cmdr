/**
 * The composer drop resolver's branch logic: an in-app LOCAL self-drag uses the recorded
 * identity paths; a VIRTUAL self-drag is dropped (its round-tripped paths mis-resolve); a
 * Finder drop uses the genuine payload paths. `installComposerDrop` is a no-op outside a
 * Tauri webview.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AttachmentRef } from '$lib/tauri-commands'
import type { SelfDragIdentity } from '$lib/file-explorer/drag/drag-drop'

const resolveMock = vi.fn<(paths: string[]) => Promise<AttachmentRef[]>>()
const state = { isSelf: false, identity: null as SelfDragIdentity | null }

vi.mock('$lib/tauri-commands', () => ({
  resolveAskCmdrAttachments: (paths: string[]) => resolveMock(paths),
}))
vi.mock('$lib/file-explorer/drag/drag-drop', () => ({
  getIsDraggingFromSelf: () => state.isSelf,
  getSelfDragIdentity: () => state.identity,
}))
vi.mock('$lib/file-explorer/drag/drag-position', () => ({
  toViewportPosition: (p: { x: number; y: number }) => p,
}))

import { installComposerDrop, resolveDroppedPaths } from './ask-cmdr-drop'

function identity(sourceVolumeId: string, sourcePaths: string[]): SelfDragIdentity {
  return { sourceVolumeId, sourcePaths, startedAt: 0 }
}

beforeEach(() => {
  resolveMock.mockReset()
  resolveMock.mockImplementation((paths) =>
    Promise.resolve(paths.map((path) => ({ path, kind: 'file' as const }))),
  )
  state.isSelf = false
  state.identity = null
})

describe('resolveDroppedPaths', () => {
  it('a local self-drag resolves the recorded identity paths (not the payload paths)', async () => {
    state.isSelf = true
    state.identity = identity('root', ['/Users/d/a', '/Users/d/b'])
    const refs = await resolveDroppedPaths(['/wrong/payload/path'])
    expect(resolveMock).toHaveBeenCalledWith(['/Users/d/a', '/Users/d/b'])
    expect(refs.map((r) => r.path)).toEqual(['/Users/d/a', '/Users/d/b'])
  })

  it('a virtual-volume self-drag is dropped (paths would mis-resolve)', async () => {
    state.isSelf = true
    state.identity = identity('smb-share', ['/photos/sunset.jpg'])
    const refs = await resolveDroppedPaths(['/photos/sunset.jpg'])
    expect(resolveMock).not.toHaveBeenCalled()
    expect(refs).toEqual([])
  })

  it('a Finder (external) drop uses the genuine payload paths', async () => {
    state.isSelf = false
    const refs = await resolveDroppedPaths(['/Users/d/from-finder.pdf'])
    expect(resolveMock).toHaveBeenCalledWith(['/Users/d/from-finder.pdf'])
    expect(refs.map((r) => r.path)).toEqual(['/Users/d/from-finder.pdf'])
  })

  it('an empty external drop resolves to nothing', async () => {
    const refs = await resolveDroppedPaths([])
    expect(resolveMock).not.toHaveBeenCalled()
    expect(refs).toEqual([])
  })
})

describe('installComposerDrop', () => {
  it('is a no-op (returns a callable unlisten) outside a Tauri webview', async () => {
    const unlisten = await installComposerDrop(
      () => null,
      () => {},
      () => {},
    )
    expect(typeof unlisten).toBe('function')
    expect(() => { unlisten(); }).not.toThrow()
  })
})
