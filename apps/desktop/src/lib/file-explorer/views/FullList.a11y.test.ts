/**
 * Tier 3 a11y tests for `FullList.svelte`.
 *
 * Virtual-scrolling vertical file list with full metadata columns. Tests cover
 * the empty state (with a safe cursor) and a populated list. The populated cases
 * mount through `mountFullList` (`test-full-list.ts`) so axe sees real `option`
 * rows rather than an empty listbox.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FullList from './FullList.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { dirEntry, fileEntry, mountFullList } from './test-full-list'
import { installLayoutMock } from '$lib/test-layout'

vi.mock('$lib/tauri-commands', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('$lib/indexing/index-state.svelte', async () =>
  (await import('./test-file-list-mocks')).indexStateMock({ getWalkedGround: () => ['/root/src'] }),
)
vi.mock('$lib/settings/reactive-settings.svelte', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock(),
)
vi.mock('$lib/settings/settings-store', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

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

describe('FullList a11y', () => {
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
