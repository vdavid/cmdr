/**
 * Behavior tests for `AskCmdrComposer.svelte`: Enter sends, the button flips to Stop while
 * streaming, and Escape returns focus to the panes. The trigger store is mocked so the
 * composer mounts without the full explorer-state chain.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { state, spies } = vi.hoisted(() => ({
  state: { streaming: false },
  spies: {
    sendMessage: vi.fn(),
    stopStreaming: vi.fn(),
    markRailFocused: vi.fn(),
    returnFocusToPane: vi.fn(),
  },
}))

vi.mock('./ask-cmdr-trigger.svelte', () => ({
  askCmdrState: state,
  sendMessage: spies.sendMessage,
  stopStreaming: spies.stopStreaming,
  markRailFocused: spies.markRailFocused,
  returnFocusToPane: spies.returnFocusToPane,
}))

import AskCmdrComposer from './AskCmdrComposer.svelte'

function mountComposer(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrComposer, { target, props: {} })
  return target
}

function textareaOf(target: HTMLElement): HTMLTextAreaElement {
  const ta = target.querySelector('textarea')
  if (ta === null) throw new Error('expected a composer textarea')
  return ta
}

beforeEach(() => {
  state.streaming = false
  spies.sendMessage.mockReset()
  spies.stopStreaming.mockReset()
  spies.returnFocusToPane.mockReset()
})

describe('AskCmdrComposer', () => {
  it('Enter sends the typed message', async () => {
    const target = mountComposer()
    await tick()
    const ta = textareaOf(target)
    ta.value = 'what is my biggest folder?'
    ta.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(spies.sendMessage).toHaveBeenCalledWith('what is my biggest folder?')
  })

  it('Shift+Enter does not send', async () => {
    const target = mountComposer()
    await tick()
    const ta = textareaOf(target)
    ta.value = 'line one'
    ta.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true, bubbles: true }))
    expect(spies.sendMessage).not.toHaveBeenCalled()
  })

  it('Escape returns focus to the panes', async () => {
    const target = mountComposer()
    await tick()
    textareaOf(target).dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(spies.returnFocusToPane).toHaveBeenCalled()
  })

  it('the stop button cancels while streaming', async () => {
    state.streaming = true
    const target = mountComposer()
    await tick()
    const stop = target.querySelector<HTMLButtonElement>('.composer-button.stop')
    if (stop === null) throw new Error('expected a stop button while streaming')
    stop.click()
    expect(spies.stopStreaming).toHaveBeenCalled()
  })
})
