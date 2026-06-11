/**
 * Behavior tests for `AiTransparencyStrip.svelte`.
 *
 * Pins:
 *   - The original prompt is rendered.
 *   - The "Here's what the agent did:" summary renders the pattern + filter lines (incl. type).
 *   - The caveat is rendered when present and hidden when empty.
 *   - The "Refine…" button is disabled and carries the "Coming soon" tooltip.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AiTransparencyStrip from './AiPromptStrip.svelte'
import type { AiSummary } from './ai-summary'

const EMPTY_SUMMARY: AiSummary = { pattern: null, patternKind: null, filters: [] }

function setup(props: { aiPrompt: string; caveat: string; summary?: AiSummary }): {
  target: HTMLDivElement
  cleanup: () => void
} {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AiTransparencyStrip, { target, props: { summary: EMPTY_SUMMARY, ...props } })
  return {
    target,
    cleanup: () => {
      target.remove()
    },
  }
}

describe('AiTransparencyStrip', () => {
  it('renders the original AI prompt', async () => {
    const { target, cleanup } = setup({ aiPrompt: 'screenshots from this week', caveat: '' })
    await tick()
    const prompt = target.querySelector('.ai-prompt')
    expect(prompt?.textContent).toBe('screenshots from this week')
    cleanup()
  })

  it('renders the agent lead-in', async () => {
    const { target, cleanup } = setup({ aiPrompt: 'photos', caveat: '' })
    await tick()
    expect(target.querySelector('.ai-summary-lead')?.textContent).toMatch(/here's what the agent did/i)
    cleanup()
  })

  it('renders the produced pattern with its labelled kind', async () => {
    const { target, cleanup } = setup({
      aiPrompt: 'images',
      caveat: '',
      summary: { pattern: '*.{jpg,png,heic}', patternKind: 'glob', filters: [] },
    })
    await tick()
    const labels = Array.from(target.querySelectorAll('.ai-summary-label')).map((e) => e.textContent)
    expect(labels).toContain('Glob:')
    expect(target.querySelector('.ai-summary-pattern')?.textContent).toBe('*.{jpg,png,heic}')
    cleanup()
  })

  it('renders the size, modified, and type filter summary lines', async () => {
    const { target, cleanup } = setup({
      aiPrompt: 'big old folders',
      caveat: '',
      summary: {
        pattern: '*',
        patternKind: 'glob',
        filters: [
          { label: 'Size', value: '> 5 MB' },
          { label: 'Modified', value: 'after 2026-01-01' },
          { label: 'Type', value: 'Folders only' },
        ],
      },
    })
    await tick()
    const text = target.querySelector('.ai-summary')?.textContent ?? ''
    expect(text).toContain('Size:')
    expect(text).toContain('> 5 MB')
    expect(text).toContain('Modified:')
    expect(text).toContain('Type:')
    expect(text).toContain('Folders only')
    cleanup()
  })

  it('shows a gentle hint when the AI produced no pattern or filters', async () => {
    const { target, cleanup } = setup({ aiPrompt: 'something vague', caveat: '' })
    await tick()
    expect(target.querySelector('.ai-summary-list')).toBeNull()
    expect(target.querySelector('.ai-summary-empty')).not.toBeNull()
    cleanup()
  })

  it('renders the caveat when present', async () => {
    const { target, cleanup } = setup({
      aiPrompt: 'big PDFs',
      caveat: "I treated 'big' as larger than 10 MB.",
    })
    await tick()
    const caveat = target.querySelector('.ai-caveat')
    expect(caveat?.textContent).toBe("I treated 'big' as larger than 10 MB.")
    cleanup()
  })

  it('does not render the caveat row when caveat is empty', async () => {
    const { target, cleanup } = setup({ aiPrompt: 'screenshots', caveat: '' })
    await tick()
    expect(target.querySelector('.ai-caveat')).toBeNull()
    cleanup()
  })

  it('renders a disabled "Refine…" button', async () => {
    const { target, cleanup } = setup({ aiPrompt: 'photos', caveat: '' })
    await tick()
    const button = target.querySelector<HTMLButtonElement>('.refine-button')
    expect(button).not.toBeNull()
    expect(button?.disabled).toBe(true)
    expect(button?.textContent.trim()).toBe('Refine…')
    cleanup()
  })

  it('communicates "coming soon" via the Refine button aria-label and use:tooltip text', async () => {
    vi.useFakeTimers()
    const { target, cleanup } = setup({ aiPrompt: 'photos', caveat: '' })
    await tick()
    const button = target.querySelector<HTMLButtonElement>('.refine-button')
    // aria-label is the always-available accessible signal that the control is a placeholder.
    expect(button?.getAttribute('aria-label')).toMatch(/coming soon/i)
    // The use:tooltip action stages its content on mouseenter behind a 400 ms delay.
    button?.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }))
    vi.advanceTimersByTime(500)
    const tip = document.body.querySelector('[role="tooltip"]')
    expect(tip?.textContent).toMatch(/coming soon: chat back to the agent/i)
    vi.useRealTimers()
    cleanup()
  })
})
