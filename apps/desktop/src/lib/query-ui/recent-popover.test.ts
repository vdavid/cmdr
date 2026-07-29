/**
 * The recent-items dropdown controller: who owns the open flag, and where focus lands
 * when the dropdown closes.
 *
 * Focus restore is the part that regresses silently. `Popover`'s Escape path focuses the
 * anchor, which is the query field's pill frame (a `<div>`, not focusable), so the caret
 * would fall to the document without the controller's refocus. Click-outside must NOT be
 * stolen, so the refocus is deferred a frame and only fires when nothing else claimed focus.
 */

import { describe, it, expect, vi } from 'vitest'
import { tick } from 'svelte'
import { createRecentPopover, type RecentPopoverController } from './recent-popover.svelte'

interface Harness {
  controller: RecentPopoverController<string>
  focusInput: ReturnType<typeof vi.fn>
  onActivate: ReturnType<typeof vi.fn>
  anchor: HTMLElement
}

function makeHarness(): Harness {
  const anchor = document.createElement('div')
  // `tabindex` makes the anchor focusable so the "focus is still on the anchor" branch
  // is reachable; the real pill frame isn't focusable, which is exactly why the
  // controller checks for it.
  anchor.tabIndex = -1
  document.body.appendChild(anchor)
  const focusInput = vi.fn()
  const onActivate = vi.fn()
  const controller = createRecentPopover<string>({
    focusInput,
    getAnchor: () => anchor,
    onActivate,
  })
  return { controller, focusInput, onActivate, anchor }
}

/** Lets the deferred (rAF) refocus in `close()` run. */
function nextFrame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      resolve()
    })
  })
}

describe('createRecentPopover', () => {
  it('starts closed', () => {
    const { controller } = makeHarness()
    expect(controller.isOpen).toBe(false)
  })

  it('opens without touching focus (the dropdown traps its own)', () => {
    const { controller, focusInput } = makeHarness()
    controller.open()
    expect(controller.isOpen).toBe(true)
    expect(focusInput).not.toHaveBeenCalled()
  })

  it('toggle opens when closed and closes when open', async () => {
    const { controller } = makeHarness()
    controller.toggle()
    expect(controller.isOpen).toBe(true)
    controller.toggle()
    expect(controller.isOpen).toBe(false)
    await tick()
  })

  it('closeAndFocus puts the caret straight back in the field', async () => {
    const { controller, focusInput } = makeHarness()
    controller.open()
    controller.closeAndFocus()
    expect(controller.isOpen).toBe(false)
    // Deferred a tick: while the popover is still mounted, its focus trap would pull
    // focus straight back.
    expect(focusInput).not.toHaveBeenCalled()
    await tick()
    expect(focusInput).toHaveBeenCalledTimes(1)
  })

  it('close refocuses the field when focus fell to the body', async () => {
    const { controller, focusInput } = makeHarness()
    controller.open()
    controller.close()
    expect(controller.isOpen).toBe(false)
    await nextFrame()
    expect(focusInput).toHaveBeenCalledTimes(1)
  })

  it('close refocuses the field when focus stayed on the (unfocusable) anchor', async () => {
    const { controller, focusInput, anchor } = makeHarness()
    controller.open()
    anchor.focus()
    controller.close()
    await nextFrame()
    expect(focusInput).toHaveBeenCalledTimes(1)
  })

  it('close leaves focus alone when something else already claimed it', async () => {
    const { controller, focusInput } = makeHarness()
    const other = document.createElement('input')
    document.body.appendChild(other)
    controller.open()
    controller.close()
    other.focus()
    await nextFrame()
    expect(focusInput).not.toHaveBeenCalled()
    other.remove()
  })

  it('picking an entry loads it and returns focus, but never runs it', async () => {
    const { controller, focusInput, onActivate } = makeHarness()
    controller.open()
    controller.pick('*.pdf')
    expect(onActivate).toHaveBeenCalledWith('*.pdf')
    expect(controller.isOpen).toBe(false)
    await tick()
    expect(focusInput).toHaveBeenCalledTimes(1)
  })
})
