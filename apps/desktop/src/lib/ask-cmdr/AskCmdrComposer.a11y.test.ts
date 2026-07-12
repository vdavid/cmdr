/**
 * Tier 3 a11y tests for `AskCmdrComposer.svelte`.
 *
 * The message input plus its send/stop button. Covers the idle state (labeled input +
 * disabled send) and the streaming state (the button flips to Stop). The trigger store is
 * mocked to a plain object so the composer mounts without the full explorer-state chain.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// `vi.hoisted` so the shared mutable state exists before the hoisted `vi.mock` factory runs.
const { triggerState } = vi.hoisted(() => ({ triggerState: { streaming: false, attachments: [] as unknown[] } }))
vi.mock('./ask-cmdr-trigger.svelte', () => ({
  askCmdrState: triggerState,
  sendMessage: vi.fn(),
  stopStreaming: vi.fn(),
  markRailFocused: vi.fn(),
  returnFocusToPane: vi.fn(),
  addAttachments: vi.fn(),
  removeAttachment: vi.fn(),
}))

import AskCmdrComposer from './AskCmdrComposer.svelte'

function mountComposer(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrComposer, { target, props: {} })
  return target
}

beforeEach(() => {
  triggerState.streaming = false
})

describe('AskCmdrComposer a11y', () => {
  it('the idle composer has no a11y violations', async () => {
    const target = mountComposer()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the streaming composer (stop button) has no a11y violations', async () => {
    triggerState.streaming = true
    const target = mountComposer()
    await tick()
    await expectNoA11yViolations(target)
  })
})
