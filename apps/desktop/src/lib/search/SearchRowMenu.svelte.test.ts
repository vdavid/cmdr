import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchRowMenu from './SearchRowMenu.svelte'

vi.mock('$lib/tooltip/tooltip', () => ({
  tooltip: () => ({ destroy() {} }),
}))

describe('SearchRowMenu', () => {
  it('renders an accessible button with a tooltip-friendly label', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchRowMenu, {
      target,
      props: { isCursorRow: false, onOpen: () => {} },
    })
    await tick()
    const btn = target.querySelector('.row-menu-btn') as HTMLButtonElement
    expect(btn).not.toBeNull()
    expect(btn.getAttribute('aria-label')).toBe('More actions')
    expect(btn.getAttribute('tabindex')).toBe('-1')
    target.remove()
  })

  it('marks the cursor row variant with .is-cursor for the always-visible CSS rule', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchRowMenu, {
      target,
      props: { isCursorRow: true, onOpen: () => {} },
    })
    await tick()
    const btn = target.querySelector('.row-menu-btn') as HTMLButtonElement
    expect(btn.classList.contains('is-cursor')).toBe(true)
    target.remove()
  })

  it('calls onOpen on click and stops the click from bubbling to the row', async () => {
    // Svelte 5 delegates `on*` events at the document root, so we assert against the
    // `stopPropagation` contract rather than racing a direct DOM listener on a wrapper
    // (which would see the click during the bubble phase, before Svelte's delegated
    // listener fires the pill's onclick).
    const stopSpy = vi.spyOn(Event.prototype, 'stopPropagation')
    const onOpen = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchRowMenu, {
      target,
      props: { isCursorRow: true, onOpen },
    })
    await tick()
    const btn = target.querySelector('.row-menu-btn') as HTMLButtonElement
    btn.click()
    expect(onOpen).toHaveBeenCalledTimes(1)
    expect(stopSpy).toHaveBeenCalled()
    stopSpy.mockRestore()
    target.remove()
  })
})
