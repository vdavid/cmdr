import { beforeEach, describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import TabBar from './TabBar.svelte'
import { installLayoutMock } from '$lib/test-layout'
import type { TabState } from './tab-types'

/**
 * A tab too narrow to hold a label AND a close button drops the close button.
 * That used to be a `@container (max-width: 80px)` query, which needs Safari 16;
 * on the Safari 15 that macOS 12 ships, the whole block is dropped silently and
 * every tab keeps a close button that doesn't fit. These tests pin the
 * measured-in-JS replacement at the same threshold.
 */

const noop = () => {}

function makeTab(id: string, path: string): TabState {
  return {
    id,
    path,
    volumeId: 'root',
    history: { stack: [{ volumeId: 'root', path }], currentIndex: 0 },
    sortBy: 'name',
    sortOrder: 'ascending',
    viewMode: 'full',
    pinned: false,
    cursorFilename: null,
    unreachable: null,
  }
}

function mountTabBar(target: HTMLElement) {
  mount(TabBar, {
    target,
    props: {
      tabs: [makeTab('t1', '/Users/test/one'), makeTab('t2', '/Users/test/two')],
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
}

describe('TabBar narrow tabs', () => {
  let target: HTMLElement

  beforeEach(() => {
    document.body.innerHTML = ''
    target = document.createElement('div')
    document.body.appendChild(target)
  })

  it('keeps the close button on a comfortably wide tab', async () => {
    installLayoutMock({ '.tab': { clientWidth: 160 } })
    mountTabBar(target)
    await tick()

    expect(target.querySelectorAll('.tab.narrow')).toHaveLength(0)
  })

  it('marks a tab narrow once it is down to the threshold', async () => {
    const layout = installLayoutMock({ '.tab': { clientWidth: 160 } })
    mountTabBar(target)
    await tick()

    layout.resize('.tab', { clientWidth: 80 })
    await tick()

    expect(target.querySelectorAll('.tab.narrow')).toHaveLength(2)
  })

  it('gives the close button back when the tab grows again', async () => {
    const layout = installLayoutMock({ '.tab': { clientWidth: 40 } })
    mountTabBar(target)
    await tick()
    expect(target.querySelectorAll('.tab.narrow')).toHaveLength(2)

    layout.resize('.tab', { clientWidth: 160 })
    await tick()

    expect(target.querySelectorAll('.tab.narrow')).toHaveLength(0)
  })

  // The observer's first callback lands after the first paint, so an unmeasured
  // tab reads as 0 px wide. Treating that as "narrow" would blink every close
  // button out and back in on mount.
  it('treats an unmeasured tab as wide, not as zero-width', async () => {
    mountTabBar(target)
    await tick()

    expect(target.querySelectorAll('.tab')).toHaveLength(2)
    expect(target.querySelectorAll('.tab.narrow')).toHaveLength(0)
  })
})
