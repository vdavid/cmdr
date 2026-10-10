/**
 * The favorites menu as a person meets it: the chip is mounted and driven with real
 * keydowns, clicks, and drags, through the house `Menu` that owns every interaction.
 *
 * What lives here is the WIRING — a typed digit reaching the right favorite, the two
 * keys that swap the chip's two menus, the switcher's own favorites section, and
 * the three edits a favorite takes. What each row SAYS and what a pick does is
 * `favorites-menu.svelte.test.ts`, which drives the controller with no DOM.
 *
 * ❗ The swap keys go through `eventMatchesCommand`, so these press the combos the
 * registry holds rather than asserting a hardcoded key: a rebind has to follow, and a
 * test that pressed a literal ⌃D would pass while the feature broke.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, flushSync } from 'svelte'
import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'
import type { VolumeChangePayload } from '../pane/types'

const addFavorite = vi.fn(() => Promise.resolve())
const removeFavorite = vi.fn(() => Promise.resolve())
const renameFavorite = vi.fn(() => Promise.resolve())
const setFavoriteShortcut = vi.fn(() => Promise.resolve())
const reorderFavorites = vi.fn(() => Promise.resolve())
const trackEvent = vi.fn()
const setSetting = vi.fn()

const stubs = vi.hoisted(() => ({
  /** The volume list the store mock answers with: the favorites, plus the disk they sit on. */
  volumes: null as unknown[] | null,
  /** The pane's folder, which is what the `0` add row acts on. */
  currentPath: '/Users/test/elsewhere',
  /** Whether the switcher's favorites section was left open, as the setting remembers it. */
  switcherFavoritesExpanded: false,
}))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/settings')>()),
  setSetting: (...args: unknown[]) => {
    setSetting(...(args as []))
  },
}))

vi.mock('$lib/tauri-commands', () => ({
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: { id: 'root', path: '/' }, timedOut: false })),
  upgradeToSmbVolume: vi.fn(() => Promise.resolve({ status: 'success' })),
  ejectVolume: vi.fn(() => Promise.resolve()),
  getVolumeSpace: vi.fn(() => Promise.resolve(null)),
  systemHasSavedSmbPassword: vi.fn(() => Promise.resolve(false)),
  upgradeToSmbVolumeUsingSavedPassword: vi.fn(() => Promise.resolve({ status: 'success' })),
  addFavorite: (...args: unknown[]) => addFavorite(...(args as [])),
  removeFavorite: (...args: unknown[]) => removeFavorite(...(args as [])),
  renameFavorite: (...args: unknown[]) => renameFavorite(...(args as [])),
  setFavoriteShortcut: (...args: unknown[]) => setFavoriteShortcut(...(args as [])),
  reorderFavorites: (...args: unknown[]) => reorderFavorites(...(args as [])),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
  disconnectPlace: vi.fn(() => Promise.resolve(true)),
  forgetServer: vi.fn(() => Promise.resolve(true)),
  forgetServerSecret: vi.fn(() => Promise.resolve(true)),
  hasServerSecret: vi.fn(() => Promise.resolve(true)),
  listSavedServers: vi.fn(() => Promise.resolve([])),
  trackEvent: (...args: unknown[]) => {
    trackEvent(...(args as []))
    return Promise.resolve()
  },
}))

vi.mock('$lib/tauri-commands/indexing', () => ({
  getVolumeIndexStatusById: () => Promise.resolve({ status: 'timedOut' }),
  onIndexFreshnessChanged: () => Promise.resolve(() => {}),
  onIndexScanStarted: () => Promise.resolve(() => {}),
  onIndexScanComplete: () => Promise.resolve(() => {}),
}))

vi.mock('../network/direct-connect', () => ({ connectDirectly: vi.fn(() => Promise.resolve({ kind: 'connected' })) }))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => stubs.volumes ?? DEFAULT_VOLUMES,
  getVolumesTimedOut: () => false,
  isVolumesRefreshing: () => false,
  isVolumeRetryFailed: () => false,
  requestVolumeRefresh: vi.fn(),
}))

vi.mock('$lib/stores/volume-busy-store.svelte', () => ({
  isVolumeBusy: () => false,
  isVolumeEjecting: () => false,
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(() => 'toast-id'),
  addToastForPane: vi.fn(() => 'toast-id'),
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: (n: number) => `${String(n)} B`,
  getFileSizeFormat: () => 'binary',
  getFileSizeUnit: () => 'bytes',
  getNetworkEnabled: () => true,
  getUseAppIconsForDocuments: () => false,
  getShowVirtualGitPortal: () => false,
  getDriveIndexingEnabled: () => false,
  getSwitcherFavoritesExpanded: () => stubs.switcherFavoritesExpanded,
}))

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: vi.fn().mockReturnValue('/icons/dir.png'),
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
  }
})

// The test DOM has no layout, so no scroll: a stub keeps `scrollHighlightedIntoView`
// from throwing into a floating promise.
Element.prototype.scrollIntoView = vi.fn()

const DISK = { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }

function favorite(n: number, path: string, name = `Fav ${String(n)}`) {
  return { id: `fav-${String(n)}`, name, path, category: 'favorite', isEjectable: false }
}

const DEFAULT_VOLUMES = [
  favorite(1, '/Users/test/Documents', 'Documents'),
  favorite(2, '/Users/test/Downloads', 'Downloads'),
  favorite(3, '/Users/test/Projects', 'Projects'),
  DISK,
]

/** Eleven favorites: the tenth and eleventh are past the last single digit. */
const ELEVEN_FAVORITES = [
  ...Array.from({ length: 11 }, (_, i) => favorite(i + 1, `/Users/test/f${String(i + 1)}`)),
  DISK,
]

interface BreadcrumbInstance {
  openVolumeChooser: () => void
  toggleVolumeChooser: () => void
  toggleFavoritesMenu: () => void
  closeHeaderMenu: () => void
  isHeaderMenuOpen: () => boolean
}

// ============================================================================
// Both menus are the house `Menu`: they PORTAL to `document.body`, carry the documented
// `data-*` hooks (`$lib/ui/DETAILS.md` § Menu), and catch keys on their own document
// capture listener. So these look at the document, select on hooks rather than classes,
// and press keys for real.
// ============================================================================

function menuSurface(): HTMLElement | null {
  return document.querySelector('[data-menu]')
}

function menuRows(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>('[data-menu] [data-menu-row]')]
}

/** The favorites menu's own rows, the `0` add row excluded. */
function favoriteRows(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>('[data-menu-section="favorites"] [data-menu-row]')]
}

function menuRow(value: string): HTMLElement | null {
  return document.querySelector(`[data-menu] [data-menu-row="${value}"]`)
}

/** A row in the open submenu, by value. */
function submenuRow(value: string): HTMLElement | null {
  return document.querySelector(`[data-menu-submenu] [data-menu-row="${value}"]`)
}

/** Right-click the nth favorite, which opens its submenu, and wait for it to be placed. */
async function openFavoriteSubmenu(index: number): Promise<void> {
  favoriteRows()[index].dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
  await tick()
  flushSync()
  // The submenu renders once its position effect measured the parent row, inside a tick of its own.
  await tick()
  await tick()
}

function isHighlighted(row: Element | null | undefined): boolean {
  return row?.hasAttribute('data-highlighted') ?? false
}

/**
 * Which of the chip's two menus is up. Each labels its surface with the name it carries in
 * Settings > Keyboard shortcuts, so the label IS the identity.
 */
const SWITCHER = 'Volume switcher'
const FAVORITES = 'Favorites menu'

function openMenuName(): string | null {
  return menuSurface()?.getAttribute('aria-label') ?? null
}

/** Press a key the way the app does. Returns true when a menu claimed it. */
function press(key: string, init: KeyboardEventInit = {}): boolean {
  return !document.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }))
}

/**
 * Type a digit. ❗ The accelerator matcher reads `event.code`, so the physical key
 * decides — which is what makes the number keys work on a layout where a digit needs
 * Shift. A test dispatching `key` alone would pass nothing to match on.
 */
function pressDigit(digit: number): boolean {
  const n = String(digit)
  return press(n, { code: `Digit${n}` })
}

/**
 * The combo a command is bound to right now, as a `KeyboardEventInit`. ❗ Built from
 * the registry, never hardcoded, so these tests follow a rebind the way the menu does.
 */
const CTRL_D: KeyboardEventInit = { key: 'd', ctrlKey: true }
const LEFT_CHOOSER: KeyboardEventInit = { key: 'F1', altKey: true }
const RIGHT_CHOOSER: KeyboardEventInit = { key: 'F2', altKey: true }

interface BreadcrumbProps {
  paneId?: 'left' | 'right'
  volumeId?: string
  currentPath?: string
  onVolumeChange?: (change: VolumeChangePayload) => void
}

function mountBreadcrumb(props: BreadcrumbProps = {}): { instance: BreadcrumbInstance; target: HTMLDivElement } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(VolumeBreadcrumb, {
    target,
    props: {
      paneId: 'left' as const,
      volumeId: 'root',
      currentPath: stubs.currentPath,
      // Nothing checked: the switcher's cursor then opens on its first row.
      containingVolumeId: null,
      ...props,
    },
  }) as unknown as BreadcrumbInstance
  // Settle the chip's `bind:this`: the menus hang under that element, so an `open()`
  // before the binding lands would have nothing to anchor to.
  flushSync()
  return { instance, target }
}

/** Mounts the chip and opens the favorites menu the way ⌃D does. */
async function openFavorites(
  props: BreadcrumbProps = {},
): Promise<{ instance: BreadcrumbInstance; target: HTMLDivElement }> {
  const mounted = mountBreadcrumb(props)
  mounted.instance.toggleFavoritesMenu()
  await tick()
  flushSync()
  return mounted
}

beforeEach(() => {
  document.body.innerHTML = ''
  stubs.volumes = null
  stubs.currentPath = '/Users/test/elsewhere'
  addFavorite.mockClear()
  removeFavorite.mockClear()
  renameFavorite.mockClear()
  setFavoriteShortcut.mockClear()
  reorderFavorites.mockClear()
  trackEvent.mockClear()
  setSetting.mockClear()
  stubs.switcherFavoritesExpanded = false
})

// A menu left open outlives its target div: it's portaled, and its key listener lives on
// the document. Escape closes whichever one is open before the next test mounts.
afterEach(() => {
  press('Escape')
  press('Escape')
  document.body.innerHTML = ''
})

/**
 * The number keys, which are the whole point of the menu: getting back somewhere is one
 * keystroke rather than a click into a dropdown.
 */
describe('the number keys', () => {
  it('opens the Nth favorite on the Nth digit', async () => {
    const onVolumeChange = vi.fn()
    await openFavorites({ onVolumeChange })

    expect(pressDigit(2)).toBe(true)
    await vi.waitFor(() => {
      expect(onVolumeChange).toHaveBeenCalled()
    })
    // The pane lands on the CONTAINING volume, at the favorite's own path.
    expect(onVolumeChange).toHaveBeenCalledWith({
      volumeId: 'root',
      volumePath: '/',
      targetPath: '/Users/test/Downloads',
      exact: true,
    })
    // A pick closes the menu: the digit is the whole interaction.
    expect(menuSurface()).toBeNull()
  })

  it('shows the digit on the row it opens, so the list teaches itself', async () => {
    await openFavorites()
    expect(favoriteRows().map((row) => row.getAttribute('data-accelerator'))).toEqual(['1', '2', '3'])
    // Assistive tech is told the same thing the glyph shows.
    expect(favoriteRows()[1].getAttribute('aria-keyshortcuts')).toBe('2')
    expect(menuRow('favorites:add')?.getAttribute('data-accelerator')).toBe('0')
  })

  it('leaves a favorite past the ninth with no number, still reachable by arrow and by pointer', async () => {
    stubs.volumes = ELEVEN_FAVORITES
    const onVolumeChange = vi.fn()
    await openFavorites({ onVolumeChange })

    const tenth = favoriteRows()[9]
    expect(tenth.hasAttribute('data-accelerator')).toBe(false)
    expect(tenth.hasAttribute('data-disabled')).toBe(false)

    // The arrows walk onto it like any other row (the cursor opens on row 0).
    for (let i = 0; i < 9; i++) press('ArrowDown')
    await tick()
    flushSync()
    expect(isHighlighted(favoriteRows()[9])).toBe(true)

    // And a click opens it, which is the other half of "still reachable".
    favoriteRows()[9].click()
    await vi.waitFor(() => {
      expect(onVolumeChange).toHaveBeenCalledWith(expect.objectContaining({ targetPath: '/Users/test/f10' }))
    })
  })

  it('swallows a digit no row claims, rather than letting it reach the pane behind', async () => {
    const onVolumeChange = vi.fn()
    await openFavorites({ onVolumeChange })

    // Only three favorites, so `7` names nothing. An open menu owns the keyboard either
    // way: without this the digit would land in the pane's type-to-jump buffer.
    expect(pressDigit(7)).toBe(true)
    await tick()
    flushSync()
    expect(onVolumeChange).not.toHaveBeenCalled()
    expect(menuSurface()).toBeTruthy()
  })

  it('runs the add row on `0`', async () => {
    await openFavorites()
    expect(pressDigit(0)).toBe(true)
    await vi.waitFor(() => {
      expect(addFavorite).toHaveBeenCalledWith('/Users/test/elsewhere', null)
    })
  })

  it('refuses `0` while the row is greyed, and leaves the menu open to say so', async () => {
    stubs.currentPath = '/Users/test/Downloads'
    await openFavorites({ currentPath: '/Users/test/Downloads' })

    const addRow = menuRow('favorites:add')
    expect(addRow?.hasAttribute('data-disabled')).toBe(true)
    expect(pressDigit(0)).toBe(true)
    await tick()
    flushSync()
    expect(addFavorite).not.toHaveBeenCalled()
    expect(menuSurface()).toBeTruthy()
  })
})

/**
 * The keys that swap the chip's two menus. Central dispatch is suppressed while either
 * is open, so the menu host matches them itself — through `eventMatchesCommand`, so a
 * rebind follows.
 *
 * ❗ These assert what the key DID, ❌ not that `press` came back claimed: a key an
 * `onKey` claims is the one class the primitive returns `true` for without stopping the
 * event, so the claim isn't visible from the outside. Whether that hole should close is
 * a primitive-level question, not this menu's.
 */
describe('swapping between the two menus', () => {
  it('⌃D inside the switcher hands the header to the favorites menu', async () => {
    const { instance } = mountBreadcrumb()
    instance.openVolumeChooser()
    await tick()
    flushSync()
    expect(openMenuName()).toBe(SWITCHER)

    press(CTRL_D.key as string, CTRL_D)
    await tick()
    flushSync()
    expect(openMenuName()).toBe(FAVORITES)
    // One menu at a time: the switcher is gone, not stacked behind.
    expect(document.querySelectorAll('[data-menu]')).toHaveLength(1)
    expect(instance.isHeaderMenuOpen()).toBe(true)
  })

  it('⌃D inside the favorites menu closes it (the key that opened it toggles)', async () => {
    const { instance } = await openFavorites()
    press(CTRL_D.key as string, CTRL_D)
    await tick()
    flushSync()
    expect(menuSurface()).toBeNull()
    expect(instance.isHeaderMenuOpen()).toBe(false)
  })

  it("the pane's own chooser key swaps back to the switcher", async () => {
    await openFavorites({ paneId: 'left' })
    press(LEFT_CHOOSER.key as string, LEFT_CHOOSER)
    await tick()
    flushSync()
    expect(openMenuName()).toBe(SWITCHER)
    expect(document.querySelectorAll('[data-menu]')).toHaveLength(1)
  })

  it("❗ the OTHER pane's chooser key does not, so ⌥F2 over the left pane still means the right pane", async () => {
    await openFavorites({ paneId: 'left' })
    press(RIGHT_CHOOSER.key as string, RIGHT_CHOOSER)
    await tick()
    flushSync()
    // Unclaimed by this menu, and swallowed rather than acted on: the favorites menu
    // stays up, and the right pane's switcher is that pane's business.
    expect(openMenuName()).toBe(FAVORITES)
  })
})

/**
 * The switcher's favorites section: the favorites themselves, folded behind a "Favorites"
 * row that remembers whether it was left open, so once a person opens it every favorite is
 * one click from the chip. It reuses the favorites menu's rows, so what a row shows and what
 * a pick does are pinned in `favorites-menu.svelte.test.ts`; this is the wiring.
 */
describe('the favorites section in the switcher', () => {
  const HEADER = 'menu-disclosure:favorites'

  async function openSwitcher(volumes?: unknown[]) {
    if (volumes) stubs.volumes = volumes
    const { instance } = mountBreadcrumb()
    instance.openVolumeChooser()
    await tick()
    flushSync()
    return instance
  }

  it('starts folded, saying how many favorites it holds', async () => {
    await openSwitcher()
    const header = menuRow(HEADER)
    expect(header?.getAttribute('aria-expanded')).toBe('false')
    expect(header?.textContent).toContain('Favorites')
    expect(header?.querySelector('[data-menu-disclosure-count]')?.textContent).toBe('3')
    expect(menuRow('fav-1')).toBeNull()
  })

  it('teaches the key with a live chip on the section row', async () => {
    await openSwitcher()
    // The chip reads the binding from the registry, so a rebind shows here. Its presence
    // is the contract; the glyphs themselves are `ShortcutChip`'s own tests.
    expect(menuRow(HEADER)?.querySelector('.shortcut-chip')).toBeTruthy()
  })

  it('opens on a click, stays up, and remembers it for next time', async () => {
    await openSwitcher()
    menuRow(HEADER)?.click()
    await tick()
    flushSync()
    expect(openMenuName()).toBe(SWITCHER)
    expect(menuRow('fav-1')).not.toBeNull()
    expect(menuRow('fav-3')).not.toBeNull()
    expect(setSetting).toHaveBeenCalledWith('behavior.switcherFavoritesExpanded', true)
  })

  it('folds again with ←, from a favorite inside it', async () => {
    stubs.switcherFavoritesExpanded = true
    await openSwitcher()
    expect(isHighlighted(menuRow(HEADER))).toBe(true)
    press('ArrowDown')
    press('ArrowLeft')
    press('ArrowLeft')
    await tick()
    flushSync()
    expect(menuRow('fav-1')).toBeNull()
    expect(setSetting).toHaveBeenLastCalledWith('behavior.switcherFavoritesExpanded', false)
  })

  it('opens already unfolded once it was left that way', async () => {
    stubs.switcherFavoritesExpanded = true
    await openSwitcher()
    expect(menuRow(HEADER)?.getAttribute('aria-expanded')).toBe('true')
    expect(menuRow('fav-2')?.textContent).toContain('Downloads')
  })

  it('opens a picked favorite the one way favorites open, and says the switcher did it', async () => {
    stubs.switcherFavoritesExpanded = true
    const onVolumeChange = vi.fn()
    const { instance } = mountBreadcrumb({ onVolumeChange })
    instance.openVolumeChooser()
    await tick()
    flushSync()
    menuRow('fav-2')?.click()
    await vi.waitFor(() => {
      expect(trackEvent).toHaveBeenCalledWith('favorite_opened', {
        surface: 'switcher',
        via: 'pointer',
        reach: 'ready',
      })
    })
    expect(onVolumeChange).toHaveBeenCalledWith(expect.objectContaining({ targetPath: '/Users/test/Downloads' }))
  })

  it('numbers no favorite (the digits are ⌃D’s), but a saved letter still opens one', async () => {
    stubs.switcherFavoritesExpanded = true
    stubs.volumes = [
      favorite(1, '/Users/test/Documents', 'Documents'),
      { ...favorite(2, '/Users/test/Downloads', 'Downloads'), favoriteShortcut: 'P' },
      DISK,
    ]
    await openSwitcher()
    expect(menuRow('fav-1')?.hasAttribute('data-accelerator')).toBe(false)
    expect(menuRow('fav-2')?.querySelector('.menu-shortcut')?.textContent).toBe('P')
    press('p')
    await vi.waitFor(() => {
      expect(trackEvent).toHaveBeenCalledWith('favorite_opened', { surface: 'switcher', via: 'letter', reach: 'ready' })
    })
  })

  it('lets a folded favorite’s letter open nothing: a row out of sight stays shut', async () => {
    stubs.volumes = [{ ...favorite(1, '/Users/test/Documents', 'Documents'), favoriteShortcut: 'P' }, DISK]
    await openSwitcher()
    press('p')
    await tick()
    expect(trackEvent).not.toHaveBeenCalledWith('favorite_opened', expect.anything())
    expect(openMenuName()).toBe(SWITCHER)
  })

  it('dims a favorite a pick can’t open right away, and still lets it be picked', async () => {
    stubs.switcherFavoritesExpanded = true
    stubs.volumes = [
      {
        ...favorite(1, '/Volumes/naspi/docs', 'Docs'),
        favoriteTarget: {
          volumeId: 'smb-naspi',
          volumeName: 'naspi',
          volumeRoot: '/Volumes/naspi',
          reach: { kind: 'connects' },
        },
      },
      DISK,
    ]
    await openSwitcher()
    const row = menuRow('fav-1')
    expect(row?.querySelector('.favorite-label.is-unreachable')).not.toBeNull()
    expect(row?.hasAttribute('data-disabled')).toBe(false)
  })

  it('renames a favorite from its row actions, in the row', async () => {
    stubs.switcherFavoritesExpanded = true
    await openSwitcher()
    menuRow('fav-1')?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    await tick()
    flushSync()
    await tick()
    await tick()
    submenuRow('row:fav-1:rename-favorite')?.click()
    await tick()
    flushSync()
    await tick()
    const input = document.querySelector<HTMLInputElement>('.favorite-rename-input')
    expect(input).not.toBeNull()
    if (!input) return
    input.value = 'Docs'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(renameFavorite).toHaveBeenCalledWith('1', 'Docs')
    })
  })

  it('reorders with ⌥↓ like the favorites menu does', async () => {
    stubs.switcherFavoritesExpanded = true
    await openSwitcher()
    press('ArrowDown')
    press('ArrowDown', { altKey: true })
    await vi.waitFor(() => {
      expect(reorderFavorites).toHaveBeenCalledWith(['2', '1', '3'])
    })
  })

  it('keeps the folded row with a zero when there are no favorites, and says so when opened', async () => {
    stubs.switcherFavoritesExpanded = true
    await openSwitcher([DISK])
    expect(menuRow(HEADER)?.querySelector('[data-menu-disclosure-count]')?.textContent).toBe('0')
    expect(document.querySelector('[data-menu-section="favorites"] [data-menu-empty]')).not.toBeNull()
  })

  it('reports ⌃D as the command, whether it was typed in the switcher or anywhere else', async () => {
    const instance = await openSwitcher()
    press(CTRL_D.key as string, CTRL_D)
    await tick()
    flushSync()
    expect(trackEvent).toHaveBeenCalledWith('favorites_menu_opened', { trigger: 'command' })

    // A menu-bar accelerator and a palette pick arrive the same way, so `command` is the
    // whole other arm.
    trackEvent.mockClear()
    instance.closeHeaderMenu()
    await tick()
    instance.toggleFavoritesMenu()
    await tick()
    flushSync()
    expect(trackEvent).toHaveBeenCalledWith('favorites_menu_opened', { trigger: 'command' })
  })
})

/**
 * How a pick was made rides all the way to the event, which is the one question the
 * number column exists to answer: do the digits earn their place, or does everyone
 * arrow down anyway?
 */
describe('how a favorite was opened', () => {
  it('reports a digit as `digit`', async () => {
    await openFavorites()
    pressDigit(1)
    await vi.waitFor(() => {
      expect(trackEvent).toHaveBeenCalledWith('favorite_opened', {
        surface: 'favorites_menu',
        via: 'digit',
        reach: 'ready',
      })
    })
  })

  it('shows a saved letter at the right and opens its favorite without modifiers', async () => {
    stubs.volumes = [
      favorite(1, '/Users/test/Documents', 'Documents'),
      { ...favorite(2, '/Users/test/Downloads', 'Downloads'), favoriteShortcut: 'P' },
      DISK,
    ]
    await openFavorites()
    expect(menuRow('fav-2')?.querySelector('.menu-shortcut')?.textContent).toBe('P')
    expect(menuRow('fav-2')?.querySelector('.menu-shortcut .shortcut-chip')?.tagName).toBe('KBD')
    expect(menuRow('fav-2')?.getAttribute('aria-keyshortcuts')).toBe('2 P')
    press('p')
    await vi.waitFor(() => {
      expect(trackEvent).toHaveBeenCalledWith('favorite_opened', {
        surface: 'favorites_menu',
        via: 'letter',
        reach: 'ready',
      })
    })
  })

  it('reports Enter on the cursor as `keyboard`', async () => {
    await openFavorites()
    press('Enter')
    await vi.waitFor(() => {
      expect(trackEvent).toHaveBeenCalledWith('favorite_opened', {
        surface: 'favorites_menu',
        via: 'keyboard',
        reach: 'ready',
      })
    })
  })

  it('reports a click as `pointer`', async () => {
    await openFavorites()
    favoriteRows()[0].click()
    await vi.waitFor(() => {
      expect(trackEvent).toHaveBeenCalledWith('favorite_opened', {
        surface: 'favorites_menu',
        via: 'pointer',
        reach: 'ready',
      })
    })
  })
})

/** Keyboard reorder: ⌥↑/⌥↓ move the highlighted favorite and persist the settled order. */
describe('reordering with ⌥↑ / ⌥↓', () => {
  it('persists the moved order', async () => {
    await openFavorites()
    // Home jumps the cursor to the first favorite.
    expect(press('Home')).toBe(true)
    await tick()
    flushSync()

    expect(press('ArrowDown', { altKey: true })).toBe(true)
    await tick()
    flushSync()

    expect(reorderFavorites).toHaveBeenCalledTimes(1)
    expect(reorderFavorites).toHaveBeenCalledWith(['2', '1', '3'])
  })

  it('keeps moving the SAME favorite on a second press (optimistic local order, no stale-state race)', async () => {
    await openFavorites()
    press('Home')
    await tick()
    flushSync()

    press('ArrowDown', { altKey: true })
    await tick()
    flushSync()

    // Immediately, BEFORE any `volumes-changed` refresh (the mock store never updates).
    // It must compute against the optimistic order: fav-1 goes 1 → 2. Without the
    // local-first override it would re-read the stale store and wrongly emit ['2','1','3'].
    press('ArrowDown', { altKey: true })
    await tick()
    flushSync()

    expect(reorderFavorites).toHaveBeenCalledTimes(2)
    expect(reorderFavorites).toHaveBeenLastCalledWith(['2', '3', '1'])
  })

  it('is a no-op at the top', async () => {
    await openFavorites()
    press('Home')
    await tick()
    flushSync()
    expect(press('ArrowUp', { altKey: true })).toBe(true)
    await tick()
    flushSync()
    expect(reorderFavorites).not.toHaveBeenCalled()
  })

  it('does nothing on the add row, which sits in a section that does not reorder', async () => {
    await openFavorites()
    // End jumps to the last row: the `0` add row.
    expect(press('End')).toBe(true)
    await tick()
    flushSync()
    press('ArrowDown', { altKey: true })
    await tick()
    flushSync()
    expect(reorderFavorites).not.toHaveBeenCalled()
  })
})

/**
 * Pointer drag reorder, the other half of what the favorite's own tooltip promises
 * ("Drag to reorder, or ⌥↑/⌥↓").
 *
 * ❗ The test DOM has no layout engine, so the rows are given rects by hand: the drop
 * target is decided from where the pointer sits against the rows' MIDPOINTS, which the
 * menu surface measures. Pre-fix this passed wrongly — the surface never registered its
 * measurement, so every drag resolved to "no target" and dropped a row back where it
 * started.
 */
describe('reordering by dragging', () => {
  /** Gives each favorite row a 20px-tall rect, so their midpoints are 10, 30, 50. */
  function layOutRows(rows: HTMLElement[]): void {
    rows.forEach((row, index) => {
      const top = index * 20
      row.getBoundingClientRect = () =>
        ({ top, bottom: top + 20, height: 20, left: 0, right: 200, width: 200, x: 0, y: top }) as DOMRect
    })
  }

  function drag(row: HTMLElement, fromY: number, toY: number): void {
    row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientY: fromY }))
    window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientY: toY }))
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientY: toY }))
  }

  it('drops the grabbed favorite where the pointer left it', async () => {
    await openFavorites()
    const rows = favoriteRows()
    layOutRows(rows)

    // Grab the first favorite and let go below the last row's midpoint: it lands last.
    drag(rows[0], 10, 55)
    await tick()
    flushSync()

    expect(reorderFavorites).toHaveBeenCalledWith(['2', '3', '1'])
  })

  it('shows the drop-line cue at the gap the row would land in', async () => {
    await openFavorites()
    const rows = favoriteRows()
    layOutRows(rows)

    rows[0].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientY: 10 }))
    window.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientY: 55 }))
    await tick()
    flushSync()

    // Slot 3 is the gap below the last of three rows, so the cue sits under it.
    const cued = document.querySelector('[data-drop-cue]')
    expect(cued?.getAttribute('data-drop-slot')).toBe('3')
    expect(favoriteRows()[0].hasAttribute('data-dragging')).toBe(true)

    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientY: 55 }))
    await tick()
    flushSync()
    expect(document.querySelector('[data-drop-cue]')).toBeNull()
  })

  it('leaves a drag that ends where it started alone', async () => {
    await openFavorites()
    const rows = favoriteRows()
    layOutRows(rows)

    // Past the 4px threshold, but still inside its own slot.
    drag(rows[1], 30, 38)
    await tick()
    flushSync()
    expect(reorderFavorites).not.toHaveBeenCalled()
  })

  it('treats a press that never travels as a plain click, and opens the favorite', async () => {
    const onVolumeChange = vi.fn()
    await openFavorites({ onVolumeChange })
    const rows = favoriteRows()
    layOutRows(rows)

    rows[1].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientY: 30 }))
    window.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientY: 30 }))
    await vi.waitFor(() => {
      expect(onVolumeChange).toHaveBeenCalledWith(expect.objectContaining({ targetPath: '/Users/test/Downloads' }))
    })
    expect(reorderFavorites).not.toHaveBeenCalled()
  })
})

/**
 * Renaming inline. While the editor is up it owns every keystroke, and four guards hold
 * that line — removing any one of them reopens the leak into the pane behind.
 */
describe('renaming a favorite', () => {
  async function startRename(): Promise<HTMLInputElement> {
    // The way a person does it: the row's submenu, then Rename. The menu stays up, since
    // the rename happens in the row.
    await openFavoriteSubmenu(0)
    submenuRow('row:fav-1:rename-favorite')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    flushSync()
    await tick()
    const input = document.querySelector<HTMLInputElement>('.favorite-rename-input')
    expect(input).toBeTruthy()
    return input as HTMLInputElement
  }

  it('claims no key while no menu is open', () => {
    mountBreadcrumb()
    expect(press('ArrowDown')).toBe(false)
  })

  it('consumes ArrowDown when the menu is open and nothing is being renamed', async () => {
    const { instance } = await openFavorites()
    expect(instance.isHeaderMenuOpen()).toBe(true)
    expect(press('ArrowDown')).toBe(true)
  })

  it('does NOT consume ArrowDown / ArrowUp / Home / End while a rename is active', async () => {
    await openFavorites()
    await startRename()
    // The keys the menu would otherwise eat must fall through, so the textbox keeps them.
    for (const key of ['ArrowDown', 'ArrowUp', 'Home', 'End']) {
      expect(press(key)).toBe(false)
    }
  })

  it('stops EVERY key (Space included) from bubbling out of the rename input to the pane', async () => {
    await openFavorites()
    const input = await startRename()

    // A document listener stands in for the pane's Space-selection / type-to-jump DOM
    // listeners. Without this guard, a Space typed into the box also selects the file
    // under the cursor.
    const leaked: string[] = []
    const docListener = (e: KeyboardEvent) => leaked.push(e.key)
    document.addEventListener('keydown', docListener)
    try {
      for (const key of [' ', 'a', 'ArrowDown', 'Backspace']) {
        input.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
      }
      await tick()
      flushSync()
      expect(leaked).toEqual([])
    } finally {
      document.removeEventListener('keydown', docListener)
    }
  })

  it('commits on Enter and writes the new name', async () => {
    await openFavorites()
    const input = await startRename()
    input.value = 'Papers'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    flushSync()

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(renameFavorite).toHaveBeenCalledWith('1', 'Papers')
    })
  })

  it('cancels on Escape without writing, and leaves the menu open', async () => {
    await openFavorites()
    const input = await startRename()
    input.value = 'Papers'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    flushSync()

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    await tick()
    flushSync()
    expect(renameFavorite).not.toHaveBeenCalled()
    expect(document.querySelector('.favorite-rename-input')).toBeNull()
    // ❗ Escape backed out of the EDITOR, not out of the menu: the guard stops the key
    // before the menu's own Escape ever sees it.
    expect(menuSurface()).toBeTruthy()
  })
})

/**
 * A favorite's actions (Rename, Set shortcut, Remove from favorites) sit in its → submenu, and a
 * right-click opens that same submenu: two doors, one list.
 */
describe('a favorite’s submenu', () => {
  it('captures a letter, lets Delete clear it, and leaves Escape as cancel', async () => {
    await openFavorites()
    await openFavoriteSubmenu(1)
    submenuRow('row:fav-2:edit-favorite-shortcut')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    flushSync()
    const input = document.querySelector<HTMLInputElement>('.favorite-shortcut-input')
    expect(input).toBeTruthy()
    input?.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', metaKey: true, bubbles: true, cancelable: true }))
    expect(setFavoriteShortcut).not.toHaveBeenCalled()
    input?.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(setFavoriteShortcut).toHaveBeenCalledWith('2', 'P')
    })
    expect(menuRow('fav-2')?.querySelector('.menu-shortcut')?.textContent).toBe('P')

    await openFavoriteSubmenu(1)
    submenuRow('row:fav-2:edit-favorite-shortcut')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    document
      .querySelector<HTMLInputElement>('.favorite-shortcut-input')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    expect(setFavoriteShortcut).toHaveBeenCalledTimes(1)
    expect(menuSurface()).toBeTruthy()

    await openFavoriteSubmenu(1)
    submenuRow('row:fav-2:edit-favorite-shortcut')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    document
      .querySelector<HTMLInputElement>('.favorite-shortcut-input')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(setFavoriteShortcut).toHaveBeenCalledWith('2', null)
    })
  })

  it('opens the right-clicked row’s submenu, not the one where the keyboard cursor sits', async () => {
    await openFavorites()

    // Park the cursor at the far end of the list (the `0` add row).
    press('End')
    await tick()
    flushSync()
    const rows = menuRows()
    expect(isHighlighted(rows[rows.length - 1])).toBe(true)

    await openFavoriteSubmenu(0)
    expect(submenuRow('row:fav-1:rename-favorite')?.textContent).toContain('Rename')
    expect(submenuRow('row:fav-1:remove-favorite')).toBeTruthy()
  })

  it('opens with → on the highlighted favorite, cursor on its first row', async () => {
    await openFavorites()
    press('ArrowDown')
    await tick()
    press('ArrowRight')
    await tick()
    flushSync()
    await tick()
    await tick()
    expect(isHighlighted(submenuRow('row:fav-2:rename-favorite'))).toBe(true)
  })

  it('removes the favorite, and leaves the menu up for the next one', async () => {
    await openFavorites()
    await openFavoriteSubmenu(1)
    submenuRow('row:fav-2:remove-favorite')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await vi.waitFor(() => {
      expect(removeFavorite).toHaveBeenCalledWith('2')
    })
    expect(openMenuName()).toBe(FAVORITES)
  })
})

describe('the empty list', () => {
  it('still reads as a favorites menu: the placeholder, and the way out of it', async () => {
    stubs.volumes = [DISK]
    await openFavorites()

    expect(document.querySelector('[data-menu-empty]')?.textContent).toContain('(Your favorites will show here)')
    expect(menuRow('favorites:add')?.hasAttribute('data-disabled')).toBe(false)
    // The placeholder is not a row: the cursor opens on the add row, the only thing to do.
    expect(isHighlighted(menuRow('favorites:add'))).toBe(true)
    expect(press('ArrowDown')).toBe(true)
    await tick()
    flushSync()
    expect(isHighlighted(menuRow('favorites:add'))).toBe(true)
  })
})
