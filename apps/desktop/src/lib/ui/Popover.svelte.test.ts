/**
 * Behavior tests for `Popover.svelte`.
 *
 * The dropdown's behavior is also exercised end-to-end via the query dialogs' filter-chip and
 * recent-items tests (they instantiate it with real content). This file covers the parts that
 * are awkward to reach from there: click-outside, anchor-click-doesn't-close, and the focus
 * return on Esc.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount, createRawSnippet } from 'svelte'
import Popover from './Popover.svelte'

function makeAnchor(): HTMLButtonElement {
  const btn = document.createElement('button')
  btn.textContent = 'Anchor'
  btn.id = 'test-anchor'
  document.body.appendChild(btn)
  return btn
}

const emptyChildren = createRawSnippet(() => ({ render: () => '<span>content</span>' }))

beforeEach(() => {
  document.body.innerHTML = ''
  document.querySelectorAll('.ui-popover').forEach((el) => {
    el.remove()
  })
})

describe('Popover behavior', () => {
  it('renders nothing when open is false', async () => {
    const anchor = makeAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: false, onClose: () => {}, children: emptyChildren },
    })
    await tick()
    expect(document.querySelector('.ui-popover')).toBeNull()
    void unmount(component)
  })

  it('renders the dialog with the role and aria-label when open', async () => {
    const anchor = makeAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, onClose: () => {}, ariaLabel: 'Test popover', children: emptyChildren },
    })
    await tick()
    const popover = document.querySelector('.ui-popover')
    expect(popover).not.toBeNull()
    expect(popover?.getAttribute('role')).toBe('dialog')
    expect(popover?.getAttribute('aria-label')).toBe('Test popover')
    void unmount(component)
  })

  it('Esc fires onClose, stops propagation, and returns focus to the anchor', async () => {
    const anchor = makeAnchor()
    const onClose = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, onClose, children: emptyChildren },
    })
    await tick()
    const popover = document.querySelector('.ui-popover') as HTMLElement
    expect(popover).not.toBeNull()

    const docHandler = vi.fn()
    document.addEventListener('keydown', docHandler)
    popover.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    document.removeEventListener('keydown', docHandler)

    expect(onClose).toHaveBeenCalledTimes(1)
    expect(docHandler).not.toHaveBeenCalled()
    expect(document.activeElement).toBe(anchor)
    void unmount(component)
  })

  it('Tab inside the popover (with no internal focusables) prevents default, never escapes', async () => {
    const anchor = makeAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, onClose: () => {}, children: emptyChildren },
    })
    await tick()
    const popover = document.querySelector('.ui-popover') as HTMLElement
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })
    popover.dispatchEvent(tab)
    expect(tab.defaultPrevented).toBe(true)
    void unmount(component)
  })

  it('clicking outside the popover and anchor fires onClose', async () => {
    const anchor = makeAnchor()
    const onClose = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const outside = document.createElement('div')
    document.body.appendChild(outside)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, onClose, children: emptyChildren },
    })
    await tick()
    outside.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
    expect(onClose).toHaveBeenCalledTimes(1)
    void unmount(component)
  })

  it('clicking inside the popover does NOT fire onClose', async () => {
    const anchor = makeAnchor()
    const onClose = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, onClose, children: emptyChildren },
    })
    await tick()
    const popover = document.querySelector('.ui-popover') as HTMLElement
    popover.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
    expect(onClose).not.toHaveBeenCalled()
    void unmount(component)
  })

  it('clicking the anchor does NOT fire onClose', async () => {
    const anchor = makeAnchor()
    const onClose = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, onClose, children: emptyChildren },
    })
    await tick()
    anchor.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
    expect(onClose).not.toHaveBeenCalled()
    void unmount(component)
  })

  it('handles a closed-then-opened transition (effect re-runs)', async () => {
    const anchor = makeAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    let open = $state(false)
    const component = mount(Popover, {
      target,
      props: {
        anchor,
        get open() {
          return open
        },
        onClose: () => {},
        children: emptyChildren,
      },
    })
    await tick()
    expect(document.querySelector('.ui-popover')).toBeNull()
    open = true
    await tick()
    expect(document.querySelector('.ui-popover')).not.toBeNull()
    void unmount(component)
  })

  it('non-modifier keys are passed through (no preventDefault)', async () => {
    const anchor = makeAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, onClose: () => {}, children: emptyChildren },
    })
    await tick()
    const popover = document.querySelector('.ui-popover') as HTMLElement
    const ev = new KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true })
    popover.dispatchEvent(ev)
    expect(ev.defaultPrevented).toBe(false)
    void unmount(component)
  })

  it('window resize repositions the popover (best-effort coverage; jsdom rects are zero)', async () => {
    const anchor = makeAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, onClose: () => {}, children: emptyChildren },
    })
    await tick()
    window.dispatchEvent(new Event('resize'))
    // No assertion: jsdom returns zero bounding rects, so we can't pin layout. This test exists
    // for coverage of the resize listener wiring.
    void unmount(component)
  })

  describe('passive (a popover that follows a field)', () => {
    const fieldChildren = createRawSnippet(() => ({ render: () => '<div><input id="inner" /></div>' }))

    async function settle(): Promise<void> {
      for (let i = 0; i < 3; i++) {
        await tick()
        await new Promise((resolve) => setTimeout(resolve, 0))
      }
    }

    it('opens without taking focus from where the user is typing, and Tab isn’t pulled into it', async () => {
      const anchor = makeAnchor()
      const field = document.createElement('input')
      document.body.appendChild(field)
      field.focus()
      const target = document.createElement('div')
      document.body.appendChild(target)
      const component = mount(Popover, {
        target,
        props: { anchor, open: true, passive: true, onClose: () => {}, children: fieldChildren },
      })
      await settle()
      expect(document.activeElement).toBe(field)
      const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true })
      field.dispatchEvent(tab)
      expect(tab.defaultPrevented).toBe(false)
      void unmount(component)
    })

    it('goes into its first field once it stops being passive', async () => {
      const anchor = makeAnchor()
      const target = document.createElement('div')
      document.body.appendChild(target)
      let passive = $state(true)
      const component = mount(Popover, {
        target,
        props: {
          anchor,
          open: true,
          get passive() {
            return passive
          },
          onClose: () => {},
          children: fieldChildren,
        },
      })
      await settle()
      expect(document.activeElement?.id).not.toBe('inner')
      passive = false
      await settle()
      expect(document.activeElement?.id).toBe('inner')
      void unmount(component)
    })

    it('stays open for a mousedown in `alsoInside`', async () => {
      const anchor = makeAnchor()
      const field = document.createElement('input')
      document.body.appendChild(field)
      const onClose = vi.fn()
      const target = document.createElement('div')
      document.body.appendChild(target)
      const component = mount(Popover, {
        target,
        props: { anchor, open: true, passive: true, alsoInside: field, onClose, children: fieldChildren },
      })
      await settle()
      field.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
      expect(onClose).not.toHaveBeenCalled()
      void unmount(component)
    })
  })

  it('wears an opaque surface when asked, for content that must read cleanly over anything', async () => {
    const anchor = makeAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(Popover, {
      target,
      props: { anchor, open: true, surface: 'solid', onClose: () => {}, children: emptyChildren },
    })
    await tick()
    expect(document.querySelector('.ui-popover')?.classList.contains('solid')).toBe(true)
    void unmount(component)
  })
})
