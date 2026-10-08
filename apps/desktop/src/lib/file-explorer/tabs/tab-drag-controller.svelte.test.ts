/**
 * The tab drag's lifecycle against a real DOM: when a press becomes a drag, what each way
 * of ending it does, and what the overlay is told to draw. The slot arithmetic is pinned
 * in `tab-drop-slot.test.ts`, and the move's own rules in `tab-state-manager.test.ts`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createTabDragController, type TabDragController, type TabDrop } from './tab-drag-controller.svelte'
import type { TabState } from './tab-types'

type Pane = 'left' | 'right'

const BAR_HEIGHT = 30
const TAB_WIDTH = 100
const TAB_GAP = 5
/** Left edge of each pane's bar; a bar is 400 px wide. */
const BAR_LEFT: Record<Pane, number> = { left: 0, right: 500 }

function makeTab(id: string, pinned = false): TabState {
  return {
    id,
    path: `/Users/test/${id}`,
    volumeId: 'root',
    history: { stack: [{ volumeId: 'root', path: `/Users/test/${id}` }], currentIndex: 0 },
    sortBy: 'name',
    sortOrder: 'ascending',
    viewMode: 'full',
    pinned,
    cursorFilename: null,
    unreachable: null,
  }
}

function rect(left: number, width: number): DOMRect {
  return new DOMRect(left, 0, width, BAR_HEIGHT)
}

/** A bar with one `role="tab"` button per tab, laid out left to right like the real one. */
function buildBar(pane: Pane, tabs: TabState[]): { bar: HTMLElement; tabEls: Map<string, HTMLElement> } {
  const bar = document.createElement('div')
  bar.getBoundingClientRect = () => rect(BAR_LEFT[pane], 400)
  const tabEls = new Map<string, HTMLElement>()
  tabs.forEach((tab, index) => {
    const el = document.createElement('button')
    el.setAttribute('role', 'tab')
    el.getBoundingClientRect = () => rect(BAR_LEFT[pane] + index * (TAB_WIDTH + TAB_GAP), TAB_WIDTH)
    const close = document.createElement('span')
    close.className = 'close-btn'
    el.appendChild(close)
    bar.appendChild(el)
    tabEls.set(tab.id, el)
  })
  document.body.appendChild(bar)
  return { bar, tabEls }
}

interface Harness {
  controller: TabDragController
  drops: TabDrop[]
  /** Presses the primary button on a tab, 10 px in from its left edge. */
  press: (pane: Pane, tabId: string, init?: PointerEventInit & { onCloseButton?: boolean }) => void
  tabEl: (pane: Pane, tabId: string) => HTMLElement
}

function harness(tabs: Record<Pane, TabState[]>): Harness {
  const drops: TabDrop[] = []
  const controller = createTabDragController({
    getTabs: (pane) => tabs[pane],
    maxTabs: 10,
    onDrop: (drop) => drops.push(drop),
  })
  const built = { left: buildBar('left', tabs.left), right: buildBar('right', tabs.right) }
  for (const pane of ['left', 'right'] as const) controller.forPane(pane).attach(built[pane].bar)

  const tabEl = (pane: Pane, tabId: string): HTMLElement => {
    const el = built[pane].tabEls.get(tabId)
    if (!el) throw new Error(`no tab ${tabId} in the ${pane} bar`)
    return el
  }

  return {
    controller,
    drops,
    tabEl,
    press: (pane, tabId, init = {}) => {
      const el = tabEl(pane, tabId)
      const target = init.onCloseButton ? el.querySelector('.close-btn') : el
      const event = new PointerEvent('pointerdown', {
        bubbles: true,
        button: 0,
        isPrimary: true,
        pointerId: 1,
        clientX: el.getBoundingClientRect().left + 10,
        clientY: 15,
        ...init,
      })
      // `currentTarget` is the tab the handler sits on; `target` may be a child of it.
      el.addEventListener(
        'pointerdown',
        (e) => {
          controller.forPane(pane).press({ tabId, label: tabId, event: e })
        },
        {
          once: true,
        },
      )
      target?.dispatchEvent(event)
    },
  }
}

function pointer(type: 'pointermove' | 'pointerup' | 'pointercancel', clientX: number, clientY = 15): void {
  // A move during a drag has the primary button down; the release and the cancel don't.
  const buttons = type === 'pointermove' ? 1 : 0
  window.dispatchEvent(
    new PointerEvent(type, { bubbles: true, isPrimary: true, pointerId: 1, buttons, clientX, clientY }),
  )
}

/** Presses tab `tabId` and drags it to `x`, far enough to count as a drag. */
function dragTo(h: Harness, pane: Pane, tabId: string, x: number, y = 15): void {
  h.press(pane, tabId)
  pointer('pointermove', x, y)
}

describe('tab drag controller', () => {
  let h: Harness

  beforeEach(() => {
    vi.useFakeTimers()
    document.body.innerHTML = ''
    h = harness({
      left: [makeTab('a'), makeTab('b'), makeTab('c')],
      right: [makeTab('x'), makeTab('y')],
    })
  })

  afterEach(() => {
    h.controller.destroy()
    vi.useRealTimers()
  })

  describe('starting', () => {
    it('stays a click while the pointer moves less than the threshold', () => {
      h.press('left', 'a')
      pointer('pointermove', 13)
      expect(h.controller.view).toBeNull()

      pointer('pointerup', 13)
      expect(h.drops).toEqual([])
    })

    it("ignores a pane's only tab, which has nowhere to go", () => {
      h.controller.destroy()
      document.body.innerHTML = ''
      h = harness({ left: [makeTab('a')], right: [makeTab('x')] })

      dragTo(h, 'left', 'a', 560)
      expect(h.controller.view).toBeNull()
      pointer('pointerup', 560)
      expect(h.drops).toEqual([])
    })

    it('becomes a drag once the pointer has moved past the threshold', () => {
      dragTo(h, 'left', 'a', 16)
      expect(h.controller.view).toMatchObject({ fromPane: 'left', tabId: 'a', label: 'a' })
    })

    it('counts vertical travel toward the threshold too', () => {
      h.press('left', 'a')
      pointer('pointermove', 10, 22)
      expect(h.controller.view).not.toBeNull()
    })

    it('ignores a pinned tab', () => {
      h.controller.destroy()
      document.body.innerHTML = ''
      h = harness({ left: [makeTab('a', true), makeTab('b')], right: [makeTab('x')] })

      dragTo(h, 'left', 'a', 150)
      expect(h.controller.view).toBeNull()
      pointer('pointerup', 150)
      expect(h.drops).toEqual([])
    })

    it('ignores every button but the primary one', () => {
      h.press('left', 'a', { button: 2 })
      pointer('pointermove', 150)
      expect(h.controller.view).toBeNull()
    })

    it('ignores a press on the close button', () => {
      h.press('left', 'a', { onCloseButton: true })
      pointer('pointermove', 150)
      expect(h.controller.view).toBeNull()
    })

    it('ignores moves from another pointer', () => {
      h.press('left', 'a')
      window.dispatchEvent(new PointerEvent('pointermove', { pointerId: 2, buttons: 1, clientX: 200, clientY: 15 }))
      expect(h.controller.view).toBeNull()
    })
  })

  describe('what each bar is told', () => {
    it('names the dragged tab to its own bar alone, and the drag to both', () => {
      dragTo(h, 'left', 'b', 300)
      expect(h.controller.forPane('left').draggedTabId).toBe('b')
      expect(h.controller.forPane('right').draggedTabId).toBeNull()
      expect(h.controller.forPane('left').isDragging).toBe(true)
      expect(h.controller.forPane('right').isDragging).toBe(true)
    })

    it('goes quiet again once the drag ends', () => {
      dragTo(h, 'left', 'b', 300)
      pointer('pointerup', 300)
      expect(h.controller.forPane('left').draggedTabId).toBeNull()
      expect(h.controller.forPane('left').isDragging).toBe(false)
    })
  })

  describe('what the overlay is told to draw', () => {
    it('keeps the ghost under the pointer where it was grabbed, locked to the bar it is over', () => {
      // Grabbed `b` (left edge 105) at x = 115, so the ghost trails the pointer by 10.
      dragTo(h, 'left', 'b', 300, 25)
      expect(h.controller.view?.ghost).toEqual({ left: 290, top: 0, width: TAB_WIDTH, height: BAR_HEIGHT })
    })

    it('lets the ghost follow the pointer freely once it leaves the bars', () => {
      dragTo(h, 'left', 'b', 300, 200)
      expect(h.controller.view).toMatchObject({ overPane: null, line: null, refused: false })
      // Grabbed 15 px below the tab's top edge.
      expect(h.controller.view?.ghost).toMatchObject({ left: 290, top: 185 })
    })

    it('draws the landing line in the gap the tab would land in', () => {
      // Past `b`'s middle (155), short of `c`'s (260): the gap between them.
      dragTo(h, 'left', 'a', 200)
      expect(h.controller.view).toMatchObject({ overPane: 'left', line: { left: 207.5, top: 0, height: BAR_HEIGHT } })
    })

    it('draws the line in the other bar when the pointer is over it', () => {
      dragTo(h, 'left', 'a', 560)
      expect(h.controller.view).toMatchObject({ overPane: 'right', line: { left: 602.5 } })
    })

    it('draws the line at the end for the empty strip past the last tab', () => {
      dragTo(h, 'left', 'a', 890)
      expect(h.controller.view).toMatchObject({ overPane: 'right', line: { left: 705 } })
    })

    it('draws no line on a slot that would leave the tab where it is', () => {
      dragTo(h, 'left', 'b', 190)
      expect(h.controller.view).toMatchObject({ overPane: 'left', line: null, refused: false })
    })

    it('marks the other bar refused when it is full', () => {
      h.controller.destroy()
      document.body.innerHTML = ''
      h = harness({
        left: [makeTab('a'), makeTab('b')],
        right: Array.from({ length: 10 }, (_, i) => makeTab(`x${String(i)}`)),
      })

      dragTo(h, 'left', 'a', 560)
      expect(h.controller.view).toMatchObject({ overPane: 'right', line: null, refused: true })
    })

    it('stops refusing once the pointer is back over its own bar', () => {
      h.controller.destroy()
      document.body.innerHTML = ''
      h = harness({
        left: [makeTab('a'), makeTab('b')],
        right: Array.from({ length: 10 }, (_, i) => makeTab(`x${String(i)}`)),
      })

      dragTo(h, 'left', 'a', 560)
      pointer('pointermove', 50)
      expect(h.controller.view).toMatchObject({ overPane: 'left', refused: false })
    })
  })

  describe('dropping', () => {
    it('reorders within the bar, with the index the tab ends up on', () => {
      dragTo(h, 'left', 'a', 300)
      pointer('pointerup', 300)
      // Past `c`'s middle: the last slot, which is index 2 once `a` is out of the way.
      expect(h.drops).toEqual([{ fromPane: 'left', tabId: 'a', toPane: 'left', toIndex: 2 }])
      expect(h.controller.view).toBeNull()
    })

    it('moves to the other bar at the slot under the pointer', () => {
      dragTo(h, 'left', 'a', 300)
      pointer('pointermove', 620)
      pointer('pointerup', 620)
      expect(h.drops).toEqual([{ fromPane: 'left', tabId: 'a', toPane: 'right', toIndex: 1 }])
    })

    it('uses where the button was released, even with no move in between', () => {
      dragTo(h, 'left', 'a', 300)
      pointer('pointerup', 890)
      expect(h.drops).toEqual([{ fromPane: 'left', tabId: 'a', toPane: 'right', toIndex: 2 }])
    })

    it('changes nothing when the tab comes back to its own slot', () => {
      dragTo(h, 'left', 'b', 300)
      pointer('pointermove', 150)
      pointer('pointerup', 150)
      expect(h.drops).toEqual([])
    })

    it('changes nothing when released outside the bars', () => {
      dragTo(h, 'left', 'a', 300, 200)
      pointer('pointerup', 300, 200)
      expect(h.drops).toEqual([])
      expect(h.controller.view).toBeNull()
    })

    it('still hands a refused drop on, so the refusal can be explained', () => {
      h.controller.destroy()
      document.body.innerHTML = ''
      h = harness({
        left: [makeTab('a'), makeTab('b')],
        right: Array.from({ length: 10 }, (_, i) => makeTab(`x${String(i)}`)),
      })

      // Past the full right bar's first middle (550): slot 1.
      dragTo(h, 'left', 'a', 560)
      expect(h.controller.view).toMatchObject({ refused: true })
      pointer('pointerup', 560)
      expect(h.drops).toEqual([{ fromPane: 'left', tabId: 'a', toPane: 'right', toIndex: 1 }])
    })
  })

  describe('cancelling', () => {
    it('cancels on Escape, and keeps the key from the rest of the app', () => {
      dragTo(h, 'left', 'a', 300)
      const appHandler = vi.fn()
      document.body.addEventListener('keydown', appHandler)
      const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
      document.body.dispatchEvent(escape)

      expect(h.controller.view).toBeNull()
      expect(appHandler).not.toHaveBeenCalled()
      expect(escape.defaultPrevented).toBe(true)

      pointer('pointerup', 300)
      expect(h.drops).toEqual([])
    })

    it('leaves Escape alone when no drag is under way', () => {
      h.press('left', 'a')
      const appHandler = vi.fn()
      document.body.addEventListener('keydown', appHandler)
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      expect(appHandler).toHaveBeenCalledOnce()
    })

    it('leaves other keys alone during a drag', () => {
      dragTo(h, 'left', 'a', 300)
      const appHandler = vi.fn()
      document.body.addEventListener('keydown', appHandler)
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true }))
      expect(appHandler).toHaveBeenCalledOnce()
      expect(h.controller.view).not.toBeNull()
    })

    it('cancels when the window loses focus', () => {
      dragTo(h, 'left', 'a', 300)
      window.dispatchEvent(new Event('blur'))
      expect(h.controller.view).toBeNull()

      pointer('pointerup', 300)
      expect(h.drops).toEqual([])
    })

    it('cancels on pointercancel', () => {
      dragTo(h, 'left', 'a', 300)
      pointer('pointercancel', 300)
      expect(h.controller.view).toBeNull()
      expect(h.drops).toEqual([])
    })

    it('cancels on a move with no button down: a release the window never saw', () => {
      dragTo(h, 'left', 'a', 300)
      window.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, buttons: 0, clientX: 620, clientY: 15 }))
      expect(h.controller.view).toBeNull()

      pointer('pointerup', 620)
      expect(h.drops).toEqual([])
    })

    it('forgets a press that lost the window before it became a drag', () => {
      h.press('left', 'a')
      window.dispatchEvent(new Event('blur'))
      pointer('pointermove', 300)
      expect(h.controller.view).toBeNull()
    })
  })

  describe('the click that follows a drag', () => {
    /** Counts clicks that reach the tab, the way `TabBar`'s own `onclick` would hear them. */
    function clicksOn(el: HTMLElement): () => number {
      let count = 0
      el.addEventListener('click', () => count++)
      return () => count
    }

    it('lets the click through when the press never became a drag', () => {
      const clicks = clicksOn(h.tabEl('left', 'b'))
      h.press('left', 'b')
      pointer('pointerup', 115)
      h.tabEl('left', 'b').click()
      expect(clicks()).toBe(1)
    })

    it('swallows the click a drop ends with, so dragging a tab never switches to it', () => {
      const clicks = clicksOn(h.tabEl('left', 'b'))
      dragTo(h, 'left', 'b', 190)
      pointer('pointerup', 190)
      h.tabEl('left', 'b').click()
      expect(clicks()).toBe(0)
    })

    it('swallows the click after an Escape-cancelled drag is released', () => {
      const clicks = clicksOn(h.tabEl('left', 'b'))
      dragTo(h, 'left', 'b', 190)
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      pointer('pointerup', 150)
      h.tabEl('left', 'b').click()
      expect(clicks()).toBe(0)
    })

    it('lets the next click through when the release after a cancelled drag went to another app', () => {
      const clicks = clicksOn(h.tabEl('left', 'b'))
      dragTo(h, 'left', 'a', 300)
      window.dispatchEvent(new Event('blur'))

      // Back in the window: a whole new press and release, on a different tab.
      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1, buttons: 1 }))
      pointer('pointerup', 150)
      h.tabEl('left', 'b').click()
      expect(clicks()).toBe(1)
    })

    it('swallows one click only: the next one is a real click', () => {
      const clicks = clicksOn(h.tabEl('left', 'b'))
      dragTo(h, 'left', 'b', 190)
      pointer('pointerup', 190)
      h.tabEl('left', 'b').click()
      h.tabEl('left', 'b').click()
      expect(clicks()).toBe(1)
    })

    it('stops waiting for a click that never comes', () => {
      const clicks = clicksOn(h.tabEl('left', 'b'))
      dragTo(h, 'left', 'a', 300, 200)
      pointer('pointerup', 300, 200)
      vi.runAllTimers()
      h.tabEl('left', 'b').click()
      expect(clicks()).toBe(1)
    })
  })

  it('stops listening once destroyed', () => {
    dragTo(h, 'left', 'a', 300)
    h.controller.destroy()
    expect(h.controller.view).toBeNull()
    pointer('pointerup', 300)
    expect(h.drops).toEqual([])
  })
})
