/**
 * Tier 3 a11y tests for `TabBar.svelte`.
 *
 * Tab strip at the top of each pane. Tests cover single tab, multiple
 * tabs, and pinned tabs.
 */

import { describe, it } from 'vitest'
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
