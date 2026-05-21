/**
 * Behavior tests for `SearchBar.svelte`.
 *
 * The bar is purely presentational: one input, one mode-driven placeholder, an `onInput` callback.
 * The tests pin: placeholder text per mode, value mirrors the `query` prop, and `onInput` fires
 * with the new value as the user types.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchBar from './SearchBar.svelte'
import type { SearchMode } from './search-state.svelte'

function mountBar(overrides: Partial<{ query: string; mode: SearchMode }>): {
  input: HTMLInputElement
  onInput: ReturnType<typeof vi.fn>
  cleanup: () => void
} {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onInput = vi.fn()
  mount(SearchBar, {
    target,
    props: {
      inputElement: undefined,
      query: overrides.query ?? '',
      mode: overrides.mode ?? 'filename',
      disabled: false,
      aiHighlight: false,
      onInput,
    },
  })
  const input = target.querySelector('input.query-input') as HTMLInputElement | null
  if (!input) throw new Error('input not found')
  return {
    input,
    onInput,
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
})
