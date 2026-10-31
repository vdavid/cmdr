/**
 * The pane divider's invisible hit areas (`PaneResizer.svelte`) and the path segments' floor (`FilePane.svelte`). Their
 * geometry lives in CSS and the test DOM does no layout, so this pins the declared offsets: the strip along the file
 * list must never cover the LEFT pane's scrollbar or the RIGHT pane's file icons, and the foot pad must hold a 24-by-24
 * square (WCAG 2.5.8, #817) inside the status footers' blank inset.
 */
import { flushSync, mount, unmount } from 'svelte'
import { describe, expect, it, vi } from 'vitest'
import PaneResizer from './PaneResizer.svelte'
import source from './PaneResizer.svelte?raw'
import filePaneSource from './FilePane.svelte?raw'
import { SEGMENT_MIN_HIT_WIDTH_PX } from '../navigation/breadcrumb-fit'

/** `--spacing-md`: the blank inset of `.selection-info`, and the left pad's overhang. */
const FOOTER_INSET_PX = 12
/** Where a file row's icon starts, from the pane's edge (`.listbox-region` gutter + `.file-entry` padding). */
const ROW_ICON_INSET_PX = 12

function ruleOf(selector: string): string {
  const rule = new RegExp(`\\.pane-resizer::${selector}\\s*\\{([^}]*)\\}`).exec(source)?.[1]
  if (rule === undefined) throw new Error(`\`.pane-resizer::${selector}\` rule not found`)
  return rule
}

/** The `left` / `right` offsets in px of a pseudo-element's rule. */
function offsets(selector: string): { left: number; right: number } {
  const rule = ruleOf(selector).replace(/calc\(-1 \* var\(--spacing-md\)\)/, `-${String(FOOTER_INSET_PX)}px`)
  const offset = (side: 'left' | 'right'): number => {
    const value = new RegExp(`(?:^|;|\\s)${side}:\\s*(-?[\\d.]+)(px)?\\s*;`).exec(rule)?.[1]
    if (value === undefined) throw new Error(`\`${side}\` missing from \`::${selector}\``)
    return Number(value)
  }
  return { left: offset('left'), right: offset('right') }
}

describe('pane divider hit areas', () => {
  it('the strip never reaches over the left pane, so its scrollbar stays fully grabbable', () => {
    expect(offsets('before').left).toBeGreaterThanOrEqual(0)
  })

  it('the strip stops short of the right pane’s file icons, so it takes no click from a row', () => {
    expect(-offsets('before').right).toBeLessThan(ROW_ICON_INSET_PX)
  })

  it('the foot pad is at least 24 wide, counting the 1px divider, and anchored to the bottom', () => {
    const { left, right } = offsets('after')
    expect(1 - left - right).toBeGreaterThanOrEqual(24)
    expect(ruleOf('after')).toMatch(/bottom:\s*0;/)
    expect(ruleOf('after')).toMatch(/height:\s*var\(--spacing-xl\);/)
  })

  it('the foot pad stays inside both status footers’ blank inset', () => {
    const { left, right } = offsets('after')
    expect(-left).toBeLessThanOrEqual(FOOTER_INSET_PX)
    expect(-right).toBeLessThanOrEqual(FOOTER_INSET_PX)
  })
})

describe('a wheel over the divider', () => {
  it('scrolls the list under the strip, so the strip never eats a scroll', () => {
    const explorer = document.createElement('div')
    document.body.appendChild(explorer)
    const instance = mount(PaneResizer, {
      target: explorer,
      props: { onResize: () => {}, onResizeEnd: () => {}, onReset: () => {} },
    })
    flushSync()
    const resizer = explorer.querySelector<HTMLElement>('.pane-resizer')
    if (!resizer) throw new Error('resizer not found')

    const list = document.createElement('div')
    list.style.overflowY = 'auto'
    Object.defineProperty(list, 'scrollHeight', { value: 5000 })
    Object.defineProperty(list, 'clientHeight', { value: 500 })
    const scrollBy = vi.fn()
    list.scrollBy = scrollBy as typeof list.scrollBy
    const row = document.createElement('div')
    list.appendChild(row)
    explorer.appendChild(list)
    const elementsFromPoint = vi.fn(() => [resizer, row, list])
    document.elementsFromPoint = elementsFromPoint

    const wheel = Object.assign(new Event('wheel', { bubbles: true }), {
      clientX: 504,
      clientY: 200,
      deltaX: 0,
      deltaY: 120,
    })
    resizer.dispatchEvent(wheel)

    expect(elementsFromPoint).toHaveBeenCalledWith(504, 200)
    expect(scrollBy).toHaveBeenCalledWith(0, 120)
    void unmount(instance)
    explorer.remove()
  })

  /**
   * `elementsFromPoint` answers as the running app does (Full view, 1081-wide window at 50%): the line has its own
   * column, so at x 540 no pane lies under it, only the explorer, and the right pane starts at x 541. The geometry
   * itself is pinned against real layout by `pane-divider-wheel.spec.ts`.
   */
  function mountOverStack(stackAt: (x: number) => Element[]): { resizer: HTMLElement; cleanup: () => void } {
    const explorer = document.createElement('div')
    document.body.appendChild(explorer)
    const instance = mount(PaneResizer, {
      target: explorer,
      props: { onResize: () => {}, onResizeEnd: () => {}, onReset: () => {} },
    })
    flushSync()
    const resizer = explorer.querySelector<HTMLElement>('.pane-resizer')
    if (!resizer) throw new Error('resizer not found')
    resizer.getBoundingClientRect = () => ({ left: 540, right: 541 }) as DOMRect
    document.elementsFromPoint = (x: number) => [resizer, ...stackAt(x)]
    return {
      resizer,
      cleanup: () => {
        void unmount(instance)
        explorer.remove()
        scrollers.splice(0).forEach((element) => {
          element.remove()
        })
      },
    }
  }

  const scrollers: HTMLElement[] = []

  function scroller(): { element: HTMLElement; scrollBy: ReturnType<typeof vi.fn> } {
    const element = document.createElement('div')
    const scrollBy = vi.fn()
    element.scrollBy = scrollBy as typeof element.scrollBy
    element.style.overflowY = 'auto'
    Object.defineProperty(element, 'scrollHeight', { value: 5000 })
    Object.defineProperty(element, 'clientHeight', { value: 500 })
    // Detached, it has no computed overflow.
    document.body.appendChild(element)
    scrollers.push(element)
    return { element, scrollBy }
  }

  function wheelAt(clientX: number): Event {
    return Object.assign(new Event('wheel', { bubbles: true }), { clientX, clientY: 200, deltaX: 0, deltaY: 120 })
  }

  it('on the line itself, where no pane lies under it, scrolls the right pane as the strip does', () => {
    const rightList = scroller()
    const nothingScrollable = document.createElement('div')
    const { resizer, cleanup } = mountOverStack((x) => (x >= 541 ? [rightList.element] : [nothingScrollable]))
    resizer.dispatchEvent(wheelAt(540))
    expect(rightList.scrollBy).toHaveBeenCalledWith(0, 120)
    cleanup()
  })

  it('left of the line (the handle’s overhang, the foot pad) still scrolls what’s there', () => {
    const leftList = scroller()
    const rightList = scroller()
    const { resizer, cleanup } = mountOverStack((x) => (x >= 541 ? [rightList.element] : [leftList.element]))
    resizer.dispatchEvent(wheelAt(538))
    expect(leftList.scrollBy).toHaveBeenCalledWith(0, 120)
    expect(rightList.scrollBy).not.toHaveBeenCalled()
    cleanup()
  })
})

describe('path segment hit area', () => {
  it('declares the floor the fit reserves room for', () => {
    const rule = /\.path :global\(\.path-segment\)\s*\{([^}]*)\}/.exec(filePaneSource)?.[1] ?? ''
    expect(rule).toContain(`min-width: ${String(SEGMENT_MIN_HIT_WIDTH_PX)}px;`)
    expect(SEGMENT_MIN_HIT_WIDTH_PX).toBeGreaterThanOrEqual(24)
  })
})

describe('dragging the divider', () => {
  function setup(): { resizer: HTMLElement; onResize: ReturnType<typeof vi.fn>; cleanup: () => void } {
    const explorer = document.createElement('div')
    explorer.className = 'dual-pane-explorer'
    explorer.getBoundingClientRect = () => ({ left: 0, width: 1000 }) as DOMRect
    document.body.appendChild(explorer)
    const onResize = vi.fn()
    const instance = mount(PaneResizer, {
      target: explorer,
      props: { onResize, onResizeEnd: () => {}, onReset: () => {} },
    })
    flushSync()
    const resizer = explorer.querySelector<HTMLElement>('.pane-resizer')
    if (!resizer) throw new Error('resizer not found')
    resizer.getBoundingClientRect = () => ({ left: 500 }) as DOMRect
    return {
      resizer,
      onResize,
      cleanup: () => {
        document.dispatchEvent(new MouseEvent('mouseup'))
        void unmount(instance)
        explorer.remove()
      },
    }
  }

  it('keeps the grab offset, so pressing the wide pad off the line doesn’t jump the divider', () => {
    const { resizer, onResize, cleanup } = setup()
    resizer.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: 510 }))
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 510 }))
    expect(onResize).toHaveBeenLastCalledWith(50)
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 610 }))
    expect(onResize).toHaveBeenLastCalledWith(60)
    cleanup()
  })

  it('ignores a secondary press, whose context menu would swallow the mouseup and strand the drag', () => {
    const { resizer, onResize, cleanup } = setup()
    resizer.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 2, clientX: 500 }))
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 600 }))
    expect(onResize).not.toHaveBeenCalled()
    cleanup()
  })
})
