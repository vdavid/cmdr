import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import TabBar from './TabBar.svelte'
import { installLayoutMock } from '$lib/test-layout'
import { createTabDragController, type TabDragController } from './tab-drag-controller.svelte'
import type { TabState } from './tab-types'

/**
 * A tab too narrow to hold a label AND a close button drops the close button.
 * That used to be a `@container (max-width: 80px)` query, which needs Safari 16;
 * on the Safari 15 that macOS 12 ships, the whole block is dropped silently and
 * every tab keeps a close button that doesn't fit. These tests pin the
 * measured-in-JS replacement at the same threshold.
 */

const noop = () => {}

function makeTab(id: string, path: string, pinned = false): TabState {
  return {
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
  }
}

function dragControllerFor(tabs: TabState[]): TabDragController {
  return createTabDragController({ getTabs: () => tabs, maxTabs: 10, onDrop: noop })
}

interface MountOptions {
  tabs?: TabState[]
  controller?: TabDragController
  onTabSwitch?: (tabId: string) => void
  onPaneFocus?: () => void
}

function mountTabBar(target: HTMLElement, opts: MountOptions = {}) {
  const tabs = opts.tabs ?? [makeTab('t1', '/Users/test/one'), makeTab('t2', '/Users/test/two')]
  mount(TabBar, {
    target,
    props: {
      tabs,
      activeTabId: 't1',
      paneId: 'left',
      maxTabs: 10,
      drag: (opts.controller ?? dragControllerFor(tabs)).forPane('left'),
      onTabSwitch: opts.onTabSwitch ?? noop,
      onTabClose: noop,
      onTabMiddleClick: noop,
      onNewTab: noop,
      onContextMenu: noop,
      onPaneFocus: opts.onPaneFocus ?? noop,
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

/**
 * What the bar itself owes a tab drag: it hands presses to the controller, dims the tab
 * in flight, and stays quiet. The drag's own lifecycle is pinned in
 * `tab-drag-controller.svelte.test.ts`.
 */
describe('TabBar tab drag', () => {
  let target: HTMLElement
  let controller: TabDragController
  let tabs: TabState[]

  beforeEach(() => {
    vi.useFakeTimers()
    document.body.innerHTML = ''
    // Clear any hover suppression a previous test's keypress left on the tooltip module.
    document.dispatchEvent(new MouseEvent('mousemove'))
    target = document.createElement('div')
    document.body.appendChild(target)
    tabs = [makeTab('t1', '/Users/test/one'), makeTab('t2', '/Users/test/two'), makeTab('t3', '/Users/test/pin', true)]
    controller = dragControllerFor(tabs)
  })

  afterEach(() => {
    controller.destroy()
    vi.useRealTimers()
  })

  const tabEl = (index: number) => target.querySelectorAll<HTMLElement>('.tab')[index]

  function press(el: HTMLElement): void {
    el.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        button: 0,
        isPrimary: true,
        pointerId: 1,
        clientX: 10,
        clientY: 10,
      }),
    )
  }

  function moveTo(clientX: number): void {
    window.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, buttons: 1, clientX, clientY: 10 }))
  }

  function release(clientX: number): void {
    window.dispatchEvent(new PointerEvent('pointerup', { pointerId: 1, clientX, clientY: 10 }))
  }

  it('dims the tab being dragged, and only that one', async () => {
    mountTabBar(target, { tabs, controller })
    await tick()

    press(tabEl(1))
    moveTo(60)
    await tick()

    expect(tabEl(1).classList.contains('is-dragging')).toBe(true)
    expect(target.querySelectorAll('.tab.is-dragging')).toHaveLength(1)
    expect(controller.view).toMatchObject({ tabId: 't2', label: 'two' })
  })

  it('leaves a pressed tab alone until the pointer has travelled', async () => {
    mountTabBar(target, { tabs, controller })
    await tick()

    press(tabEl(1))
    moveTo(12)
    await tick()

    expect(target.querySelectorAll('.tab.is-dragging')).toHaveLength(0)
  })

  it('never drags a pinned tab', async () => {
    mountTabBar(target, { tabs, controller })
    await tick()

    press(tabEl(2))
    moveTo(60)
    await tick()

    expect(controller.view).toBeNull()
    expect(target.querySelectorAll('.tab.is-dragging')).toHaveLength(0)
  })

  it('switches tab on a plain click', async () => {
    const onTabSwitch = vi.fn()
    mountTabBar(target, { tabs, controller, onTabSwitch })
    await tick()

    press(tabEl(1))
    release(10)
    tabEl(1).click()

    expect(onTabSwitch).toHaveBeenCalledExactlyOnceWith('t2')
  })

  it('neither switches tab nor focuses the pane when the press was a drag', async () => {
    const onTabSwitch = vi.fn()
    const onPaneFocus = vi.fn()
    mountTabBar(target, { tabs, controller, onTabSwitch, onPaneFocus })
    await tick()

    press(tabEl(1))
    moveTo(60)
    release(60)
    tabEl(1).click()

    expect(onTabSwitch).not.toHaveBeenCalled()
    expect(onPaneFocus).not.toHaveBeenCalled()
  })

  it('shows a tab tooltip on hover when nothing is being dragged', async () => {
    mountTabBar(target, { tabs, controller })
    await tick()

    tabEl(0).dispatchEvent(new MouseEvent('mouseenter'))
    vi.advanceTimersByTime(500)

    expect(document.querySelector('.cmdr-tooltip.visible')?.textContent).toContain('/Users/test/one')
  })

  it('keeps tooltips quiet during a drag, even one whose delay had already started', async () => {
    mountTabBar(target, { tabs, controller })
    await tick()

    tabEl(1).dispatchEvent(new MouseEvent('mouseenter'))
    press(tabEl(1))
    moveTo(60)
    await tick()
    tabEl(0).dispatchEvent(new MouseEvent('mouseenter'))
    vi.advanceTimersByTime(500)

    expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
  })
})
