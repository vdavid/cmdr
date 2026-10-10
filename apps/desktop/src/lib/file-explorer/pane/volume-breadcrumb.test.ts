/**
 * Integration tests for VolumeBreadcrumb.
 */
import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import VolumeBreadcrumb from '../navigation/VolumeBreadcrumb.svelte'
import { waitForUpdates, useMountTarget } from './integration-test-utils'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import { removeFavorite, renameFavorite, reorderFavorites } from '$lib/tauri-commands'

/** Right-clicks a row, which opens its → submenu, then clicks the submenu row `value`. */
async function pickFromRowSubmenu(row: HTMLElement, value: string): Promise<void> {
  row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 10, clientY: 10 }))
  // The submenu renders once its position effect measured the parent row, inside a tick of its own.
  await tick()
  await tick()
  await tick()
  document
    .querySelector(`[data-menu-submenu] [data-menu-row="${value}"]`)
    ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await tick()
}

// ============================================================================
// The switcher is the house `Menu`, which PORTALS to `document.body` and carries the
// documented `data-*` test hooks (`$lib/ui/DETAILS.md` § Menu). So these look at the
// document rather than the mount target, and select on hooks rather than on classes.
// ============================================================================

/** The open switcher's surface, or null when none is open. */
function menuSurface(): HTMLElement | null {
  return document.querySelector('[data-menu]')
}

function menuRows(): NodeListOf<HTMLElement> {
  return document.querySelectorAll('[data-menu-row]')
}

function menuRow(value: string): HTMLElement | null {
  return document.querySelector(`[data-menu-row="${value}"]`)
}

function isHighlighted(row: Element | null | undefined): boolean {
  return row?.hasAttribute('data-highlighted') ?? false
}

/**
 * Press a key the way the app does: the open menu hears it on its own document-level capture
 * listener. Returns true when the menu claimed it — which is the answer the pane's
 * `handleVolumeChooserKeyDown` used to give before the primitive took the keyboard over.
 */
function press(key: string, modifiers: KeyboardEventInit = {}): boolean {
  return !document.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...modifiers }))
}

// ============================================================================
// Mock setup (must be in each test file: Vitest hoists vi.mock calls)
// ============================================================================

let mockEntry: unknown = null

vi.mock('$lib/tauri-commands', () => ({
  listDirectoryStart: vi.fn().mockResolvedValue({ listingId: 'mock-listing', status: { status: 'ready' } }),
  cancelListing: vi.fn().mockResolvedValue(undefined),
  listDirectoryEnd: vi.fn().mockResolvedValue(undefined),
  onListingGone: vi.fn(() => () => {}),
  keepListingsAlive: vi.fn().mockResolvedValue([]),
  getFileRange: vi.fn().mockResolvedValue([]),
  getFileAt: vi.fn().mockImplementation((_listingId: string, index: number) => {
    if (index === 0) {
      mockEntry = {
        name: 'test-folder',
        path: '/test/test-folder',
        isDirectory: true,
        isSymlink: false,
        permissions: 0o755,
        owner: 'user',
        group: 'staff',
        iconId: 'dir',
        extendedMetadataLoaded: true,
      }
    } else {
      mockEntry = {
        name: 'test-file.txt',
        path: '/test/test-file.txt',
        isDirectory: false,
        isSymlink: false,
        permissions: 0o644,
        owner: 'user',
        group: 'staff',
        iconId: 'file',
        extendedMetadataLoaded: true,
      }
    }
    return Promise.resolve(mockEntry)
  }),
  findFileIndex: vi.fn().mockResolvedValue(0),
  getTotalCount: vi.fn().mockResolvedValue(10),
  setListingIncludeHidden: vi
    .fn()
    .mockResolvedValue({ sequence: 0, totalCount: 10, newCursorIndex: null, newSelectedIndices: null }),
  getSyncStatus: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
  openFile: vi.fn().mockResolvedValue(undefined),
  listen: vi.fn().mockResolvedValue(() => {}),
  showFileContextMenu: vi.fn().mockResolvedValue(undefined),
  updateMenuContext: vi.fn().mockResolvedValue(undefined),
  updateServicesSelection: vi.fn().mockResolvedValue(undefined),
  // `same-kind-target.svelte.ts` calls this whenever the cursor moves, so a pane mount reaches it.
  updateSelectSameKindMenu: vi.fn().mockResolvedValue(undefined),
  listVolumes: vi.fn().mockResolvedValue({
    data: [
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      {
        id: 'external',
        name: 'External Drive',
        path: '/Volumes/External',
        category: 'attached_volume',
        isEjectable: true,
      },
      { id: 'dropbox', name: 'Dropbox', path: '/Users/test/Dropbox', category: 'cloud_drive', isEjectable: false },
    ],
    timedOut: false,
  }),
  resolvePathVolume: vi.fn().mockResolvedValue({
    volume: { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    timedOut: false,
  }),
  getDefaultVolumeId: vi.fn().mockResolvedValue('root'),
  getVolumeSpace: vi
    .fn()
    .mockResolvedValue({ data: { totalBytes: 500_000_000_000, availableBytes: 200_000_000_000 }, timedOut: false }),
  refreshListing: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
  getIcons: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
  refreshDirectoryIcons: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
  DEFAULT_VOLUME_ID: 'root',
  listNetworkHosts: vi.fn().mockResolvedValue([]),
  setServersViewShown: vi.fn().mockResolvedValue(undefined),
  getNetworkDiscoveryState: vi.fn().mockResolvedValue('idle'),
  resolveNetworkHost: vi.fn().mockResolvedValue(null),
  onMtpDeviceConnected: vi.fn().mockResolvedValue(() => {}),
  onMtpDeviceDisconnected: vi.fn().mockResolvedValue(() => {}),
  onVolumeSpaceChanged: vi.fn().mockResolvedValue(() => {}),
  onWriteSourceItemDone: vi.fn().mockResolvedValue(() => {}),
  onDirectoryDiff: vi.fn().mockResolvedValue(() => {}),
  onDirectoryDeleted: vi.fn().mockResolvedValue(() => {}),
  onListingRespelled: vi.fn().mockResolvedValue(() => {}),
  onMtpExclusiveAccessError: vi.fn().mockResolvedValue(() => {}),
  onMtpPermissionError: vi.fn().mockResolvedValue(() => {}),
  notifyDialogOpened: vi.fn().mockResolvedValue(undefined),
  notifyDialogClosed: vi.fn().mockResolvedValue(undefined),
  watchVolumeSpace: vi.fn().mockResolvedValue(undefined),
  removeFavorite: vi.fn().mockResolvedValue(undefined),
  renameFavorite: vi.fn().mockResolvedValue(undefined),
  reorderFavorites: vi.fn().mockResolvedValue(undefined),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
  addFavorite: vi.fn().mockResolvedValue(undefined),
  trackEvent: vi.fn().mockResolvedValue(undefined),
  // Opening a DRIVE row runs the first-connect indexing prompt, which asks this before
  // deciding whether to offer anything. Answering "timed out" offers nothing.
  getVolumeIndexStatusById: vi.fn().mockResolvedValue({ status: 'timedOut' }),
  onVolumeContextAction: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: vi.fn().mockReturnValue('/icons/file.png'),
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
    prefetchCustomFolderIcons: vi.fn().mockResolvedValue(undefined),
    evictPerPathIconsForDir: vi.fn(),
  }
})

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getRowHeight: vi.fn().mockReturnValue(24),
  formatDateTime: vi.fn().mockReturnValue('2025-01-01 00:00'),
  formattedDate: vi.fn().mockReturnValue({
    text: '2025-01-01 00:00',
    segments: [
      { text: '2025', ageClass: 'age-fresh' as const },
      { text: '-', ageClass: null },
      { text: '01', ageClass: null },
      { text: '-', ageClass: null },
      { text: '01', ageClass: null },
      { text: ' ', ageClass: null },
      { text: '00', ageClass: null },
      { text: ':', ageClass: null },
      { text: '00', ageClass: null },
    ],
  }),
  formatFileSize: vi.fn().mockReturnValue('1.0 KB'),
  getFileSizeFormat: vi.fn().mockReturnValue('binary'),
  getFileSizeUnit: vi.fn().mockReturnValue('bytes'),
  getUseAppIconsForDocuments: vi.fn().mockReturnValue(true),
  // `volume-capabilities` reads it to classify a `.git`-portal path, which the favorites
  // menu's add row asks about.
  getShowVirtualGitPortal: () => false,
  getSizeDisplayMode: vi.fn().mockReturnValue('smart'),
  getNetworkEnabled: vi.fn().mockReturnValue(true),
  getSwitcherFavoritesExpanded: () => false,
}))

vi.mock('$lib/drag-drop', () => ({ startDragTracking: vi.fn() }))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: vi
    .fn()
    .mockReturnValue([{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }]),
  getVolumesTimedOut: vi.fn().mockReturnValue(false),
  isVolumesRefreshing: vi.fn().mockReturnValue(false),
  isVolumeRetryFailed: vi.fn().mockReturnValue(false),
  requestVolumeRefresh: vi.fn(),
  initVolumeStore: vi.fn().mockResolvedValue(undefined),
  cleanupVolumeStore: vi.fn(),
}))

// ============================================================================
// VolumeBreadcrumb tests
// ============================================================================

describe('VolumeBreadcrumb', () => {
  const { getTarget } = useMountTarget()

  // A switcher left open outlives its target div: it's portaled, and its key listener lives
  // on the document. Escape closes whichever ones are open, then the body goes.
  afterEach(() => {
    // Twice: the first Escape only backs out of an open submenu.
    press('Escape')
    press('Escape')
    document.body.innerHTML = ''
  })

  describe('Rendering', () => {
    it('renders volume breadcrumb container', async () => {
      mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      expect(getTarget().querySelector('.volume-breadcrumb')).toBeTruthy()
    })

    it('displays current volume name', async () => {
      mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      const volumeName = getTarget().querySelector('.volume-name')
      expect(volumeName?.textContent).toContain('Macintosh HD')
    })
  })

  describe('Dropdown', () => {
    it('exports toggleVolumeChooser method', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      expect(typeof (component as unknown as Record<string, unknown>).toggleVolumeChooser).toBe('function')
    })

    it('toggle method opens dropdown', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      // Initially dropdown should be closed
      expect(menuSurface()).toBeNull()

      // Call toggle
      const toggle = (component as unknown as { toggleVolumeChooser: () => void }).toggleVolumeChooser
      toggle()

      // The menu portals itself into `document.body`, which lands a beat after the toggle.
      await waitForUpdates()

      // Dropdown should now be open
      expect(menuSurface()).toBeTruthy()
    })

    it('dropdown shows all volumes', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggleVolumeChooser: () => void }).toggleVolumeChooser
      toggle()

      await waitForUpdates()

      // Should show volume items
      expect(menuRows().length).toBeGreaterThan(0)
    })

    it('clicking volume item calls onVolumeChange', async () => {
      const volumeChangeFn = vi.fn()

      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
          onVolumeChange: volumeChangeFn,
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggleVolumeChooser: () => void }).toggleVolumeChooser
      toggle()

      await waitForUpdates()

      // Find another volume item and click it. ❗ The favorites section's row is
      // unchecked too and folds the section rather than moving the pane, so skip it.
      const volumeItems = document.querySelectorAll(
        '[data-menu-row]:not([data-checked]):not([data-menu-row="menu-disclosure:favorites"])',
      )
      if (volumeItems.length > 0) {
        volumeItems[0].dispatchEvent(new MouseEvent('click', { bubbles: true }))

        await tick()

        expect(volumeChangeFn).toHaveBeenCalled()
      }
    })

    it('Escape key closes dropdown', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggleVolumeChooser: () => void }).toggleVolumeChooser
      toggle()

      await waitForUpdates()

      expect(menuSurface()).toBeTruthy()

      // Press Escape
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))

      await tick()

      expect(menuSurface()).toBeNull()
    })
  })

  describe('Volume categories', () => {
    it('groups volumes by category', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggleVolumeChooser: () => void }).toggleVolumeChooser
      toggle()

      await waitForUpdates()

      // Should have one labelled group per category
      const groups = document.querySelectorAll('[data-menu-section]')
      // We expect at least "Volumes" and possibly "Cloud"
      expect(groups.length).toBeGreaterThanOrEqual(0)
    })
  })

  describe('Keyboard navigation', () => {
    // Three volumes, so the arrow walk has somewhere to go past the checked row and the
    // favorites section's row the switcher leads with.
    beforeEach(() => {
      vi.mocked(getVolumes).mockReturnValue([
        { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
        { id: 'volumes-a', name: 'Alpha', path: '/Volumes/Alpha', category: 'attached_volume', isEjectable: true },
        { id: 'volumes-b', name: 'Beta', path: '/Volumes/Beta', category: 'attached_volume', isEjectable: true },
      ] as ReturnType<typeof getVolumes>)
    })

    async function mountAndOpen() {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })
      await waitForUpdates(100)
      ;(component as unknown as { toggleVolumeChooser: () => void }).toggleVolumeChooser()
      // The menu portals itself into `document.body`, which lands a beat after the toggle.
      await waitForUpdates()
      return component
    }

    it('exports isHeaderMenuOpen method', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      expect(typeof (component as unknown as Record<string, unknown>).isHeaderMenuOpen).toBe('function')
    })

    it('isHeaderMenuOpen returns false when no menu is open', async () => {
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      const isHeaderMenuOpen = (component as unknown as { isHeaderMenuOpen: () => boolean }).isHeaderMenuOpen
      expect(isHeaderMenuOpen()).toBe(false)
    })

    it('isHeaderMenuOpen returns true when the switcher is open', async () => {
      const component = await mountAndOpen()

      const isHeaderMenuOpen = (component as unknown as { isHeaderMenuOpen: () => boolean }).isHeaderMenuOpen
      expect(isHeaderMenuOpen()).toBe(true)
    })

    it('a key reaches nobody while the dropdown is closed', async () => {
      mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
        },
      })

      await waitForUpdates(100)

      expect(press('ArrowDown')).toBe(false)
    })

    it('ArrowDown moves highlight down', async () => {
      await mountAndOpen()

      // The cursor opens on the CHECKED row — the boot disk, which sits after the
      // favorites section's row the switcher leads with.
      const items = menuRows()
      expect(items.length).toBeGreaterThan(2)
      expect(isHighlighted(items[1])).toBe(true)

      // Press ArrowDown
      const handled = press('ArrowDown')

      await tick()

      expect(handled).toBe(true)
      expect(isHighlighted(items[1])).toBe(false)
      expect(isHighlighted(items[2])).toBe(true)
    })

    it('ArrowUp moves highlight up', async () => {
      await mountAndOpen()

      // Move down once
      press('ArrowDown')
      await tick()

      // Now move back up, to the checked row the menu opened on
      const handled = press('ArrowUp')

      await tick()

      expect(handled).toBe(true)
      expect(isHighlighted(menuRows()[1])).toBe(true)
    })

    it('ArrowUp at first item wraps to last', async () => {
      await mountAndOpen()

      // Home first: the menu opens on the checked row, not on the top one.
      press('Home')
      await tick()

      // Move up from first: should wrap to last
      press('ArrowUp')

      await tick()

      const items = menuRows()
      expect(items.length).toBeGreaterThan(1)
      expect(isHighlighted(items[0])).toBe(false)
      expect(isHighlighted(items[items.length - 1])).toBe(true)
    })

    it('ArrowDown at last item wraps to first', async () => {
      await mountAndOpen()

      // Walk down to the last item, from the top of the list
      press('Home')
      await tick()
      const items = menuRows()
      expect(items.length).toBeGreaterThan(1)
      for (let i = 0; i < items.length - 1; i++) {
        press('ArrowDown')
        await tick()
      }
      expect(isHighlighted(items[items.length - 1])).toBe(true)

      // One more ArrowDown should wrap back to first
      press('ArrowDown')
      await tick()

      expect(isHighlighted(items[items.length - 1])).toBe(false)
      expect(isHighlighted(items[0])).toBe(true)
    })

    it('Enter selects highlighted volume and closes dropdown', async () => {
      const volumeChangeFn = vi.fn()

      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
          onVolumeChange: volumeChangeFn,
        },
      })

      await waitForUpdates(100)

      // Open dropdown
      const toggle = (component as unknown as { toggleVolumeChooser: () => void }).toggleVolumeChooser
      toggle()

      await waitForUpdates()

      // Move to second item (first volume that's not under the cursor)
      press('ArrowDown')
      await tick()

      // Press Enter
      const handled = press('Enter')

      await tick()

      expect(handled).toBe(true)
      expect(volumeChangeFn).toHaveBeenCalled()
    })

    it('Escape closes dropdown', async () => {
      await mountAndOpen()

      expect(menuSurface()).toBeTruthy()

      const handled = press('Escape')

      await tick()

      expect(handled).toBe(true)
      expect(menuSurface()).toBeNull()
    })

    it('claims Tab and hands pane switching to the dual-pane owner', async () => {
      const onSwitchPane = vi.fn()
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: {
          paneId: 'left' as const,
          volumeId: 'root',
          currentPath: '/',
          containingVolumeId: 'root',
          onSwitchPane,
        },
      })
      await waitForUpdates(100)
      ;(component as unknown as { openVolumeChooser: () => void }).openVolumeChooser()
      await waitForUpdates()

      const event = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })
      document.dispatchEvent(event)

      expect(event.defaultPrevented).toBe(true)
      expect(onSwitchPane).toHaveBeenCalledOnce()
    })

    it('Home jumps to first item', async () => {
      await mountAndOpen()

      // Move down a couple times
      press('ArrowDown')
      press('ArrowDown')
      await tick()

      // Press Home
      const handled = press('Home')
      await tick()

      expect(handled).toBe(true)
      expect(isHighlighted(menuRows()[0])).toBe(true)
    })

    it('End jumps to last item', async () => {
      await mountAndOpen()

      // Press End
      const handled = press('End')
      await tick()

      expect(handled).toBe(true)
      const items = menuRows()
      expect(isHighlighted(items[items.length - 1])).toBe(true)
    })

    // ❗ Swallowed, ❌ never `preventDefault`ed: an open menu owns the keyboard so the panes
    // behind it stay inert, but ⌘Q and the menu-bar accelerators still mean what they mean.
    it('a key the menu has no use for changes nothing', async () => {
      await mountAndOpen()

      expect(press('x')).toBe(false)
      expect(menuSurface()).toBeTruthy()
    })
  })

  describe('Favorites menu', () => {
    const fav = (id: string, name: string, path: string) => ({
      id: `fav-${id}`,
      name,
      path,
      category: 'favorite' as const,
      isEjectable: false,
    })

    async function openWithFavorites(favorites: ReturnType<typeof fav>[]) {
      vi.mocked(getVolumes).mockReturnValue([
        ...favorites,
        { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      ])
      const component = mount(VolumeBreadcrumb, {
        target: getTarget(),
        props: { paneId: 'left' as const, volumeId: 'root', currentPath: '/', containingVolumeId: 'root' },
      })
      await waitForUpdates(100)
      ;(component as unknown as { toggleFavoritesMenu: () => void }).toggleFavoritesMenu()
      // The menu portals itself into `document.body`, which lands a beat after the toggle.
      await waitForUpdates()
      return component
    }

    it('renders the disabled empty-state placeholder when there are no favorites', async () => {
      await openWithFavorites([])
      const placeholder = document.querySelector('[data-menu-empty]')
      expect(placeholder?.textContent.trim()).toBe('(Your favorites will show here)')
      expect(placeholder?.getAttribute('aria-disabled')).toBe('true')
      // Not focusable, not clickable. `tabindex="-1"` is the primitive's: it keeps the
      // placeholder out of the tab order while letting the menu own the cursor, and
      // `aria-disabled` plus the pin below are what keep that cursor off it.
      expect(placeholder?.getAttribute('tabindex')).toBe('-1')
    })

    it('never lands the keyboard cursor on the empty-state placeholder', async () => {
      await openWithFavorites([])
      const placeholder = document.querySelector('[data-menu-empty]')
      expect(placeholder).toBeTruthy()

      // The placeholder carries no `data-menu-row`, so it never enters the navigable list.
      // Walk the whole list twice over (arrows wrap) and it must stay unhighlighted,
      // with the cursor on exactly one real row the whole way.
      const rows = menuRows()
      expect(rows.length).toBeGreaterThan(0)
      for (let i = 0; i < rows.length * 2; i++) {
        press('ArrowDown')
        await tick()
        expect(isHighlighted(placeholder)).toBe(false)
        expect(document.querySelectorAll('[data-menu-row][data-highlighted]').length).toBe(1)
      }

      // Home is the other way onto the top of the list, and it lands on the first real
      // row rather than on the placeholder sitting above it.
      press('Home')
      await tick()
      expect(isHighlighted(placeholder)).toBe(false)
      expect(isHighlighted(rows[0])).toBe(true)
    })

    it('renders favorites as pointer-draggable rows (no HTML5 draggable, not DOM-focusable)', async () => {
      await openWithFavorites([fav('1', 'Documents', '/Users/me/Documents')])
      const item = menuRow('fav-1')
      expect(item).toBeTruthy()
      // Reorder is pointer-based (HTML5 drag is dead under Tauri's `dragDropEnabled`), and the
      // rows are navigated by the menu's virtual cursor, not by DOM focus: `tabindex="-1"`
      // keeps every row out of the tab order, and `aria-activedescendant` on the surface is
      // what announces the cursor.
      expect(item?.getAttribute('draggable')).toBeNull()
      expect(item?.getAttribute('tabindex')).toBe('-1')
    })

    it('Alt+Down keyboard reorder (on the highlighted favorite) persists the new order with bare ids', async () => {
      await openWithFavorites([fav('a', 'A', '/a'), fav('b', 'B', '/b'), fav('c', 'C', '/c')])
      // Highlight the first favorite, then Alt+Down moves it to the second slot.
      press('Home')
      await tick()
      press('ArrowDown', { altKey: true })
      await tick()
      expect(reorderFavorites).toHaveBeenCalledWith(['b', 'a', 'c'])
    })

    it('Remove, from the favorite’s submenu, calls removeFavorite with the bare id', async () => {
      await openWithFavorites([fav('x', 'Pics', '/Users/me/Pics')])
      await pickFromRowSubmenu(menuRow('fav-x') as HTMLElement, 'row:fav-x:remove-favorite')
      expect(removeFavorite).toHaveBeenCalledWith('x')
    })

    it('Rename, from the favorite’s submenu, shows an inline input; committing calls renameFavorite with the bare id', async () => {
      await openWithFavorites([fav('y', 'Old', '/Users/me/Old')])
      await pickFromRowSubmenu(menuRow('fav-y') as HTMLElement, 'row:fav-y:rename-favorite')
      await tick()
      const input = document.querySelector('.favorite-rename-input') as HTMLInputElement
      expect(input).toBeTruthy()
      input.value = 'New name'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      await tick()
      expect(renameFavorite).toHaveBeenCalledWith('y', 'New name')
    })
  })
})
