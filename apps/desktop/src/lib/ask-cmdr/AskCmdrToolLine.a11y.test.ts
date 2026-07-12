/**
 * Tier 3 a11y tests for `AskCmdrToolLine.svelte`.
 *
 * One collapsible "looked at X" line for a tool call. Covers the running state (a busy
 * status with a spinner), a finished-ok line with an expandable path, its expanded state,
 * and a refused line. `role="status"` + `aria-busy` and the toggle's `aria-expanded` are
 * the load-bearing attributes.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import AskCmdrToolLine from './AskCmdrToolLine.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { RailToolCall } from './ask-cmdr-trigger.svelte'

function tool(overrides: Partial<RailToolCall> = {}): RailToolCall {
  return { callId: 'c1', tool: 'list_dir', running: false, ok: true, path: null, ...overrides }
}

function mountLine(t: RailToolCall): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrToolLine, { target, props: { tool: t } })
  return target
}

describe('AskCmdrToolLine a11y', () => {
  it('a running tool line has no a11y violations', async () => {
    const target = mountLine(tool({ running: true }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a finished line with a path has no a11y violations', async () => {
    const target = mountLine(tool({ path: '/Users/me/Documents' }))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('an expanded line has no a11y violations', async () => {
    const target = mountLine(tool({ path: '/Users/me/Documents' }))
    await tick()
    const toggle = target.querySelector<HTMLButtonElement>('.tool-toggle')
    if (toggle === null) throw new Error('expected a .tool-toggle button')
    toggle.click()
    await tick()
    expect(target.querySelector('.detail')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('a refused line has no a11y violations', async () => {
    const target = mountLine(tool({ ok: false }))
    await tick()
    await expectNoA11yViolations(target)
  })
})
