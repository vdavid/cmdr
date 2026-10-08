/**
 * Tests for `path-sync.ts`, the two decisions a pane makes when its props move
 * under it. They pin the truth table that used to live inside two `$effect`s:
 * - a changed `initialPath` loads on a normal pane, and only syncs the path on a
 *   search-results pane (whose data comes from the snapshot store, not a listing),
 *   on a phone being opened, and on the network view, which owns its own data,
 * - an unchanged `initialPath` does nothing at all,
 * - a tab that just became reachable reloads only when the path stayed put (the
 *   "Open home folder" recovery changes the path, so the path branch takes it).
 */
import { describe, it, expect } from 'vitest'
import { resolveInitialPathAction, shouldReloadAfterReachable } from './path-sync'

const base = {
  initialPath: '/dir',
  currentPath: '/dir',
  isSearchResultsView: false,
  isNetworkView: false,
  deviceIsConnecting: false,
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

  it('only syncs the path on a search-results pane, which has no listing to load', () => {
    expect(
      resolveInitialPathAction({ ...base, isSearchResultsView: true, initialPath: 'search-results://sr-2' }),
    ).toEqual({ kind: 'sync-path', path: 'search-results://sr-2' })
  })

  it('does nothing on a search-results pane whose path is unchanged', () => {
    expect(resolveInitialPathAction({ ...base, isSearchResultsView: true })).toEqual({ kind: 'none' })
  })

  it('only syncs the path while a phone is being opened, so the dial is not doubled', () => {
    // ❗ A `loadDirectory` here reaches `resolve_path_to_volume`, which dials the
    // same phone again under the backend's own attempt id — and the pane's Cancel
    // aims at the OTHER one.
    expect(
      resolveInitialPathAction({
        ...base,
        deviceIsConnecting: true,
        initialPath: 'adb://R58M12345/sdcard',
      }),
    ).toEqual({ kind: 'sync-path', path: 'adb://R58M12345/sdcard' })
  })

  /**
   * ❗ The network view owns its own data, so nothing LOADS, but the path still
   * commits: left as the previous volume's, the header read "Servers ▸ /Volumes/public"
   * after a share, and anything reading the pane's path got a folder it isn't on
   * (QA 2026-09-25).
   */
  it('commits the network view’s path without loading a listing', () => {
    expect(
      resolveInitialPathAction({
        ...base,
        isNetworkView: true,
        currentPath: '/Volumes/public',
        initialPath: 'smb://',
      }),
    ).toEqual({ kind: 'sync-path', path: 'smb://' })
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
