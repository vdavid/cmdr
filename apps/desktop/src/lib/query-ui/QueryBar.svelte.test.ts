/**
 * Behavior tests for `SearchBar.svelte`.
 *
 * The bar is purely presentational: one input, one mode-driven placeholder, an `onInput` callback.
 * The tests pin: placeholder text per mode, value mirrors the `query` prop, and `onInput` fires
 * with the new value as the user types.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchBar from './QueryBar.svelte'
import type { SearchMode } from './query-filter-state.svelte'

function mountBar(overrides: Partial<{ query: string; mode: SearchMode; showRunHint: boolean }>): {
  target: HTMLDivElement
  input: HTMLInputElement
  onInput: ReturnType<typeof vi.fn>
  onRun: ReturnType<typeof vi.fn>
  onCompositionStart: ReturnType<typeof vi.fn>
  onCompositionEnd: ReturnType<typeof vi.fn>
  cleanup: () => void
} {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onInput = vi.fn()
  const onRun = vi.fn()
  const onCompositionStart = vi.fn()
  const onCompositionEnd = vi.fn()
  mount(SearchBar, {
    target,
    props: {
      inputElement: undefined,
      query: overrides.query ?? '',
      mode: overrides.mode ?? 'filename',
      disabled: false,
      aiHighlight: false,
      showRunHint: overrides.showRunHint ?? false,
      onInput,
      onRun,
      onCompositionStart,
      onCompositionEnd,
    },
  })
  const input = target.querySelector<HTMLInputElement>('input.query-input')
  if (!input) throw new Error('input not found')
  return {
    target,
    input,
    onInput,
    onRun,
    onCompositionStart,
    onCompositionEnd,
    cleanup: () => {
      target.remove()
    },
  }
}

describe('SearchBar', () => {
  it('shows the filename placeholder when mode is filename', async () => {
    const { input, cleanup } = mountBar({ mode: 'filename' })
    await tick()
    expect(input.placeholder).toMatch(/Filename pattern/i)
    cleanup()
  })

  it('shows the regex placeholder when mode is regex', async () => {
    const { input, cleanup } = mountBar({ mode: 'regex' })
    await tick()
    expect(input.placeholder).toMatch(/regular expression/i)
    cleanup()
  })

  it('shows the AI placeholder when mode is ai', async () => {
    const { input, cleanup } = mountBar({ mode: 'ai' })
    await tick()
    expect(input.placeholder).toMatch(/describe what you/i)
    cleanup()
  })

  it('mirrors the query prop into the input value', async () => {
    const { input, cleanup } = mountBar({ query: '*.pdf' })
    await tick()
    expect(input.value).toBe('*.pdf')
    cleanup()
  })

  it('fires onInput with the new value when the user types', async () => {
    const { input, onInput, cleanup } = mountBar({})
    input.value = 'photo*'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(onInput).toHaveBeenCalledWith('photo*')
    cleanup()
  })

  it('renders the ⏎ run button and calls onRun when clicked', async () => {
    const { target, onRun, cleanup } = mountBar({})
    await tick()
    const button = target.querySelector<HTMLButtonElement>('button.run-button')
    expect(button).not.toBeNull()
    button?.click()
    expect(onRun).toHaveBeenCalledTimes(1)
    cleanup()
  })

  // The run button renders "Search ⏎" exactly once, with no leading icon. Pins:
  //   1. No leading icon (no corner-down-left SVG).
  //   2. The "Search" label is followed by exactly one "⏎" hint.
  //   3. The hint is separated from "Search" with a visible space (rendered via a
  //      spacing gap on the inline-flex parent, so we just assert the textContent
  //      reads "Search ⏎" with a space between them).
  it('renders the run label as "Search ⏎" once, no leading icon', async () => {
    const { target, cleanup } = mountBar({})
    await tick()
    const button = target.querySelector('button.run-button')
    expect(button).not.toBeNull()
    // No leading icon. The corner-down-left lucide icon used to live here.
    const svgs = button?.querySelectorAll('svg') ?? []
    expect(svgs.length).toBe(0)
    // Exactly one "⏎" hint chip inside the button.
    const enterHints = button?.querySelectorAll('.shortcut-chip') ?? []
    expect(enterHints.length).toBe(1)
    expect(enterHints[0]?.textContent).toBe('⏎')
    // The visible label reads "Search ⏎" (a single space between "Search" and "⏎").
    const text = button?.textContent.replace(/\s+/g, ' ').trim()
    expect(text).toBe('Search ⏎')
    cleanup()
  })

  it('shows the "Press Enter to search" hint only when showRunHint is true', async () => {
    const { target, cleanup } = mountBar({ showRunHint: true })
    await tick()
    const hint = target.querySelector('.run-hint')
    expect(hint?.textContent).toMatch(/Press Enter to search/i)
    cleanup()

    const { target: noHintTarget, cleanup: cleanup2 } = mountBar({ showRunHint: false })
    await tick()
    expect(noHintTarget.querySelector('.run-hint')).toBeNull()
    cleanup2()
  })

  it('forwards compositionstart and compositionend to the parent (IME guard)', async () => {
    const { input, onCompositionStart, onCompositionEnd, cleanup } = mountBar({})
    await tick()
    input.dispatchEvent(new CompositionEvent('compositionstart'))
    expect(onCompositionStart).toHaveBeenCalledTimes(1)
    input.dispatchEvent(new CompositionEvent('compositionend'))
    expect(onCompositionEnd).toHaveBeenCalledTimes(1)
    cleanup()
  })
})
