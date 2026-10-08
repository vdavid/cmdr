/**
 * Tier 3 a11y tests for `TabBar.svelte`.
 *
 * Tab strip at the top of each pane. Tests cover single tab, multiple
 * tabs, and pinned tabs.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TabBar from './TabBar.svelte'
import TabDragOverlay from './TabDragOverlay.svelte'
import { createTabDragController, type TabDragView } from './tab-drag-controller.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { TabState } from './tab-types'

const noop = () => {}

/** A drag face that never drags: these tests are about the bar at rest. */
const inertDrag = () => createTabDragController({ getTabs: () => [], maxTabs: 10, onDrop: noop }).forPane('left')

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
        drag: inertDrag(),
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
        drag: inertDrag(),
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
        drag: inertDrag(),
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
        drag: inertDrag(),
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
    // Click the .tab-list element directly (not on any child .tab); the trailing
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

describe('TabDragOverlay', () => {
  const view = (overrides: Partial<TabDragView> = {}): TabDragView => ({
    fromPane: 'left',
    tabId: 't1',
    label: 'Documents',
    ghost: { left: 40, top: 0, width: 120, height: 28 },
    overPane: 'left',
    line: { left: 207.5, top: 0, height: 28 },
    refused: false,
    ...overrides,
  })

  function mountOverlay(props: { view: TabDragView | null }): HTMLElement {
    document.body.innerHTML = ''
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TabDragOverlay, { target, props })
    return target
  }

  it('draws nothing while no tab is being dragged', () => {
    expect(mountOverlay({ view: null }).children).toHaveLength(0)
  })

  it('draws the ghost and the landing line where the controller put them', () => {
    const target = mountOverlay({ view: view() })
    const ghost = target.querySelector<HTMLElement>('.ghost')
    const line = target.querySelector<HTMLElement>('.drop-line')

    expect(ghost?.textContent.trim()).toBe('Documents')
    expect(ghost?.style.left).toBe('40px')
    expect(ghost?.style.width).toBe('120px')
    expect(line?.style.left).toBe('207.5px')
    expect(ghost?.classList.contains('cannot-drop')).toBe(false)
  })

  it('draws no line, and the refusal cursor, over a bar that refuses the tab', () => {
    const target = mountOverlay({ view: view({ overPane: 'right', line: null, refused: true }) })

    expect(target.querySelector('.drop-line')).toBeNull()
    expect(target.querySelector('.tab-drag-layer')?.classList.contains('refused')).toBe(true)
    expect(target.querySelector('.ghost')?.classList.contains('cannot-drop')).toBe(true)
  })

  it('fades the ghost off the bars, where a release cancels', () => {
    const target = mountOverlay({ view: view({ overPane: null, line: null }) })

    expect(target.querySelector('.ghost')?.classList.contains('cannot-drop')).toBe(true)
    expect(target.querySelector('.tab-drag-layer')?.classList.contains('refused')).toBe(false)
  })

  it('has no a11y violations mid-drag', async () => {
    const target = mountOverlay({ view: view() })
    await tick()
    await expectNoA11yViolations(target)
  })
})
