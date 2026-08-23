/**
 * Tier 3 a11y tests for the Full view: the list and its column header.
 *
 * `svelte-tests` charges per test FILE, not per test (`docs/testing.md` § "What a
 * test actually costs"), so the two share a file. The header needs no stubs and is
 * unaffected by the five below: it mounts `SortableHeader` buttons and nothing that
 * reads IPC, the icon cache, index state, or either settings layer.
 *
 * `BriefList` stays in its own file on purpose. It stubs the same five modules with
 * DIFFERENT values — its own `getDirStatsBatch` shape, its own `getWalkedGround`,
 * and the two `brief.columnWidth*` settings — and reconciling five module stubs
 * would be forcing the merge, not making one.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import FullList from './FullList.svelte'
import FullListHeader from './FullListHeader.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { dirEntry, fileEntry, mountFullList } from './test-full-list'
import { installLayoutMock } from '$lib/test-layout'
import type { SortColumn, SortOrder } from '../types'

vi.mock('$lib/tauri-commands', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('$lib/indexing/index-state.svelte', async () =>
  (await import('./test-file-list-mocks')).indexStateMock({ getWalkedGround: () => ['/root/src'] }),
)
vi.mock('$lib/settings/reactive-settings.svelte', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock(),
)
vi.mock('$lib/settings/settings-store', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

// Both components share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `FullList.svelte`.
 *
 * Virtual-scrolling vertical file list with full metadata columns. Tests cover
 * the empty state (with a safe cursor) and a populated list. The populated cases
 * mount through `mountFullList` (`test-full-list.ts`) so axe sees real `option`
 * rows rather than an empty listbox.
 */
describe('FullList a11y', () => {
  /** An empty listing still gets a measured surface: the empty-state branch is
   *  about having no ENTRIES, not about having no room to show them. */
  async function mountEmpty(props: { cursorIndex: number; isFocused?: boolean }): Promise<HTMLElement> {
    installLayoutMock({ '[data-file-list-surface]': { clientHeight: 400, clientWidth: 800, offsetWidth: 800 } })
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FullList, {
      target,
      props: {
        listingId: 'l1',
        volumeId: 'root',
        totalCount: 0,
        includeHidden: false,
        isFocused: true,
        hasParent: false,
        parentPath: '',
        currentPath: '/root',
        sortBy: 'name',
        sortOrder: 'ascending',
        onSelect: () => {},
        onNavigate: () => {},
        ...props,
      },
    })
    await tick()
    return target
  }

  // Pins the `aria-activedescendant` gate: the cursor exists but no row is
  // rendered, so the attribute must be absent rather than name a missing id.
  it('empty folder with cursor at 0 has no a11y violations', async () => {
    const target = await mountEmpty({ cursorIndex: 0 })
    expect(target.querySelector('[role="listbox"]')?.getAttribute('aria-activedescendant')).toBeNull()
    await expectNoA11yViolations(target)
  })

  // Pins the empty-state text staying OUTSIDE the listbox: an empty listbox
  // passes `aria-required-children`, one holding a non-option child does not.
  it('empty folder with no cursor has no a11y violations', async () => {
    const target = await mountEmpty({ cursorIndex: -1 })
    expect(target.querySelector('.empty-folder-message')).toBeTruthy()
    await expectNoA11yViolations(target)
  })

  it('populated (parent row, a walked folder, and a file) has no a11y violations', async () => {
    const list = await mountFullList({
      entries: [dirEntry({ name: 'src' }), fileEntry({ name: 'report.md', iconId: 'ext:md', size: 2048 })],
      props: { hasParent: true, parentPath: '/root/..', totalCount: 3 },
    })
    // `..` plus the two entries, one of them wearing the size-updating hourglass
    // — so axe is checking the row chrome, not an empty listbox.
    expect(list.rowNames()).toEqual(['..', 'src', 'report.md'])
    expect(list.hourglassRowNames()).toEqual(['src'])
    await expectNoA11yViolations(list.target)
  })

  it('unfocused pane has no a11y violations', async () => {
    const list = await mountFullList({
      entries: [fileEntry({ name: 'report.md' })],
      props: { isFocused: false, cursorIndex: -1 },
    })
    expect(list.rowNames()).toEqual(['report.md'])
    await expectNoA11yViolations(list.target)
  })
})

/**
 * Tier 3 a11y tests for `FullListHeader.svelte`.
 *
 * The Full view's column header. It mounts with no Tauri, settings, or
 * icon-cache dependency, so unlike its `FullList` host it needs no stubs.
 *
 * Every branch that changes the DOM gets a case, because each one adds or moves a
 * focusable sort trigger: the default four columns, the `showExtensionInName` split
 * (two triggers sharing the Name track), the optional Git cell, and an unfocused
 * pane (`SortableHeader` styles its caret off `isFocused`).
 *
 * Deliberately NOT asserted here: the header carries no `role`. It sits inside
 * `FullList`'s `role="listbox"` scroll container, and `role="toolbar"` would violate
 * `aria-required-children` on the listbox. The composed header-inside-listbox tree is
 * what the `FullList` block above covers.
 */
describe('FullListHeader a11y', () => {
  interface HeaderProps {
    gridTemplate: string
    isFocused: boolean
    sortBy: SortColumn
    sortOrder: SortOrder
    showExtensionInName: boolean
    gitColumnVisible: boolean
    skipTransition: boolean
    scrollbarWidth: number
    onSortChange?: (column: SortColumn) => void
  }

  async function mountHeader(overrides: Partial<HeaderProps> = {}): Promise<HTMLElement> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FullListHeader, {
      target,
      props: {
        gridTemplate: '16px 1fr 60px 115px 80px',
        isFocused: true,
        sortBy: 'name',
        sortOrder: 'ascending',
        showExtensionInName: false,
        gitColumnVisible: false,
        skipTransition: false,
        scrollbarWidth: 0,
        onSortChange: vi.fn(),
        ...overrides,
      } satisfies HeaderProps,
    })
    await tick()
    return target
  }

  it('default four columns have no a11y violations', async () => {
    await expectNoA11yViolations(await mountHeader())
  })

  it('the split Name+Ext header has no a11y violations', async () => {
    // Two sort triggers share the single `1fr` Name track here, so both must
    // still be reachable and named.
    await expectNoA11yViolations(await mountHeader({ showExtensionInName: true, sortBy: 'extension' }))
  })

  it('the optional Git column has no a11y violations', async () => {
    // The Git cell is a label, not a trigger: it carries a `title` and no role.
    await expectNoA11yViolations(await mountHeader({ gitColumnVisible: true }))
  })

  it('an unfocused pane has no a11y violations', async () => {
    await expectNoA11yViolations(await mountHeader({ isFocused: false, sortBy: 'modified', sortOrder: 'descending' }))
  })

  it('every branch at once has no a11y violations', async () => {
    await expectNoA11yViolations(
      await mountHeader({ showExtensionInName: true, gitColumnVisible: true, skipTransition: true, sortBy: 'size' }),
    )
  })
})
