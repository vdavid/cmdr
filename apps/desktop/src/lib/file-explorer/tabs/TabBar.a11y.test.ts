/**
 * Tier 3 a11y tests for `TabBar.svelte`.
 *
 * Tab strip at the top of each pane. Tests cover single tab, multiple
 * tabs, and pinned tabs.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TabBar from './TabBar.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { TabState } from './tab-types'

const noop = () => {}

const makeTab = (id: string, path: string, pinned = false): TabState => ({
  id,
  path,
  volumeId: 'root',
  history: { stack: [{ volumeId: 'root', path }], currentIndex: 0 },
  sortBy: 'name',
  sortOrder: 'ascending',
  viewMode: 'full',
  pinned,
  cursorFilename: null,
  unreachable: null,
})

describe('TabBar a11y', () => {
  it('single tab has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TabBar, {
      target,
      props: {
        tabs: [makeTab('t1', '/Users/test')],
        activeTabId: 't1',
        paneId: 'left',
        maxTabs: 10,
        onTabSwitch: noop,
        onTabClose: noop,
        onTabMiddleClick: noop,
        onNewTab: noop,
        onContextMenu: noop,
        onPaneFocus: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('multiple tabs with pinned first has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TabBar, {
      target,
      props: {
        tabs: [
          makeTab('t1', '/Users/test/pinned', true),
          makeTab('t2', '/Users/test/Documents'),
          makeTab('t3', '/Users/test/Downloads'),
        ],
        activeTabId: 't2',
        paneId: 'left',
        maxTabs: 10,
        onTabSwitch: noop,
        onTabClose: noop,
        onTabMiddleClick: noop,
        onNewTab: noop,
        onContextMenu: noop,
        onPaneFocus: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('at max tabs (new-tab button disabled) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TabBar, {
      target,
      props: {
        tabs: Array.from({ length: 10 }, (_, i) => makeTab(`t${String(i)}`, `/path-${String(i)}`)),
        activeTabId: 't0',
        paneId: 'left',
        maxTabs: 10,
        onTabSwitch: noop,
        onTabClose: noop,
        onTabMiddleClick: noop,
        onNewTab: noop,
        onContextMenu: noop,
        onPaneFocus: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('TabBar double-click empty area', () => {
  /** Mounts a fresh TabBar with the given onNewTab spy and returns the target + element refs. */
  async function mountTabBar(onNewTab: () => void) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TabBar, {
      target,
      props: {
        tabs: [makeTab('t1', '/Users/test'), makeTab('t2', '/Users/test/Downloads')],
        activeTabId: 't1',
        paneId: 'left',
        maxTabs: 10,
        onTabSwitch: noop,
        onTabClose: noop,
        onTabMiddleClick: noop,
        onNewTab,
        onContextMenu: noop,
        onPaneFocus: noop,
      },
    })
    await tick()
    const bar = target.querySelector('.tab-bar') as HTMLElement
    return { target, bar }
  }

  it('dblclick on the empty .tab-bar padding fires onNewTab', async () => {
    const onNewTab = vi.fn()
    const { bar } = await mountTabBar(onNewTab)
    // The bar itself (not a child) is the empty padding/spacer surface.
    bar.dispatchEvent(new MouseEvent('dblclick', { button: 0, bubbles: true }))
    expect(onNewTab).toHaveBeenCalledTimes(1)
  })

  it('dblclick on the trailing flex space of .tab-list fires onNewTab', async () => {
    const onNewTab = vi.fn()
    const { target } = await mountTabBar(onNewTab)
    const tabList = target.querySelector('.tab-list') as HTMLElement
    // Click the .tab-list element directly (not on any child .tab) — the trailing
    // empty flex region is the .tab-list itself outside of any `.tab` button.
    tabList.dispatchEvent(new MouseEvent('dblclick', { button: 0, bubbles: true }))
    expect(onNewTab).toHaveBeenCalledTimes(1)
  })

  it('dblclick on a .tab does NOT fire onNewTab', async () => {
    const onNewTab = vi.fn()
    const { target } = await mountTabBar(onNewTab)
    const tab = target.querySelector('.tab') as HTMLElement
    tab.dispatchEvent(new MouseEvent('dblclick', { button: 0, bubbles: true }))
    expect(onNewTab).not.toHaveBeenCalled()
  })

  it('dblclick on .new-tab-btn does NOT fire onNewTab (avoids double-create)', async () => {
    const onNewTab = vi.fn()
    const { target } = await mountTabBar(onNewTab)
    const newTabBtn = target.querySelector('.new-tab-btn') as HTMLElement
    newTabBtn.dispatchEvent(new MouseEvent('dblclick', { button: 0, bubbles: true }))
    expect(onNewTab).not.toHaveBeenCalled()
  })

  it('dblclick on .close-btn does NOT fire onNewTab', async () => {
    const onNewTab = vi.fn()
    const { target } = await mountTabBar(onNewTab)
    const closeBtn = target.querySelector('.close-btn') as HTMLElement
    expect(closeBtn).not.toBeNull()
    closeBtn.dispatchEvent(new MouseEvent('dblclick', { button: 0, bubbles: true }))
    expect(onNewTab).not.toHaveBeenCalled()
  })
})
