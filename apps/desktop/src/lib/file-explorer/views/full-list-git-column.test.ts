/**
 * The Git status column's data. The load itself is trivial; what these pin is the
 * teardown, because getting it wrong is invisible until it bites: a watcher listener
 * that outlives its directory stacks one more subscription per navigation, and a
 * load that lands after the pane moved on paints the previous folder's glyphs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FileEntry } from '../types'

const mocks = vi.hoisted(() => ({
  fetchStatusMap: vi.fn(),
  onGitStateChanged: vi.fn(),
  unlisten: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({ onGitStateChanged: mocks.onGitStateChanged }))
vi.mock('../git/status-column', () => ({ fetchStatusMap: mocks.fetchStatusMap }))

import { createGitStatusColumn } from './full-list-git-column.svelte'

/** The watcher callback the controller registered, so tests can fire an event. */
let emitGitStateChanged: (payload: { repoRoot: string }) => void

function file(path: string): FileEntry {
  return {
    name: path.split('/').pop() ?? '',
    path,
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'me',
    group: 'staff',
    iconId: 'icon',
    extendedMetadataLoaded: false,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  emitGitStateChanged = () => {}
  mocks.fetchStatusMap.mockResolvedValue(new Map([['src/a.ts', 'M']]))
  mocks.onGitStateChanged.mockImplementation((cb: (payload: { repoRoot: string }) => void) => {
    emitGitStateChanged = cb
    return Promise.resolve(mocks.unlisten)
  })
})

describe('watch', () => {
  it('loads the map for the directory on screen', async () => {
    const column = createGitStatusColumn()

    column.watch('/repo', '/repo/src')
    await vi.waitFor(() => {
      expect(column.statusFor(file('/repo/src/a.ts'))).toBe('M')
    })
    expect(mocks.fetchStatusMap).toHaveBeenCalledWith('/repo', '/repo/src')
  })

  it('clears the map and listens to nothing outside a worktree', async () => {
    const column = createGitStatusColumn()
    column.watch('/repo', '/repo/src')
    await vi.waitFor(() => {
      expect(column.statusFor(file('/repo/src/a.ts'))).toBe('M')
    })

    column.watch(null, '/elsewhere')

    expect(column.statusFor(file('/repo/src/a.ts'))).toBeNull()
    expect(mocks.fetchStatusMap).toHaveBeenCalledOnce()
  })

  it('reloads when the watcher reports a change in this repo', async () => {
    const column = createGitStatusColumn()
    column.watch('/repo', '/repo/src')
    await vi.waitFor(() => {
      expect(mocks.onGitStateChanged).toHaveBeenCalled()
    })

    emitGitStateChanged({ repoRoot: '/repo' })

    await vi.waitFor(() => {
      expect(mocks.fetchStatusMap).toHaveBeenCalledTimes(2)
    })
  })

  it('ignores a change in a different repo', async () => {
    const column = createGitStatusColumn()
    column.watch('/repo', '/repo/src')
    await vi.waitFor(() => {
      expect(mocks.onGitStateChanged).toHaveBeenCalled()
    })

    emitGitStateChanged({ repoRoot: '/other-repo' })
    await Promise.resolve()

    expect(mocks.fetchStatusMap).toHaveBeenCalledOnce()
  })

  it('removes the watcher listener on teardown', async () => {
    const column = createGitStatusColumn()
    const stop = column.watch('/repo', '/repo/src')
    await vi.waitFor(() => {
      expect(mocks.onGitStateChanged).toHaveBeenCalled()
    })

    stop()

    expect(mocks.unlisten).toHaveBeenCalledOnce()
  })

  it('drops a load that lands after teardown, so a stale folder cannot paint', async () => {
    let land: (map: Map<string, string>) => void = () => {}
    mocks.fetchStatusMap.mockReturnValue(
      new Promise<Map<string, string>>((resolve) => {
        land = resolve
      }),
    )
    const column = createGitStatusColumn()
    const stop = column.watch('/repo', '/repo/src')

    stop()
    land(new Map([['src/a.ts', 'M']]))
    await Promise.resolve()

    expect(column.statusFor(file('/repo/src/a.ts'))).toBeNull()
  })

  it('unlistens immediately when the registration resolves after teardown', async () => {
    const column = createGitStatusColumn()
    const stop = column.watch('/repo', '/repo/src')

    stop()
    await vi.waitFor(() => {
      expect(mocks.unlisten).toHaveBeenCalled()
    })
  })

  it('shows no glyphs when the load fails', async () => {
    mocks.fetchStatusMap.mockRejectedValue(new Error('not a repo'))
    const column = createGitStatusColumn()

    column.watch('/repo', '/repo/src')
    await Promise.resolve()

    expect(column.statusFor(file('/repo/src/a.ts'))).toBeNull()
  })
})

describe('statusFor', () => {
  beforeEach(() => {
    mocks.fetchStatusMap.mockResolvedValue(
      new Map([
        ['src/a.ts', 'M'],
        ['repo/nested.ts', 'A'],
      ]),
    )
  })

  it('keys by the path relative to the repo root', async () => {
    const column = createGitStatusColumn()
    column.watch('/home/repo', '/home/repo/src')
    await vi.waitFor(() => {
      expect(column.statusFor(file('/home/repo/src/a.ts'))).toBe('M')
    })
  })

  it('resolves a row whose path repeats the repo name below the root', async () => {
    // Would break on a naive "strip everything before the last `repo/`" scheme.
    const column = createGitStatusColumn()
    column.watch('/home/repo', '/home/repo/repo')
    await vi.waitFor(() => {
      expect(column.statusFor(file('/home/repo/repo/nested.ts'))).toBe('A')
    })
  })

  it('tolerates a repo root with a trailing slash', async () => {
    const column = createGitStatusColumn()
    column.watch('/home/repo/', '/home/repo/src')
    await vi.waitFor(() => {
      expect(column.statusFor(file('/home/repo/src/a.ts'))).toBe('M')
    })
  })

  it('is null for a clean row and for one outside the worktree', async () => {
    const column = createGitStatusColumn()
    column.watch('/home/repo', '/home/repo/src')
    await vi.waitFor(() => {
      expect(column.statusFor(file('/home/repo/src/a.ts'))).toBe('M')
    })

    expect(column.statusFor(file('/home/repo/src/clean.ts'))).toBeNull()
    expect(column.statusFor(file('/somewhere/else/a.ts'))).toBeNull()
  })

  it('is null before the map lands', () => {
    const column = createGitStatusColumn()

    expect(column.statusFor(file('/home/repo/src/a.ts'))).toBeNull()
  })
})
