/**
 * The favorites menu minus its DOM: the rows it offers and what a pick does.
 *
 * What's pinned here is everything the house `Menu` hands BACK to this module —
 * which number each row carries, why the `0` row is greyed, and which analytics
 * `via` an activation source becomes. The wiring in the other direction (a real
 * keydown reaching the right row, the two keys that swap menus, the switcher's
 * row) is `FavoritesMenu.svelte.test.ts`, which mounts the chip and presses keys.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { VolumeInfo } from '$lib/file-explorer/types'
import type { MenuActivationSource, MenuItem } from '$lib/ui/menu-types'

const addFavorite = vi.fn(() => Promise.resolve())
const removeFavorite = vi.fn(() => Promise.resolve())
const renameFavorite = vi.fn(() => Promise.resolve())
const setFavoriteShortcut = vi.fn(() => Promise.resolve())
const reorderFavorites = vi.fn<(ids: string[]) => Promise<void>>(() => Promise.resolve())
const resolvePathVolume = vi.fn<(path: string) => Promise<{ volume: VolumeInfo | null; timedOut: boolean }>>()
const trackEvent = vi.fn()
const addToast = vi.fn<(message: string, options: { level: string }) => void>()

/** The volume list the store mock answers with: the favorites, plus the disk they sit on. */
const stubs = vi.hoisted(() => ({
  volumes: [] as unknown[],
  showVirtualGitPortal: false,
}))

vi.mock('$lib/tauri-commands', () => ({
  addFavorite: (...args: unknown[]) => addFavorite(...(args as [])),
  removeFavorite: (...args: unknown[]) => removeFavorite(...(args as [])),
  renameFavorite: (...args: unknown[]) => renameFavorite(...(args as [])),
  setFavoriteShortcut: (...args: unknown[]) => setFavoriteShortcut(...(args as [])),
  reorderFavorites: (ids: string[]) => reorderFavorites(ids),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
  resolvePathVolume: (path: string) => resolvePathVolume(path),
  trackEvent: (...args: unknown[]) => {
    trackEvent(...(args as []))
    return Promise.resolve()
  },
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: (message: string, options: { level: string }) => {
    addToast(message, options)
  },
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => stubs.volumes }))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  // Read by `volume-capabilities.ts` to classify a `.git`-portal path, which is one of
  // the two reasons the `0` row greys out.
  getShowVirtualGitPortal: () => stubs.showVirtualGitPortal,
}))

vi.mock('$lib/logging/logger', () => ({ getAppLogger: () => ({ warn: vi.fn() }) }))

import { createFavoritesMenu, ADD_ROW_VALUE, FAVORITES_SECTION_ID } from './favorites-menu.svelte'

const DISK: VolumeInfo = { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }

function favorite(n: number, path: string, name = `Fav ${String(n)}`): VolumeInfo {
  return { id: `fav-${String(n)}`, name, path, category: 'favorite', isEjectable: false }
}

const THREE_FAVORITES = [
  favorite(1, '/Users/test/Documents', 'Documents'),
  favorite(2, '/Users/test/Downloads', 'Downloads'),
  favorite(3, '/Users/test/Projects', 'Projects'),
]

interface Harness {
  menu: ReturnType<typeof createFavoritesMenu>
  /** Every `VolumeChangePayload` the menu handed its host. */
  went: { volumeId: string; volumePath: string; targetPath: string }[]
  renameInput: HTMLInputElement
}

let dispose: (() => void) | undefined

/**
 * Builds the controller inside an effect root, so its `$derived` rows and the
 * optimistic-order reconciliation `$effect` both run the way they do in a component.
 */
function harness(paneCurrentPath = '/Users/test/elsewhere', paneVolumeId = 'root'): Harness {
  const went: Harness['went'] = []
  const renameInput = document.createElement('input')
  let menu!: ReturnType<typeof createFavoritesMenu>
  dispose = $effect.root(() => {
    menu = createFavoritesMenu({
      getVolumes: () => stubs.volumes as VolumeInfo[],
      getPaneVolumeId: () => paneVolumeId,
      getPaneCurrentPath: () => paneCurrentPath,
      getDirIconFallback: () => '/icons/dir.png',
      getRenameInputRef: () => renameInput,
      getShortcutInputRef: () => renameInput,
      go: (target) => {
        went.push(target)
      },
    })
  })
  flushSync()
  return { menu, went, renameInput }
}

/** The one row of the add section: the `0` row. */
function addRow(menu: ReturnType<typeof createFavoritesMenu>): MenuItem {
  const section = menu.sections.find((s) => s.id === 'add')
  expect(section).toBeTruthy()
  return section?.items[0] as MenuItem
}

function favoriteRows(menu: ReturnType<typeof createFavoritesMenu>) {
  return menu.sections.find((s) => s.id === FAVORITES_SECTION_ID)?.items ?? []
}

beforeEach(() => {
  stubs.volumes = [...THREE_FAVORITES, DISK]
  stubs.showVirtualGitPortal = false
  addFavorite.mockClear()
  removeFavorite.mockClear()
  renameFavorite.mockClear()
  setFavoriteShortcut.mockClear()
  reorderFavorites.mockClear()
  reorderFavorites.mockImplementation(() => Promise.resolve())
  trackEvent.mockClear()
  addToast.mockClear()
  resolvePathVolume.mockReset()
  resolvePathVolume.mockResolvedValue({ volume: DISK, timedOut: false })
})

afterEach(() => {
  dispose?.()
  dispose = undefined
})

describe('the number column', () => {
  it('keeps a letter shortcut separate from the numbered accelerator', () => {
    stubs.volumes = [{ ...THREE_FAVORITES[0], favoriteShortcut: 'P' }, ...THREE_FAVORITES.slice(1), DISK]
    const { menu } = harness()
    expect(favoriteRows(menu)[0]).toMatchObject({ accelerator: '1', shortcut: 'P' })
  })
  it('numbers the favorites 1-9 in display order, and gives the add row 0', () => {
    const { menu } = harness()
    expect(favoriteRows(menu).map((item) => item.accelerator)).toEqual(['1', '2', '3'])
    expect(addRow(menu).accelerator).toBe('0')
  })

  it('leaves a favorite past the ninth with NO number: there is no single digit left to give', () => {
    // Eleven favorites: the tenth and eleventh get an empty number column and are
    // reached by arrow or pointer instead. They are still real, activatable rows.
    stubs.volumes = [...Array.from({ length: 11 }, (_, i) => favorite(i + 1, `/Users/test/f${String(i + 1)}`)), DISK]
    const { menu } = harness()
    const rows = favoriteRows(menu)
    expect(rows).toHaveLength(11)
    expect(rows.map((item) => item.accelerator)).toEqual([
      '1',
      '2',
      '3',
      '4',
      '5',
      '6',
      '7',
      '8',
      '9',
      undefined,
      undefined,
    ])
    expect(rows[9].disabled ?? false).toBe(false)
  })

  it('renumbers after a reorder, so the digit always means the row you can see', () => {
    const { menu } = harness()
    menu.applyReorder(['fav-3', 'fav-1', 'fav-2'])
    flushSync()
    const rows = favoriteRows(menu)
    expect(rows.map((item) => item.value)).toEqual(['fav-3', 'fav-1', 'fav-2'])
    expect(rows.map((item) => item.accelerator)).toEqual(['1', '2', '3'])
  })
})

/**
 * The `0` row's three states. The two REFUSALS have to say different things: one is
 * "you're already here" and the other is "a favorite could never point here", and a
 * greyed row with the wrong sentence is worse than no sentence.
 */
describe('the add row', () => {
  it('is enabled, with nothing to explain, on a plain folder that is not a favorite yet', () => {
    const { menu } = harness('/Users/test/elsewhere')
    const row = addRow(menu)
    expect(row.disabled).toBe(false)
    expect(row.tooltip).toBeUndefined()
  })

  it('greys out on a folder that is already a favorite, saying so', () => {
    const { menu } = harness('/Users/test/Downloads')
    const row = addRow(menu)
    expect(row.disabled).toBe(true)
    expect(row.tooltip).toBe('This folder is already a favorite')
  })

  it('greys out where a favorite could never point, with a DIFFERENT reason', () => {
    // A pane inside a zip: `capabilitiesForPane` reads the kind off the PATH, so the
    // writable parent drive in `volumeId` doesn't make it favoritable.
    const { menu } = harness('/Users/test/archive.zip/inner')
    const row = addRow(menu)
    expect(row.disabled).toBe(true)
    expect(row.tooltip).toBe('This folder can’t be a favorite: favorites only work on disks and mounted shares')

    // The point of the pin: a user who sees the greyed row learns which of the two
    // it is. Same string for both would be a dead end wearing an explanation.
    const already = harness('/Users/test/Downloads')
    expect(addRow(already.menu).tooltip).not.toBe(row.tooltip)
  })

  it('answers CAPABILITY first: an unfavoritable pane never claims the folder is already a favorite', () => {
    // The zip path is in the list AND unfavoritable. "Already a favorite" would be
    // answering a question that doesn't arise here.
    stubs.volumes = [favorite(1, '/Users/test/archive.zip/inner'), DISK]
    const { menu } = harness('/Users/test/archive.zip/inner')
    expect(addRow(menu).tooltip).toBe(
      'This folder can’t be a favorite: favorites only work on disks and mounted shares',
    )
  })

  it('ignores a trailing slash when deciding "already a favorite"', () => {
    // The store dedupes an add the same way, so the row must agree with it.
    const { menu } = harness('/Users/test/Downloads/')
    expect(addRow(menu).disabled).toBe(true)
  })

  it('adds the pane`s folder when picked, and opens no favorite', async () => {
    const { menu, went } = harness('/Users/test/elsewhere')
    await menu.select(addRow(menu) as MenuItem<never>, 'accelerator')
    expect(addFavorite).toHaveBeenCalledWith('/Users/test/elsewhere', null)
    expect(went).toEqual([])
    expect(trackEvent).not.toHaveBeenCalledWith('favorite_opened', expect.anything())
  })
})

/**
 * Opening a favorite. The pane lands on the CONTAINING volume (`open-favorite.ts`
 * owns that half and pins it), and the analytics `via` is whatever the primitive said
 * made the pick — the one question the number column exists to answer.
 */
describe('opening a favorite', () => {
  async function pick(menu: ReturnType<typeof createFavoritesMenu>, index: number, source: MenuActivationSource) {
    await menu.select(favoriteRows(menu)[index], source)
  }

  it('sends the pane to the favorite`s path on the volume that contains it', async () => {
    const { menu, went } = harness()
    await pick(menu, 1, 'pointer')
    expect(resolvePathVolume).toHaveBeenCalledWith('/Users/test/Downloads')
    expect(went).toEqual([{ volumeId: 'root', volumePath: '/', targetPath: '/Users/test/Downloads' }])
  })

  it.each([
    ['accelerator', 'digit'],
    ['keyboard', 'keyboard'],
    ['pointer', 'pointer'],
  ] as const)('reports a %s activation as via: %s', async (source, via) => {
    const { menu } = harness()
    await pick(menu, 0, source)
    expect(trackEvent).toHaveBeenCalledWith('favorite_opened', { surface: 'favorites_menu', via })
  })

  it('reports NOTHING when the favorite resolves to no volume, and leaves the pane put', async () => {
    resolvePathVolume.mockResolvedValue({ volume: null, timedOut: false })
    const { menu, went } = harness()
    await pick(menu, 0, 'accelerator')
    expect(went).toEqual([])
    expect(trackEvent).not.toHaveBeenCalled()
  })
})

describe('the empty list', () => {
  it('still reads as a favorites section, with its placeholder and the add row', () => {
    stubs.volumes = [DISK]
    const { menu } = harness()
    const section = menu.sections.find((s) => s.id === FAVORITES_SECTION_ID)
    expect(section?.items).toEqual([])
    expect(section?.emptyLabel).toBe('(Your favorites will show here)')
    expect(section?.heading).toBe('Favorites')
    // The way OUT of the empty state is the row that's still there.
    expect(addRow(menu).value).toBe(ADD_ROW_VALUE)
    expect(addRow(menu).disabled).toBe(false)
  })
})

/** Reordering, renaming, removing: the three edits a favorite takes from this menu. */
describe('editing favorites', () => {
  it('persists a settled order with the `fav-` prefix stripped, and shows it before the backend answers', () => {
    const { menu } = harness()
    menu.applyReorder(['fav-2', 'fav-1', 'fav-3'])
    flushSync()
    // The optimistic override paints first: the store mock never changes, so this is
    // the local order alone.
    expect(menu.favorites.map((f) => f.id)).toEqual(['fav-2', 'fav-1', 'fav-3'])
    expect(reorderFavorites).toHaveBeenCalledWith(['2', '1', '3'])
  })

  it('reverts to the store order and says so when the persist fails', async () => {
    reorderFavorites.mockRejectedValueOnce(new Error('nope'))
    const { menu } = harness()
    menu.applyReorder(['fav-3', 'fav-2', 'fav-1'])
    await vi.waitFor(() => {
      expect(addToast).toHaveBeenCalled()
    })
    flushSync()
    expect(menu.favorites.map((f) => f.id)).toEqual(['fav-1', 'fav-2', 'fav-3'])
  })

  it('starts an inline rename from the native row menu, and the editor then owns every key', () => {
    const { menu } = harness()
    expect(menu.isEditing()).toBe(false)
    menu.handleContextAction({ action: 'rename-favorite', volumeId: 'fav-2' })
    flushSync()
    expect(menu.renamingFavoriteId).toBe('fav-2')
    expect(menu.renameDraft).toBe('Downloads')
    expect(menu.isEditing()).toBe(true)
  })

  it('commits a trimmed rename, and leaves the editor', async () => {
    const { menu } = harness()
    menu.handleContextAction({ action: 'rename-favorite', volumeId: 'fav-2' })
    flushSync()
    menu.renameDraft = '  Loads  '
    await menu.commitRename(THREE_FAVORITES[1])
    expect(renameFavorite).toHaveBeenCalledWith('2', 'Loads')
    expect(menu.isEditing()).toBe(false)
  })

  it.each([
    ['an unchanged name', 'Downloads'],
    ['a name typed away to nothing', '   '],
  ])('writes nothing for %s', async (_label, draft) => {
    const { menu } = harness()
    menu.handleContextAction({ action: 'rename-favorite', volumeId: 'fav-2' })
    flushSync()
    menu.renameDraft = draft
    await menu.commitRename(THREE_FAVORITES[1])
    expect(renameFavorite).not.toHaveBeenCalled()
    expect(menu.isEditing()).toBe(false)
  })

  it('removes a favorite from the native row menu', async () => {
    const { menu } = harness()
    menu.handleContextAction({ action: 'remove-favorite', volumeId: 'fav-3' })
    await vi.waitFor(() => {
      expect(removeFavorite).toHaveBeenCalledWith('3')
    })
  })

  it('ignores a row-menu pick for a volume that is not one of ITS favorites', () => {
    // Every pane's menu hears `volume-context-action`; the id alone can't say whose
    // it was, so a menu that doesn't list the row does nothing with it.
    const { menu } = harness()
    menu.handleContextAction({ action: 'remove-favorite', volumeId: 'fav-99' })
    menu.handleContextAction({ action: 'eject', volumeId: 'fav-1' } as never)
    flushSync()
    expect(removeFavorite).not.toHaveBeenCalled()
    expect(menu.isEditing()).toBe(false)
  })

  it('cancels a rename without writing', () => {
    const { menu } = harness()
    menu.handleContextAction({ action: 'rename-favorite', volumeId: 'fav-1' })
    flushSync()
    menu.renameDraft = 'Something else'
    menu.cancelRename()
    expect(menu.isEditing()).toBe(false)
    expect(renameFavorite).not.toHaveBeenCalled()
  })
})
