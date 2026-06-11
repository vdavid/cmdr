/**
 * Behavior tests for `Chip.svelte`.
 *
 * The chip's in-context behavior is also exercised through `FilterChips.svelte.test.ts` (filter
 * variant) and `RecentItemsFooter.svelte.test.ts` (recent variant). This file pins the chip's own
 * contract: activate, clear (× + Backspace), the popover ARIA flags, and the recent variant's
 * context menu.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick, createRawSnippet, type ComponentProps } from 'svelte'
import Chip from './Chip.svelte'

type Props = ComponentProps<typeof Chip>

function mountChip(props: Props): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(Chip, { target, props })
  return target
}

describe('Chip filter variant', () => {
  it('renders the label only when not configured, and carries popover ARIA', async () => {
    const target = mountChip({ label: 'Size', configured: false, isOpen: false, onActivate: () => {} })
    await tick()
    const button = target.querySelector('button')
    expect(button?.textContent.trim()).toBe('Size')
    expect(button?.getAttribute('aria-haspopup')).toBe('dialog')
    expect(button?.getAttribute('aria-expanded')).toBe('false')
    target.remove()
  })

  it('renders "label: value" and a × clear marker when configured', async () => {
    const target = mountChip({
      label: 'Size',
      value: '> 100 MB',
      configured: true,
      isOpen: false,
      onActivate: () => {},
      onClear: () => {},
    })
    await tick()
    const button = target.querySelector('button')
    expect(button?.textContent).toContain('Size: > 100 MB')
    expect(target.querySelector('.chip-clear')).not.toBeNull()
    target.remove()
  })

  it('fires onActivate on Enter and aria-expanded reflects isOpen', async () => {
    const onActivate = vi.fn()
    const target = mountChip({ label: 'Size', configured: false, isOpen: true, onActivate })
    await tick()
    const button = target.querySelector('button') as HTMLButtonElement
    expect(button.getAttribute('aria-expanded')).toBe('true')
    button.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    expect(onActivate).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it('Backspace on a focused configured chip clears it', async () => {
    const onClear = vi.fn()
    const target = mountChip({
      label: 'Size',
      value: '> 100 MB',
      configured: true,
      isOpen: false,
      onActivate: () => {},
      onClear,
    })
    await tick()
    const button = target.querySelector('button') as HTMLButtonElement
    button.dispatchEvent(new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true }))
    expect(onClear).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it('× mousedown clears without firing onActivate', async () => {
    const onActivate = vi.fn()
    const onClear = vi.fn()
    const target = mountChip({
      label: 'Size',
      value: '> 100 MB',
      configured: true,
      isOpen: false,
      onActivate,
      onClear,
    })
    await tick()
    const clear = target.querySelector('.chip-clear') as HTMLElement
    clear.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
    expect(onClear).toHaveBeenCalledTimes(1)
    expect(onActivate).not.toHaveBeenCalled()
    target.remove()
  })
})

describe('Chip recent variant', () => {
  it('omits popover ARIA and renders a leading badge', async () => {
    const target = mountChip({
      variant: 'recent',
      label: '*.log',
      onActivate: () => {},
      leading: createRawSnippet(() => ({ render: () => '<span class="badge">Aa</span>' })),
    })
    await tick()
    const button = target.querySelector('button')
    expect(button?.getAttribute('aria-haspopup')).toBeNull()
    expect(button?.getAttribute('aria-expanded')).toBeNull()
    expect(target.querySelector('.badge')?.textContent).toBe('Aa')
    target.remove()
  })

  it('fires onContextMenu on right-click', async () => {
    const onContextMenu = vi.fn()
    const target = mountChip({
      variant: 'recent',
      label: '*.log',
      onActivate: () => {},
      onContextMenu,
    })
    await tick()
    const button = target.querySelector('button') as HTMLButtonElement
    button.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    expect(onContextMenu).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it('does not fire onActivate when disabled', async () => {
    const onActivate = vi.fn()
    const target = mountChip({ variant: 'recent', label: '*.log', disabled: true, onActivate })
    await tick()
    const button = target.querySelector('button') as HTMLButtonElement
    button.click()
    expect(onActivate).not.toHaveBeenCalled()
    target.remove()
  })
})
