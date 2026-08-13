/**
 * Behavior tests for `ModalDialog.svelte`. Tier-3 a11y wiring lives in
 * `ModalDialog.a11y.test.ts`. This file covers focus restoration on close, the
 * Enter-on-focused-button suppression, resizing, and the MCP close registry.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, unmount, tick, createRawSnippet } from 'svelte'
import ModalDialog from './ModalDialog.svelte'
import { closeDialogById } from './dialog-close-registry'

// Avoid Tauri IPC side-effects from notifyDialogOpened / notifyDialogClosed.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

const titleSnippet = createRawSnippet(() => ({ render: () => `<span>Dialog title</span>` }))
const bodySnippet = createRawSnippet(() => ({ render: () => `<p>Body.</p>` }))
const footerSnippet = createRawSnippet(() => ({ render: () => `<button>OK</button>` }))

/** The resize bands the panel exposes, in DOM order (which is also their hit-test order). */
function bandDirections(target: HTMLElement): string[] {
  return [...target.querySelectorAll('.resize-band')].map((band) => band.getAttribute('data-direction') ?? '')
}

/**
 * jsdom lays nothing out, so the panel is told how big it is and the drag reads that back.
 * 400×300 is a realistic opening size (`GoToPathDialog` opens at 460 wide).
 */
function givePanelSize(panel: HTMLElement, width: number, height: number) {
  panel.getBoundingClientRect = () =>
    ({ width, height, top: 0, left: 0, right: width, bottom: height, x: 0, y: 0 }) as DOMRect
}

/** Presses a resize band, moves the pointer by (dx, dy), and releases it. */
async function dragBand(target: HTMLElement, direction: string, dx: number, dy: number) {
  const band = target.querySelector(`.resize-band[data-direction='${direction}']`)
  const [startX, startY] = [100, 100]
  band?.dispatchEvent(new MouseEvent('pointerdown', { clientX: startX, clientY: startY, bubbles: true }))
  document.dispatchEvent(new MouseEvent('pointermove', { clientX: startX + dx, clientY: startY + dy }))
  document.dispatchEvent(new MouseEvent('pointerup'))
  await tick()
}

describe('ModalDialog focus restoration', () => {
  it('restores focus to the previously focused element on destroy', async () => {
    const trigger = document.createElement('button')
    document.body.appendChild(trigger)
    trigger.focus()
    expect(document.activeElement).toBe(trigger)

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodySnippet },
    })

    // Let onMount run so the dialog captures `trigger` as previously focused.
    await tick()

    // Simulate "dialog has stolen focus".
    const otherEl = document.createElement('input')
    document.body.appendChild(otherEl)
    otherEl.focus()
    expect(document.activeElement).toBe(otherEl)

    void unmount(component)
    await tick()

    expect(document.activeElement).toBe(trigger)

    otherEl.remove()
    trigger.remove()
    target.remove()
  })

  it('does not throw if the previously focused element is no longer in the DOM', async () => {
    const trigger = document.createElement('button')
    document.body.appendChild(trigger)
    trigger.focus()

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodySnippet },
    })
    await tick()

    trigger.remove()
    expect(() => {
      void unmount(component)
    }).not.toThrow()
    await tick()

    target.remove()
  })
})

describe('ModalDialog body padding and resizing', () => {
  function mountDialog(props: Record<string, unknown>) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodySnippet, ...props },
    })
    return target
  }

  it('wraps children in a .modal-body element', () => {
    const target = mountDialog({})
    const body = target.querySelector('.modal-body')
    expect(body).not.toBeNull()
    expect(body?.textContent).toContain('Body.')
    target.remove()
  })

  it('adds .no-footer when there is no footer so the body owns bottom padding', () => {
    const target = mountDialog({})
    expect(target.querySelector('.modal-body')?.classList.contains('no-footer')).toBe(true)
    target.remove()
  })

  it('drops .no-footer when a footer is present (footer owns bottom padding)', () => {
    const target = mountDialog({ footer: footerSnippet })
    expect(target.querySelector('.modal-body')?.classList.contains('no-footer')).toBe(false)
    target.remove()
  })

  // There is no full-bleed opt-out: the body inset is always ModalDialog's, so a
  // dialog can't quietly leave one of its sections hanging off the panel edge.
  it('always keeps the body inset (no full-bleed opt-out)', () => {
    const target = mountDialog({})
    expect(target.querySelector('.modal-body')?.classList.contains('flush')).toBe(false)
    target.remove()
  })

  it('adds .resizable to the dialog when resizable is true', () => {
    const target = mountDialog({ resizable: true })
    expect(target.querySelector('.modal-dialog')?.classList.contains('resizable')).toBe(true)
    target.remove()
  })

  it('does not add .resizable by default', () => {
    const target = mountDialog({})
    expect(target.querySelector('.modal-dialog')?.classList.contains('resizable')).toBe(false)
    target.remove()
  })

  // `resizable="horizontal"` locks the height by exposing no band that could change it,
  // so the panel can't be dragged into a strip of dead space above the footer.
  it('exposes a grab band per resizable edge, with corners only when both axes are free', () => {
    const horizontal = mountDialog({ resizable: 'horizontal' })
    expect(bandDirections(horizontal)).toEqual(['w', 'e'])
    horizontal.remove()

    const both = mountDialog({ resizable: true })
    // Corners last: they're siblings, so the later ones win the hit test on overlap.
    expect(bandDirections(both)).toEqual(['n', 's', 'w', 'e', 'nw', 'ne', 'sw', 'se'])
    both.remove()

    const plain = mountDialog({})
    expect(bandDirections(plain)).toEqual([])
    plain.remove()
  })

  // `fillBody`'s inner region owns the scrolling; `resizable` only adds the bands and the
  // floors. Combining them is how the query dialogs and the operation log get resized.
  it('combines resizable with fillBody', () => {
    const target = mountDialog({ resizable: true, fillBody: true })
    const panel = target.querySelector('.modal-dialog')
    expect(panel?.classList.contains('resizable')).toBe(true)
    expect(panel?.classList.contains('fill-body')).toBe(true)
    target.remove()
  })

  // Every drag frame parks the user's size in the panel's inline `style`, so re-rendering
  // that attribute would snap a resized dialog back. Size and position ride on inline
  // PROPERTIES instead, leaving the attribute to `containerStyle` alone.
  it('keeps a user-set width across a title-bar drag', async () => {
    const target = mountDialog({ resizable: true, containerStyle: 'width: 500px' })
    const panel = target.querySelector<HTMLElement>('.modal-dialog')
    expect(panel).not.toBeNull()
    if (!panel) return
    panel.style.width = '820px'

    const titleBar = target.querySelector('.dialog-title-bar')
    titleBar?.dispatchEvent(new MouseEvent('mousedown', { clientX: 10, clientY: 10, bubbles: true }))
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 60, clientY: 40 }))
    document.dispatchEvent(new MouseEvent('mouseup'))
    await tick()

    expect(panel.style.width).toBe('820px')
    expect(panel.style.left).toBe('50px')
    target.remove()
  })
})

// The panel is CENTERED, not absolutely placed, so growing it by N slides its layout box
// by N/2 and the drag offset has to pay that back — otherwise both edges crawl outward and
// the one the user isn't holding drifts away from under the pointer.
describe('ModalDialog edge resizing', () => {
  // The `tick` matters: `bind:this` lands in an effect, so before the first flush the
  // component has no panel to measure and a band drag would no-op.
  async function mountResizable(props: Record<string, unknown> = {}) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodySnippet, resizable: true, ...props },
    })
    await tick()
    const panel = target.querySelector<HTMLElement>('.modal-dialog')
    if (!panel) throw new Error('the panel never mounted')
    givePanelSize(panel, 400, 300)
    return { target, panel }
  }

  it('widens to the east and leaves the west edge where it was', async () => {
    const { target, panel } = await mountResizable()
    await dragBand(target, 'e', 60, 0)

    expect(panel.style.width).toBe('460px')
    expect(panel.style.left).toBe('30px')
    expect(panel.style.height).toBe('')
    target.remove()
  })

  it('widens to the west and leaves the east edge where it was', async () => {
    const { target, panel } = await mountResizable()
    await dragBand(target, 'w', -60, 0)

    expect(panel.style.width).toBe('460px')
    expect(panel.style.left).toBe('-30px')
    target.remove()
  })

  it('grows a centered dialog upward from the north edge, bottom edge staying put', async () => {
    const { target, panel } = await mountResizable()
    await dragBand(target, 'n', 0, -60)

    expect(panel.style.height).toBe('360px')
    expect(panel.style.top).toBe('-30px')
    target.remove()
  })

  it('grows a centered dialog downward from the south edge, top edge staying put', async () => {
    const { target, panel } = await mountResizable()
    await dragBand(target, 's', 0, 60)

    expect(panel.style.height).toBe('360px')
    expect(panel.style.top).toBe('30px')
    target.remove()
  })

  // `align="top"` pins the panel's top edge in the layout, so it never drifts on its own:
  // the whole growth has to come from the offset, not half of it.
  it('moves a top-aligned dialog by its full growth when the north edge is dragged', async () => {
    const { target, panel } = await mountResizable({ align: 'top' })
    await dragBand(target, 'n', 0, -60)

    expect(panel.style.height).toBe('360px')
    expect(panel.style.top).toBe('-60px')
    target.remove()
  })

  it('resizes both axes at once from a corner', async () => {
    const { target, panel } = await mountResizable()
    await dragBand(target, 'se', 60, 60)

    expect(panel.style.width).toBe('460px')
    expect(panel.style.height).toBe('360px')
    expect(panel.style.left).toBe('30px')
    expect(panel.style.top).toBe('30px')
    target.remove()
  })

  // Dragged past the floor, the panel stops but the pointer doesn't: the offset has to stop
  // with it, or the panel keeps sliding while its size stands still.
  it('floors the height at the chrome plus a readable body', async () => {
    const { target, panel } = await mountResizable()
    await dragBand(target, 's', 0, -400)

    // jsdom reports no height for the title bar or footer, so the floor is the body's 60.
    expect(panel.style.height).toBe('60px')
    expect(panel.style.top).toBe('-120px')
    target.remove()
  })
})

describe('ModalDialog Enter key', () => {
  // Body containing both a button (Cancel) and an input. The test dispatches Enter
  // from each and verifies the dialog's default-action handler only fires for the input.
  const bodyWithControls = createRawSnippet(() => ({
    render: () => `<div><button id="cancel-btn">Cancel</button><input id="path-input" /></div>`,
  }))

  it('suppresses the default action when Enter is pressed on a focused button', async () => {
    const onkeydown = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodyWithControls, onkeydown },
    })
    await tick()

    const cancelBtn = target.querySelector<HTMLButtonElement>('#cancel-btn')
    if (!cancelBtn) throw new Error('cancel button not rendered')
    cancelBtn.focus()
    cancelBtn.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))

    expect(onkeydown).not.toHaveBeenCalled()
    target.remove()
  })

  it('still fires the default action when Enter is pressed on a non-button element', async () => {
    const onkeydown = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodyWithControls, onkeydown },
    })
    await tick()

    const input = target.querySelector<HTMLInputElement>('#path-input')
    if (!input) throw new Error('input not rendered')
    input.focus()
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))

    expect(onkeydown).toHaveBeenCalledTimes(1)
    target.remove()
  })
})

describe('ModalDialog MCP close registry', () => {
  /**
   * A dialog whose `onclose` is CONDITIONAL renders a new arrow every time the
   * condition flips (`TransferProgressDialog` withdraws its close while a
   * conflict is on screen). The registry is keyed by dialog id but guarded by
   * function identity, so it has to follow the current `onclose` rather than
   * the one that happened to exist at mount: a stale entry makes MCP's
   * `dialog close` answer `true` for a dialog that isn't there.
   */
  /**
   * ⚠️ The props object goes to `mount` UNSPREAD: spreading a `$state` object
   * into a fresh literal reads every field once and hands the component a
   * plain snapshot, so nothing the test writes afterwards reaches it.
   */
  interface ClosableProps {
    titleId: string
    title: typeof titleSnippet
    children: typeof bodySnippet
    dialogId: 'transfer-progress'
    onclose?: () => void
  }

  function mountWithProps(props: ClosableProps) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(ModalDialog, { target, props })
    return { target, component }
  }

  it('follows the current onclose when its identity changes, and unregisters on unmount', async () => {
    const closed: string[] = []
    const props: ClosableProps = $state({
      titleId: 't',
      title: titleSnippet,
      children: bodySnippet,
      dialogId: 'transfer-progress',
      onclose: () => closed.push('first'),
    })
    const { target, component } = mountWithProps(props)
    await tick()

    expect(closeDialogById('transfer-progress')).toBe(true)

    props.onclose = () => closed.push('second')
    await tick()
    expect(closeDialogById('transfer-progress')).toBe(true)
    expect(closed).toEqual(['first', 'second'])

    void unmount(component)
    await tick()

    expect(closeDialogById('transfer-progress')).toBe(false)
    target.remove()
  })

  it('drops the registration while the dialog renders without an onclose', async () => {
    const closed: string[] = []
    const props: ClosableProps = $state({
      titleId: 't',
      title: titleSnippet,
      children: bodySnippet,
      dialogId: 'transfer-progress',
      onclose: () => closed.push('closed'),
    })
    const { target, component } = mountWithProps(props)
    await tick()

    // The conflict body takes over: no ×, no Escape, so nothing to close by.
    props.onclose = undefined
    await tick()
    expect(closeDialogById('transfer-progress')).toBe(false)
    expect(closed).toEqual([])

    void unmount(component)
    await tick()
    expect(closeDialogById('transfer-progress')).toBe(false)
    target.remove()
  })
})
