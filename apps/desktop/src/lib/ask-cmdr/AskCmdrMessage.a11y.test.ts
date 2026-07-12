/**
 * Tier 3 a11y tests for `AskCmdrMessage.svelte`.
 *
 * One rendered thread item. Covers a user bubble, an assistant turn (tool lines +
 * "thinking…" + streaming markdown prose in a polite `aria-live` region), and a typed
 * failure notice. Takes its `message` as a prop, so no trigger-store wiring is needed.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import AskCmdrMessage from './AskCmdrMessage.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { RailMessage } from './ask-cmdr-trigger.svelte'

function mountMessage(message: RailMessage): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrMessage, { target, props: { message } })
  return target
}

describe('AskCmdrMessage a11y', () => {
  it('a user bubble has no a11y violations', async () => {
    const target = mountMessage({ kind: 'user', id: 1, text: 'What is my biggest folder?' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a streaming assistant turn with a tool line and thinking has no a11y violations', async () => {
    const target = mountMessage({
      kind: 'assistant',
      id: null,
      text: 'Your **Downloads** folder is the largest.',
      tools: [{ callId: 'c1', tool: 'largest_dirs', running: false, ok: true, path: '/Users/me' }],
      thinking: true,
      streaming: true,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a finished assistant turn has no a11y violations', async () => {
    const target = mountMessage({
      kind: 'assistant',
      id: 5,
      text: 'Here is a list:\n\n- one\n- two',
      tools: [],
      thinking: false,
      streaming: false,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a typed error notice has no a11y violations', async () => {
    const target = mountMessage({ kind: 'error', errorKind: 'rateLimited' })
    await tick()
    await expectNoA11yViolations(target)
  })
})
