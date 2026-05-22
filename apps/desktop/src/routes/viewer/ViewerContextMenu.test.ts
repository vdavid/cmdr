import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewerContextMenu from './ViewerContextMenu.svelte'

beforeEach(() => {
  document.body.innerHTML = ''
})

interface MountOpts {
  hasSelection?: boolean
  onCopy?: () => void
  onSelectAll?: () => void
  onClose?: () => void
}

async function mountMenu(opts: MountOpts = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onClose = opts.onClose ?? (() => {})
  const onCopy = opts.onCopy ?? (() => {})
  const onSelectAll = opts.onSelectAll ?? (() => {})
  const instance = mount(ViewerContextMenu, {
    target,
    props: { x: 50, y: 50, hasSelection: opts.hasSelection ?? true, onCopy, onSelectAll, onClose },
  })
  await tick()
  return { target, instance }
}

describe('ViewerContextMenu keyboard', () => {
  it('Escape closes the menu', async () => {
    const onClose = vi.fn()
    const { instance } = await mountMenu({ onClose })

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await tick()

    expect(onClose).toHaveBeenCalledTimes(1)
    void unmount(instance)
  })

  it('Escape calls stopImmediatePropagation so a later sibling listener does not fire', async () => {
    // This protects against future regressions where the page's Escape listener is
    // registered AFTER the menu's. Today the page registers first (the menu mounts
    // later) so the page also has its own `contextMenuPos` short-circuit; this test
    // is the defense-in-depth half.
    const { instance } = await mountMenu()
    const laterListener = vi.fn()
    window.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') laterListener()
    })

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await tick()

    expect(laterListener).not.toHaveBeenCalled()
    void unmount(instance)
  })

  it('ArrowUp moves focus to the previous item, wrapping at the start', async () => {
    const { instance, target } = await mountMenu()
    const items = target.querySelectorAll<HTMLButtonElement>('.menu-item')
    expect(items.length).toBe(2)

    // ArrowUp from the first item should wrap to the last one (Select all).
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
    await tick()
    expect(document.activeElement).toBe(items[1])

    // ArrowUp again wraps back to the first item.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
    await tick()
    expect(document.activeElement).toBe(items[0])

    void unmount(instance)
  })

  it('ArrowDown moves focus to the next item, wrapping at the end', async () => {
    const { instance, target } = await mountMenu()
    const items = target.querySelectorAll<HTMLButtonElement>('.menu-item')

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    await tick()
    expect(document.activeElement).toBe(items[1])

    // ArrowDown from the last item wraps to the first.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    await tick()
    expect(document.activeElement).toBe(items[0])

    void unmount(instance)
  })
})
