/**
 * Tests for `path-sync.ts`, the two decisions a pane makes when its props move
 * under it. They pin the truth table that used to live inside two `$effect`s:
 * - an MTP device finishing its connection (device-only id → storage id) always
 *   loads, and wins over the `initialPath` branch,
 * - a changed `initialPath` loads on a normal pane, syncs the path only on a
 *   device-only MTP pane and on a search-results pane (whose data comes from the
 *   snapshot store, not a listing), and does nothing on the network view,
 *   which owns its own data,
 * - an unchanged `initialPath` does nothing at all,
 * - a tab that just became reachable reloads only when the path stayed put (the
 *   "Open home folder" recovery changes the path, so the path branch takes it).
 */
import { describe, it, expect } from 'vitest'
import { resolveInitialPathAction, shouldReloadAfterReachable } from './path-sync'

const base = {
  initialPath: '/dir',
  currentPath: '/dir',
  prevVolumeId: 'root',
  volumeId: 'root',
  isSearchResultsView: false,
  isNetworkView: false,
  isMtpDeviceOnly: false,
}

describe('resolveInitialPathAction', () => {
  it('does nothing when nothing moved', () => {
    expect(resolveInitialPathAction(base)).toEqual({ kind: 'none' })
  })

  it('loads a changed path on a normal pane', () => {
    expect(resolveInitialPathAction({ ...base, initialPath: '/elsewhere' })).toEqual({
      kind: 'load',
      path: '/elsewhere',
    })
  })

  it('loads when an MTP device finishes connecting, even at the same path', () => {
    expect(
      resolveInitialPathAction({ ...base, prevVolumeId: 'mtp-2097152', volumeId: 'mtp-2097152:65537' }),
    ).toEqual({ kind: 'mtp-connected', path: '/dir' })
  })

  it('lets the MTP connection win over a simultaneous path change', () => {
    expect(
      resolveInitialPathAction({
        ...base,
        initialPath: '/DCIM',
        prevVolumeId: 'mtp-2097152',
        volumeId: 'mtp-2097152:65537',
      }),
    ).toEqual({ kind: 'mtp-connected', path: '/DCIM' })
  })

  it('only syncs the path on a search-results pane, which has no listing to load', () => {
    expect(
      resolveInitialPathAction({ ...base, isSearchResultsView: true, initialPath: 'search-results://sr-2' }),
    ).toEqual({ kind: 'sync-path', path: 'search-results://sr-2' })
  })

  it('does nothing on a search-results pane whose path is unchanged', () => {
    expect(resolveInitialPathAction({ ...base, isSearchResultsView: true })).toEqual({ kind: 'none' })
  })

  it('only syncs the path on a device-only MTP pane, which needs connecting first', () => {
    expect(
      resolveInitialPathAction({ ...base, isMtpDeviceOnly: true, volumeId: 'mtp-2097152', initialPath: '/DCIM' }),
    ).toEqual({ kind: 'sync-path', path: '/DCIM' })
  })

  it('does nothing on the network view, which owns its own data', () => {
    expect(resolveInitialPathAction({ ...base, isNetworkView: true, initialPath: '/elsewhere' })).toEqual({
      kind: 'none',
    })
  })

  it('treats a plain volume switch as an ordinary path change', () => {
    expect(
      resolveInitialPathAction({ ...base, prevVolumeId: 'root', volumeId: 'ext', initialPath: '/Volumes/Ext' }),
    ).toEqual({ kind: 'load', path: '/Volumes/Ext' })
  })

  it('ignores an MTP id that was already connected', () => {
    expect(
      resolveInitialPathAction({ ...base, prevVolumeId: 'mtp-1:5', volumeId: 'mtp-1:5' }),
    ).toEqual({ kind: 'none' })
  })
})

describe('shouldReloadAfterReachable', () => {
  it('reloads when a retry made the volume reachable at the same path', () => {
    expect(
      shouldReloadAfterReachable({
        prevUnreachable: { originalPath: '/dir', retrying: true },
        unreachable: null,
        initialPath: '/dir',
        currentPath: '/dir',
      }),
    ).toBe(true)
  })

  it('leaves the reload to the path branch when the recovery changed the path', () => {
    expect(
      shouldReloadAfterReachable({
        prevUnreachable: { originalPath: '/gone', retrying: true },
        unreachable: null,
        initialPath: '/Users/test',
        currentPath: '/gone',
      }),
    ).toBe(false)
  })

  it('does nothing while the tab is still unreachable', () => {
    expect(
      shouldReloadAfterReachable({
        prevUnreachable: { originalPath: '/dir', retrying: false },
        unreachable: { originalPath: '/dir', retrying: true },
        initialPath: '/dir',
        currentPath: '/dir',
      }),
    ).toBe(false)
  })

  it('does nothing for a tab that was never unreachable', () => {
    expect(
      shouldReloadAfterReachable({
        prevUnreachable: null,
        unreachable: null,
        initialPath: '/dir',
        currentPath: '/dir',
      }),
    ).toBe(false)
  })
})
