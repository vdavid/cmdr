/**
 * The note under a turn that reached a per-answer ceiling: the limit and its value, a link that
 * opens Send feedback, and "Keep going" only where continuing makes sense.
 */

import { describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'
import type { TurnLimit } from '#lib/ipc/bindings.js'

const { openFeedbackDialog } = vi.hoisted(() => ({ openFeedbackDialog: vi.fn() }))
vi.mock('#lib/feedback/feedback-flow.svelte.js', () => ({ openFeedbackDialog }))

import AskCmdrLimitNotice from './AskCmdrLimitNotice.svelte'

const ROUNDS: TurnLimit = { kind: 'toolRounds', max: 25, answered: true }

function render(canContinue: boolean): { target: HTMLElement; onContinue: ReturnType<typeof vi.fn> } {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onContinue = vi.fn()
  mount(AskCmdrLimitNotice, { target, props: { limit: ROUNDS, canContinue, onContinue } })
  flushSync()
  return { target, onContinue }
}

describe('AskCmdrLimitNotice', () => {
  it('names the limit, and its link opens Send feedback', () => {
    const { target } = render(false)
    expect(target.textContent).toContain('25 rounds')
    const link = target.querySelector<HTMLButtonElement>('button.link-button')
    expect(link).not.toBeNull()
    link?.click()
    expect(openFeedbackDialog).toHaveBeenCalledOnce()
  })

  it('offers Keep going only where continuing makes sense', () => {
    expect(render(false).target.querySelectorAll('button').length).toBe(1)
    const { target, onContinue } = render(true)
    const buttons = [...target.querySelectorAll<HTMLButtonElement>('button')]
    expect(buttons.length).toBe(2)
    buttons.find((button) => !button.classList.contains('link-button'))?.click()
    expect(onContinue).toHaveBeenCalledOnce()
  })
})
