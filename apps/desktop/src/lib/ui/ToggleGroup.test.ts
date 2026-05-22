/**
 * Tests for the generic `ToggleGroup` primitive.
 *
 * Covers both ARIA shapes (`semantics: 'tabs'` and `semantics: 'toggles'`),
 * option rendering with optional badge/hint, click activation, arrow-key motion
 * in tabs semantics (skipping disabled options), and the disabled root short-circuit.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ToggleGroup from './ToggleGroup.svelte'

interface Option {
  value: string
  label: string
  badge?: string
  hint?: string
  disabled?: boolean
  tooltip?: string
  ariaLabel?: string
}

function setupTabs(options: Option[], value: string, opts: Partial<{ disabled: boolean }> = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onChange = vi.fn()
  mount(ToggleGroup, {
    target,
    props: {
      semantics: 'tabs',
      value,
      options,
      onChange,
      ariaLabel: 'Test mode',
      disabled: opts.disabled ?? false,
    },
  })
  return { target, onChange }
}

function setupToggles(options: Option[], value: string, opts: Partial<{ disabled: boolean }> = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onChange = vi.fn()
  mount(ToggleGroup, {
    target,
    props: {
      semantics: 'toggles',
      value,
      options,
      onChange,
      ariaLabel: 'Test setting',
      disabled: opts.disabled ?? false,
    },
  })
  return { target, onChange }
}

function getTabs(target: HTMLElement): HTMLButtonElement[] {
  return Array.from(target.querySelectorAll<HTMLButtonElement>('[role="tab"]'))
}

describe('ToggleGroup (tabs semantics)', () => {
  const baseOptions: Option[] = [
    { value: 'ai', label: 'Ask anything', badge: 'AI', hint: '⌥A' },
    { value: 'filename', label: 'Filename', hint: '⌥F' },
    { value: 'content', label: 'Content', disabled: true, tooltip: 'Coming soon' },
    { value: 'regex', label: 'Regex', hint: '⌥R' },
  ]

  it('renders one button per option, with role="tab" and the right ARIA shape', async () => {
    const { target } = setupTabs(baseOptions, 'filename')
    await tick()
    const list = target.querySelector('[role="tablist"]')
    expect(list).not.toBeNull()
    expect(list?.getAttribute('aria-label')).toBe('Test mode')
    const tabs = getTabs(target)
    expect(tabs).toHaveLength(4)
    expect(tabs[1].getAttribute('aria-selected')).toBe('true')
    expect(tabs[0].getAttribute('aria-selected')).toBe('false')
  })

  it('renders badge and hint elements when provided, and not when omitted', async () => {
    const { target } = setupTabs(baseOptions, 'ai')
    await tick()
    const tabs = getTabs(target)
    // AI tab has both badge and hint.
    expect(tabs[0].querySelector('.tg-badge')?.textContent).toBe('AI')
    expect(tabs[0].querySelector('.tg-hint')?.textContent).toBe('⌥A')
    // Content tab has neither.
    expect(tabs[2].querySelector('.tg-badge')).toBeNull()
    expect(tabs[2].querySelector('.tg-hint')).toBeNull()
  })

  it('fires onChange with the option value on click', async () => {
    const { target, onChange } = setupTabs(baseOptions, 'filename')
    await tick()
    const tabs = getTabs(target)
    tabs[0].click()
    expect(onChange).toHaveBeenCalledWith('ai')
  })

  it('does not fire onChange when a disabled option is clicked', async () => {
    const { target, onChange } = setupTabs(baseOptions, 'filename')
    await tick()
    const tabs = getTabs(target)
    tabs[2].click()
    expect(onChange).not.toHaveBeenCalled()
  })

  it('does not fire onChange when the root is disabled', async () => {
    const { target, onChange } = setupTabs(baseOptions, 'filename', { disabled: true })
    await tick()
    const tabs = getTabs(target)
    tabs[0].click()
    expect(onChange).not.toHaveBeenCalled()
    // Every interactive tab also reports `disabled` to the DOM.
    expect(tabs.every((t) => t.disabled)).toBe(true)
  })

  it('sets tabindex=0 on the active tab and tabindex=-1 on the rest', async () => {
    const { target } = setupTabs(baseOptions, 'filename')
    await tick()
    const tabs = getTabs(target)
    expect(tabs[0].getAttribute('tabindex')).toBe('-1')
    expect(tabs[1].getAttribute('tabindex')).toBe('0')
    expect(tabs[2].getAttribute('tabindex')).toBe('-1')
    expect(tabs[3].getAttribute('tabindex')).toBe('-1')
  })

  it('falls back to the first interactive tab when the active one is disabled', async () => {
    const { target } = setupTabs(baseOptions, 'content')
    await tick()
    const tabs = getTabs(target)
    // Active is "content" (disabled), so the AI tab gets tabindex=0 (first interactive).
    expect(tabs[0].getAttribute('tabindex')).toBe('0')
  })

  it('ArrowRight moves focus to the next interactive tab and skips disabled', async () => {
    const { target } = setupTabs(baseOptions, 'filename')
    await tick()
    const tabs = getTabs(target)
    tabs[1].focus()
    tabs[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    // Skips "content" (disabled), lands on "regex".
    expect(document.activeElement).toBe(tabs[3])
  })

  it('ArrowLeft moves focus to the previous interactive tab and wraps', async () => {
    const { target } = setupTabs(baseOptions, 'ai')
    await tick()
    const tabs = getTabs(target)
    tabs[0].focus()
    tabs[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }))
    // Wraps to "regex" (last interactive).
    expect(document.activeElement).toBe(tabs[3])
  })

  it('Enter and Space on a tab activate it', async () => {
    const { target, onChange } = setupTabs(baseOptions, 'filename')
    await tick()
    const tabs = getTabs(target)
    tabs[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    expect(onChange).toHaveBeenLastCalledWith('ai')
    tabs[3].dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true }))
    expect(onChange).toHaveBeenLastCalledWith('regex')
  })

  it('passes ariaLabel override through to the button', async () => {
    const opts: Option[] = [{ value: 'a', label: 'A', ariaLabel: 'AI mode (Alt+A)' }]
    const { target } = setupTabs(opts, 'a')
    await tick()
    const tabs = getTabs(target)
    expect(tabs[0].getAttribute('aria-label')).toBe('AI mode (Alt+A)')
  })
})

describe('ToggleGroup (toggles semantics)', () => {
  const opts: Option[] = [
    { value: 'compact', label: 'Compact' },
    { value: 'comfortable', label: 'Comfortable' },
    { value: 'spacious', label: 'Spacious' },
  ]

  it('renders an Ark ToggleGroup root with the active option pressed', async () => {
    const { target } = setupToggles(opts, 'comfortable')
    await tick()
    const root = target.querySelector('[data-scope="toggle-group"][data-part="root"]')
    expect(root).not.toBeNull()
    expect(root?.getAttribute('aria-label')).toBe('Test setting')
    const items = Array.from(
      target.querySelectorAll<HTMLButtonElement>('[data-scope="toggle-group"][data-part="item"]'),
    )
    expect(items).toHaveLength(3)
    expect(items[1].getAttribute('data-state')).toBe('on')
    expect(items[0].getAttribute('data-state')).toBe('off')
  })

  it('fires onChange with the new value on click', async () => {
    const { target, onChange } = setupToggles(opts, 'comfortable')
    await tick()
    const items = Array.from(
      target.querySelectorAll<HTMLButtonElement>('[data-scope="toggle-group"][data-part="item"]'),
    )
    items[0].click()
    expect(onChange).toHaveBeenCalledWith('compact')
  })

  it('does not fire onChange when clicking the already-active option (no deselect)', async () => {
    const { target, onChange } = setupToggles(opts, 'comfortable')
    await tick()
    const items = Array.from(
      target.querySelectorAll<HTMLButtonElement>('[data-scope="toggle-group"][data-part="item"]'),
    )
    items[1].click()
    await tick()
    expect(onChange).not.toHaveBeenCalled()
  })

  it('does not fire onChange when the root is disabled', async () => {
    const { target, onChange } = setupToggles(opts, 'comfortable', { disabled: true })
    await tick()
    const items = Array.from(
      target.querySelectorAll<HTMLButtonElement>('[data-scope="toggle-group"][data-part="item"]'),
    )
    items[0].click()
    await tick()
    expect(onChange).not.toHaveBeenCalled()
  })

  it('renders badge and hint elements on toggle items the same way as on tabs', async () => {
    const richOpts: Option[] = [
      { value: 'a', label: 'A', badge: 'NEW', hint: '⌘1' },
      { value: 'b', label: 'B' },
    ]
    const { target } = setupToggles(richOpts, 'a')
    await tick()
    const items = Array.from(
      target.querySelectorAll<HTMLButtonElement>('[data-scope="toggle-group"][data-part="item"]'),
    )
    expect(items[0].querySelector('.tg-badge')?.textContent).toBe('NEW')
    expect(items[0].querySelector('.tg-hint')?.textContent).toBe('⌘1')
    expect(items[1].querySelector('.tg-badge')).toBeNull()
    expect(items[1].querySelector('.tg-hint')).toBeNull()
  })
})
