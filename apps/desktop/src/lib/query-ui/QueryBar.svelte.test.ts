/**
 * Behavior tests for `QueryBar.svelte`.
 *
 * The bar is purely presentational: the house `TextInput` pill, a dropdown-trigger chevron,
 * and the house run `Button`. The tests pin: placeholder text per mode, value mirrors the
 * `query` prop, `onInput` fires with the new value as the user types, the run button's
 * "Search ⏎" label, and the chevron's toggle + `aria-expanded` contract.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import QueryBar from './QueryBar.svelte'
import type { SearchMode } from './query-filter-state.svelte'

function mountBar(
  overrides: Partial<{
    query: string
    mode: SearchMode
    showRunHint: boolean
    runHintCopy: string
    recentOpen: boolean
  }>,
): {
  target: HTMLDivElement
  input: HTMLInputElement
  onInput: ReturnType<typeof vi.fn>
  onRun: ReturnType<typeof vi.fn>
  onToggleRecent: ReturnType<typeof vi.fn>
  onCompositionStart: ReturnType<typeof vi.fn>
  onCompositionEnd: ReturnType<typeof vi.fn>
  cleanup: () => void
} {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onInput = vi.fn()
  const onRun = vi.fn()
  const onToggleRecent = vi.fn()
  const onCompositionStart = vi.fn()
  const onCompositionEnd = vi.fn()
  mount(QueryBar, {
    target,
    props: {
      inputElement: undefined,
      query: overrides.query ?? '',
      mode: overrides.mode ?? 'filename',
      disabled: false,
      aiHighlight: false,
      showRunHint: overrides.showRunHint ?? false,
      // The bar renders whatever hint it's handed; each dialog names its own verb.
      runHintCopy: overrides.runHintCopy ?? 'Press Enter to search',
      recentOpen: overrides.recentOpen ?? false,
      onInput,
      onRun,
      onToggleRecent,
      recentTriggerLabel: 'All recent searches',
      recentTriggerTooltip: 'Show all recent searches',
      onCompositionStart,
      onCompositionEnd,
    },
  })
  // `input.text-field-control` is the documented stable selector for the house text field
  // (`lib/ui/CLAUDE.md` § Text-field chrome); the E2E helpers key on it too.
  const input = target.querySelector<HTMLInputElement>('input.text-field-control')
  if (!input) throw new Error('input not found')
  return {
    target,
    input,
    onInput,
    onRun,
    onToggleRecent,
    onCompositionStart,
    onCompositionEnd,
    cleanup: () => {
      target.remove()
    },
  }
}

describe('QueryBar', () => {
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

  // The field is the house search pill: `radius="full"` plus the magnifier leading icon,
  // matching the Settings sidebar's search field.
  it('renders the field as the house search pill with the magnifier', async () => {
    const { target, cleanup } = mountBar({})
    await tick()
    const frame = target.querySelector('.text-field')
    expect(frame).not.toBeNull()
    expect(frame?.classList.contains('text-field-radius-full')).toBe(true)
    // Leading affix (magnifier) sits before the control inside the frame.
    expect(frame?.querySelector('.text-field-affix svg')).not.toBeNull()
    cleanup()
  })

  it('renders the run button and calls onRun when clicked', async () => {
    const { target, onRun, cleanup } = mountBar({})
    await tick()
    const button = target.querySelector<HTMLButtonElement>('button.btn')
    expect(button).not.toBeNull()
    button?.click()
    expect(onRun).toHaveBeenCalledTimes(1)
    cleanup()
  })

  // The run button is the house `Button` (same family as the footer actions), reading
  // "Search ⏎" with no leading icon.
  it('renders the run label as "Search ⏎" once, on the house Button', async () => {
    const { target, cleanup } = mountBar({})
    await tick()
    const button = target.querySelector('button.btn')
    expect(button).not.toBeNull()
    expect(button?.classList.contains('btn-secondary')).toBe(true)
    // No leading icon. The corner-down-left lucide icon used to live here.
    const svgs = button?.querySelectorAll('svg') ?? []
    expect(svgs.length).toBe(0)
    // Exactly one "⏎" hint chip inside the button.
    const enterHints = button?.querySelectorAll('.shortcut-chip') ?? []
    expect(enterHints.length).toBe(1)
    expect(enterHints[0]?.textContent).toBe('⏎')
    // Label then hint, in that order. The visible gap between them is the `--spacing-xs`
    // flex gap on `.run-label`, not literal whitespace, so the text runs together here.
    const text = button?.textContent.replace(/\s+/g, '').trim()
    expect(text).toBe('Search⏎')
    cleanup()
  })

  it('renders the dropdown trigger inside the pill and toggles on click', async () => {
    const { target, onToggleRecent, cleanup } = mountBar({})
    await tick()
    const trigger = target.querySelector<HTMLButtonElement>('.text-field .recent-trigger')
    expect(trigger).not.toBeNull()
    expect(trigger?.getAttribute('aria-label')).toBe('All recent searches')
    expect(trigger?.getAttribute('aria-haspopup')).toBe('dialog')
    expect(trigger?.getAttribute('aria-expanded')).toBe('false')
    trigger?.click()
    expect(onToggleRecent).toHaveBeenCalledTimes(1)
    cleanup()
  })

  it('marks the dropdown trigger expanded while the dropdown is open', async () => {
    const { target, cleanup } = mountBar({ recentOpen: true })
    await tick()
    const trigger = target.querySelector<HTMLButtonElement>('.recent-trigger')
    expect(trigger?.getAttribute('aria-expanded')).toBe('true')
    cleanup()
  })

  it('shows the run hint it was handed, and only when showRunHint is true', async () => {
    const { target, cleanup } = mountBar({ showRunHint: true })
    await tick()
    const hint = target.querySelector('.run-hint')
    expect(hint?.textContent).toMatch(/Press Enter to search/i)
    cleanup()

    // The copy is the caller's, not the bar's: Selection hands it a "filter" verb.
    const { target: filterTarget, cleanup: cleanupFilter } = mountBar({
      showRunHint: true,
      runHintCopy: 'Press Enter to filter',
    })
    await tick()
    expect(filterTarget.querySelector('.run-hint')?.textContent).toMatch(/Press Enter to filter/i)
    cleanupFilter()

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
