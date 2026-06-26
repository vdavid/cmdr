/**
 * Tests for the one-time double-click-to-parent hint toast.
 *
 * "Never do this again" turns the behavior off and dismisses; "I like it"
 * just dismisses (keeps the behavior on).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import DoubleClickPaneHintToastContent from './DoubleClickPaneHintToastContent.svelte'

const { setSetting, dismissToast } = vi.hoisted(() => ({
  setSetting: vi.fn((..._args: unknown[]) => Promise.resolve()),
  dismissToast: vi.fn((..._args: unknown[]) => undefined),
}))

vi.mock('$lib/settings', () => ({ setSetting }))
vi.mock('$lib/ui/toast', () => ({ dismissToast }))

beforeEach(() => {
  setSetting.mockClear()
  dismissToast.mockClear()
})

async function mountToast(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DoubleClickPaneHintToastContent, { target, props: { toastId: 'hint-1' } })
  await tick()
  return target
}

function buttonByText(target: HTMLElement, text: string): HTMLButtonElement {
  const btn = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === text)
  if (!btn) throw new Error(`button not found: ${text}`)
  return btn
}

describe('DoubleClickPaneHintToastContent', () => {
  it('renders the title, body, and both action buttons', async () => {
    const target = await mountToast()
    expect(target.textContent).toContain('What just happened?')
    expect(target.textContent).toContain('parent folder')
    buttonByText(target, 'Never do this again')
    buttonByText(target, 'I like it')
    target.remove()
  })

  it('"Never do this again" turns the setting off and dismisses', async () => {
    const target = await mountToast()
    buttonByText(target, 'Never do this again').click()
    await tick()
    expect(setSetting).toHaveBeenCalledWith('behavior.doubleClickPaneNavigatesToParent', false)
    expect(dismissToast).toHaveBeenCalledWith('hint-1')
    target.remove()
  })

  it('"I like it" only dismisses, leaving the setting untouched', async () => {
    const target = await mountToast()
    buttonByText(target, 'I like it').click()
    await tick()
    expect(setSetting).not.toHaveBeenCalled()
    expect(dismissToast).toHaveBeenCalledWith('hint-1')
    target.remove()
  })
})
