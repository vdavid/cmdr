/**
 * Behavior tests for `SearchModeChips.svelte`.
 *
 * Pins:
 *   - All four chips render when AI is enabled; AI chip is hidden when AI is off.
 *   - Active chip has `aria-selected="true"`; others have `aria-selected="false"`.
 *   - Clicking a chip calls `onSelect` with the chip's mode key.
 *   - The Content chip is disabled (visible-disabled) and never fires `onSelect`.
 *   - ←/→ moves focus between chips and skips the disabled Content chip.
 *   - Space and Enter on a focused chip activate it.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchModeChips from './SearchModeChips.svelte'
import type { SearchMode } from './search-state.svelte'

function setup(overrides: Partial<{ mode: SearchMode; aiEnabled: boolean; disabled: boolean }> = {}): {
  target: HTMLDivElement
  chips: HTMLButtonElement[]
  onSelect: ReturnType<typeof vi.fn>
  cleanup: () => void
} {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onSelect = vi.fn()
  mount(SearchModeChips, {
    target,
    props: {
      mode: overrides.mode ?? 'filename',
      aiEnabled: overrides.aiEnabled ?? true,
      disabled: overrides.disabled ?? false,
      onSelect,
    },
  })
  const chips = Array.from(target.querySelectorAll<HTMLButtonElement>('.mode-chip'))
  return {
    target,
    chips,
    onSelect,
    cleanup: () => {
      target.remove()
    },
  }
}

describe('SearchModeChips', () => {
  it('renders 4 chips when AI is enabled (AI, Filename, Content, Regex)', async () => {
    const { chips, cleanup } = setup({ aiEnabled: true })
    await tick()
    expect(chips).toHaveLength(4)
    const labels = chips.map((c) => c.textContent?.trim())
    expect(labels[0]).toMatch(/AI\s*Ask anything/)
    expect(labels[1]).toBe('Filename')
    expect(labels[2]).toBe('Content')
    expect(labels[3]).toBe('Regex')
    cleanup()
  })

  it('hides the AI chip when AI is disabled', async () => {
    const { chips, cleanup } = setup({ aiEnabled: false, mode: 'filename' })
    await tick()
    expect(chips).toHaveLength(3)
    const labels = chips.map((c) => c.textContent?.trim())
    expect(labels[0]).toBe('Filename')
    expect(labels[1]).toBe('Content')
    expect(labels[2]).toBe('Regex')
    cleanup()
  })

  it('marks the active chip with aria-selected="true"', async () => {
    const { chips, cleanup } = setup({ aiEnabled: true, mode: 'regex' })
    await tick()
    const regexChip = chips[3]
    expect(regexChip.getAttribute('aria-selected')).toBe('true')
    expect(chips[0].getAttribute('aria-selected')).toBe('false')
    cleanup()
  })

  it('clicking a chip fires onSelect with its mode', async () => {
    const { chips, onSelect, cleanup } = setup({ aiEnabled: true, mode: 'filename' })
    await tick()
    chips[3].click()
    expect(onSelect).toHaveBeenCalledWith('regex')
    cleanup()
  })

  it('Content chip is disabled and never fires onSelect', async () => {
    const { chips, onSelect, cleanup } = setup({ aiEnabled: true })
    await tick()
    const contentChip = chips[2]
    expect(contentChip.disabled).toBe(true)
    contentChip.click()
    expect(onSelect).not.toHaveBeenCalled()
    cleanup()
  })

  it('Content chip has a Coming soon aria-label', async () => {
    const { chips, cleanup } = setup({ aiEnabled: true })
    await tick()
    const contentChip = chips[2]
    expect(contentChip.getAttribute('aria-label')).toMatch(/coming soon/i)
    cleanup()
  })

  it('ArrowRight on the active chip moves focus to the next interactive chip (skipping Content)', async () => {
    const { chips, cleanup } = setup({ aiEnabled: true, mode: 'filename' })
    await tick()
    chips[1].focus()
    chips[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await tick()
    // Skip Content (chips[2]) and land on Regex (chips[3]).
    expect(document.activeElement).toBe(chips[3])
    cleanup()
  })

  it('ArrowLeft moves focus to the previous interactive chip', async () => {
    const { chips, cleanup } = setup({ aiEnabled: true, mode: 'regex' })
    await tick()
    chips[3].focus()
    chips[3].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }))
    await tick()
    expect(document.activeElement).toBe(chips[1])
    cleanup()
  })

  it('Enter on a focused chip activates it', async () => {
    const { chips, onSelect, cleanup } = setup({ aiEnabled: true, mode: 'filename' })
    await tick()
    chips[3].focus()
    chips[3].dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    expect(onSelect).toHaveBeenCalledWith('regex')
    cleanup()
  })

  it('Space on a focused chip activates it', async () => {
    const { chips, onSelect, cleanup } = setup({ aiEnabled: true, mode: 'filename' })
    await tick()
    chips[0].focus()
    chips[0].dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true }))
    expect(onSelect).toHaveBeenCalledWith('ai')
    cleanup()
  })

  it('the active chip is the focusable one (tabindex=0); others are tabindex=-1', async () => {
    const { chips, cleanup } = setup({ aiEnabled: true, mode: 'regex' })
    await tick()
    expect(chips[3].getAttribute('tabindex')).toBe('0')
    expect(chips[1].getAttribute('tabindex')).toBe('-1')
    cleanup()
  })
})
