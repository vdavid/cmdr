/**
 * Behavior tests for `AiTransparencyStrip.svelte`.
 *
 * Pins:
 *   - The original prompt is rendered.
 *   - The caveat is rendered when present and hidden when empty.
 *   - The "Refine…" button is disabled and carries the "Coming soon" tooltip.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AiTransparencyStrip from './AiTransparencyStrip.svelte'

function setup(props: { aiPrompt: string; caveat: string }): { target: HTMLDivElement; cleanup: () => void } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AiTransparencyStrip, { target, props })
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
