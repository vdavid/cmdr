/**
 * Panes, tabs, their history, and remembered paths following a saved server to
 * its new address (`server-place-moved`).
 *
 * ❗ The store patch lands BEFORE any pane moves: the event beats the debounced
 * republish, and `navigate()` looks the new id up in the volume list, which would
 * otherwise still hold only the old one.
 */

import { describe, expect, it, vi } from 'vitest'
import type { ServerPlaceMoved } from '$lib/ipc/bindings'

// The subscription wrapper reaches the real store, which carries a Svelte toast
// component; the cells here inject every dependency instead.
vi.mock('$lib/stores/volume-store.svelte', () => ({ applyServerPlaceMoved: vi.fn() }))
import { addTab, createTabManager, getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { push } from '../navigation/navigation-history'
import { createInitialTabState } from './tab-operations'
import type { NavigateIntent, NavigateResult } from './navigate'
import { followServerMove, type ServerMoveFollowDeps } from './server-move-follow'

const OLD_ID = 'sftp-nas-local-22-ada'
const NEW_ID = 'sftp-10-0-0-5-2222-ada'
const OLD_PREFIX = 'sftp://ada@nas.local:22'
const NEW_PREFIX = 'sftp://ada@10.0.0.5:2222'

/** The prod case: a NAS reached by its `.local` name now answers on a fixed IP and another port. */
const MOVED: ServerPlaceMoved = {
  oldPrefix: OLD_PREFIX,
  newPrefix: NEW_PREFIX,
  places: [
    {
      oldVolumeId: OLD_ID,
      newVolumeId: NEW_ID,
      newRoot: `${NEW_PREFIX}/srv/data`,
      newLanding: `${NEW_PREFIX}/srv/data`,
      name: 'Naspolya',
      connectionState: null,
    },
  ],
}

interface HarnessOpts {
  /** Each pane's active tab, as `[volumeId, path]`. */
  left: [string, string]
  right: [string, string]
  /** `lastUsedPaths`, by volume id. */
  remembered?: Record<string, string>
}

function harness(opts: HarnessOpts) {
  const managers: Record<'left' | 'right', TabManager> = {
    left: createTabManager(createInitialTabState(opts.left[1], opts.left[0])),
    right: createTabManager(createInitialTabState(opts.right[1], opts.right[0])),
  }
  const order: string[] = []
  const navigate = vi.fn((intent: NavigateIntent): NavigateResult => {
    order.push(`navigate:${intent.pane}`)
    return { status: 'started', settled: Promise.resolve() }
  })
  const saveTabs = vi.fn()
  const remembered = { ...opts.remembered }
  const deps: ServerMoveFollowDeps = {
    dismissSignIn: (oldVolumeIds) => {
      order.push(`sign-in:${oldVolumeIds.join(',')}`)
    },
    applyToVolumeList: () => {
      order.push('store')
    },
    notePlacesMoved: (newVolumeIds) => {
      order.push(`moved:${newVolumeIds.join(',')}`)
    },
    getTabMgr: (pane) => managers[pane],
    navigate,
    saveTabs,
    getLastUsedPath: (volumeId) => Promise.resolve(remembered[volumeId]),
    saveLastUsedPath: ({ volumeId, path }) => {
      remembered[volumeId] = path
      return Promise.resolve()
    },
    forgetLastUsedPath: (volumeId) => {
      delete remembered[volumeId]
      return Promise.resolve()
    },
  }
  return { managers, deps, navigate, saveTabs, remembered, order }
}

describe('followServerMove: the panes', () => {
  /**
   * ❗ The old place's sign-in sheet goes FIRST: its host-key step would dial the old
   * address, and the pane's redial at the new one may open a sheet of its own.
   */
  it('moves a pane on the old address to the same folder at the new one, after the store patch', async () => {
    const h = harness({ left: [OLD_ID, `${OLD_PREFIX}/srv/data/photos`], right: ['root', '/Users/ada'] })

    await followServerMove(MOVED, h.deps)

    // ❗ A terminal `'fallback'` commit with no history push: following the server
    // isn't a step the person took. The pane lands on the place's `saved` row and
    // dials it there.
    expect(h.navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: NEW_ID, path: `${NEW_PREFIX}/srv/data/photos` } },
      source: 'fallback',
      pushHistory: false,
    })
    expect(h.order).toEqual([`sign-in:${OLD_ID}`, 'store', `moved:${NEW_ID}`, 'navigate:left'])
  })

  it('redials a pane whose place kept its id without navigating it (a WebDAV base path that moved)', async () => {
    const id = 'webdav-cloud-443-ada'
    const prefix = 'webdav://ada@cloud.example.com:443'
    const h = harness({ left: [id, `${prefix}/Photos`], right: ['root', '/Users/ada'] })

    await followServerMove(
      {
        oldPrefix: prefix,
        newPrefix: prefix,
        places: [
          {
            oldVolumeId: id,
            newVolumeId: id,
            newRoot: prefix,
            newLanding: prefix,
            name: 'Cloud',
            connectionState: null,
          },
        ],
      },
      h.deps,
    )

    // ❗ The row was `saved` before the move too (the dial to the old URL was still
    // out), so the move count is what makes the pane dial again.
    expect(h.order).toEqual([`sign-in:${id}`, 'store', `moved:${id}`])
  })
})

describe('followServerMove: the tabs behind the panes', () => {
  it('respells a tab no pane is showing, keeps its cursor, and saves that pane’s tabs', async () => {
    const h = harness({ left: ['root', '/Users/ada'], right: ['root', '/Users/ada'] })
    const left = h.managers.left
    const behind = createInitialTabState(`${OLD_PREFIX}/srv/data/docs`, OLD_ID)
    behind.cursorFilename = 'notes.txt'
    addTab(left, getActiveTab(left).id, behind)

    await followServerMove(MOVED, h.deps)

    const moved = left.tabs.find((tab) => tab.id === behind.id)
    expect(moved?.volumeId).toBe(NEW_ID)
    expect(moved?.path).toBe(`${NEW_PREFIX}/srv/data/docs`)
    // The same folder, so the row it remembered is still there.
    expect(moved?.cursorFilename).toBe('notes.txt')
    expect(h.navigate).not.toHaveBeenCalled()
    expect(h.saveTabs).toHaveBeenCalledWith('left')
    expect(h.saveTabs).not.toHaveBeenCalledWith('right')
  })

  it('respells the back and forward history, so Back lands on the moved place too', async () => {
    const h = harness({ left: ['root', '/Users/ada'], right: ['root', '/Users/ada'] })
    const tab = getActiveTab(h.managers.left)
    tab.history = push(tab.history, { volumeId: OLD_ID, path: `${OLD_PREFIX}/srv/data` }).history
    tab.history = push(tab.history, { volumeId: 'root', path: '/Users/ada/Desktop' }).history

    await followServerMove(MOVED, h.deps)

    expect(tab.history.stack.map((entry) => [entry.volumeId, entry.path])).toEqual([
      ['root', '/Users/ada'],
      [NEW_ID, `${NEW_PREFIX}/srv/data`],
      ['root', '/Users/ada/Desktop'],
    ])
  })

  it('leaves a tab on another server alone, even one whose address starts the same', async () => {
    const h = harness({ left: ['root', '/Users/ada'], right: ['root', '/Users/ada'] })
    const lookalike = createInitialTabState('sftp://ada@nas.local:2222/srv', 'sftp-nas-local-2222-ada')
    addTab(h.managers.right, getActiveTab(h.managers.right).id, lookalike)

    await followServerMove(MOVED, h.deps)

    expect(h.managers.right.tabs.find((tab) => tab.id === lookalike.id)?.path).toBe('sftp://ada@nas.local:2222/srv')
    expect(h.saveTabs).not.toHaveBeenCalled()
  })
})

describe('followServerMove: the remembered path', () => {
  it('moves the place’s remembered path to its new id, respelled, and forgets the old id', async () => {
    const h = harness({
      left: ['root', '/'],
      right: ['root', '/'],
      remembered: { [OLD_ID]: `${OLD_PREFIX}/srv/data/photos`, root: '/Users/ada' },
    })

    await followServerMove(MOVED, h.deps)

    expect(h.remembered).toEqual({ [NEW_ID]: `${NEW_PREFIX}/srv/data/photos`, root: '/Users/ada' })
  })

  it('writes nothing when the place had no remembered path', async () => {
    const h = harness({ left: ['root', '/'], right: ['root', '/'], remembered: { root: '/Users/ada' } })
    const save = vi.spyOn(h.deps, 'saveLastUsedPath')

    await followServerMove(MOVED, h.deps)

    expect(save).not.toHaveBeenCalled()
  })
})
