/**
 * Behavior tests for `AskCmdrComposer.svelte`: Enter sends, the button flips to Stop while
 * streaming, Escape returns focus to the panes, and the AI-provider gate (provider off ⇒
 * Send disabled + inline hint, live across a settings flip). The trigger store and the
 * settings module are mocked so the composer mounts without the full app chain.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { state, spies } = vi.hoisted(() => ({
  state: { streaming: false, attachments: [] as unknown[] },
  spies: {
    sendMessage: vi.fn(),
    stopStreaming: vi.fn(),
    markRailFocused: vi.fn(),
    returnFocusToPane: vi.fn(),
    addAttachments: vi.fn(),
    removeAttachment: vi.fn(),
  },
}))

// Controllable `ai.provider` setting: `setProvider` mutates the value AND notifies the
// live listeners the composer registers, so a test can flip the provider with no remount.
const settings = vi.hoisted(() => {
  const store = { provider: 'cloud' as string }
  const listeners = new Set<(id: string, v: unknown) => void>()
  return {
    getSetting: (id: string) => (id === 'ai.provider' ? store.provider : undefined),
    onSpecificSettingChange: (id: string, cb: (id: string, v: unknown) => void) => {
      if (id === 'ai.provider') listeners.add(cb)
      return () => listeners.delete(cb)
    },
    setProvider(v: string): void {
      store.provider = v
      for (const cb of listeners) cb('ai.provider', v)
    },
    reset(provider: string): void {
      listeners.clear()
      store.provider = provider
    },
  }
})

vi.mock('./ask-cmdr-trigger.svelte', () => ({
  askCmdrState: state,
  sendMessage: spies.sendMessage,
  stopStreaming: spies.stopStreaming,
  markRailFocused: spies.markRailFocused,
  returnFocusToPane: spies.returnFocusToPane,
  addAttachments: spies.addAttachments,
  removeAttachment: spies.removeAttachment,
}))

vi.mock('$lib/settings', () => ({
  getSetting: settings.getSetting,
  onSpecificSettingChange: settings.onSpecificSettingChange,
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

function sendButtonOf(target: HTMLElement): HTMLButtonElement {
  const btn = target.querySelector<HTMLButtonElement>('.composer-button:not(.ghost):not(.stop)')
  if (btn === null) throw new Error('expected a send button')
  return btn
}

async function typeInto(ta: HTMLTextAreaElement, value: string): Promise<void> {
  ta.value = value
  ta.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
}

beforeEach(() => {
  state.streaming = false
  settings.reset('cloud')
  spies.sendMessage.mockReset()
  spies.stopStreaming.mockReset()
  spies.returnFocusToPane.mockReset()
})

describe('AskCmdrComposer', () => {
  it('Enter sends the typed message', async () => {
    const target = mountComposer()
    await tick()
    const ta = textareaOf(target)
    await typeInto(ta, 'what is my biggest folder?')
    ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(spies.sendMessage).toHaveBeenCalledWith('what is my biggest folder?')
  })

  it('Shift+Enter does not send', async () => {
    const target = mountComposer()
    await tick()
    const ta = textareaOf(target)
    await typeInto(ta, 'line one')
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

  describe('AI-provider gate', () => {
    it('with a provider on, typing enables Send and shows no hint', async () => {
      const target = mountComposer()
      await tick()
      await typeInto(textareaOf(target), 'hi')
      expect(sendButtonOf(target).disabled).toBe(false)
      expect(target.querySelector('.provider-off-hint')).toBeNull()
    })

    it('with the provider off, Send stays disabled, Enter does nothing, and the hint shows', async () => {
      settings.reset('off')
      const target = mountComposer()
      await tick()
      const ta = textareaOf(target)
      await typeInto(ta, 'hi')
      expect(sendButtonOf(target).disabled).toBe(true)
      expect(target.querySelector('.provider-off-hint')).not.toBeNull()
      ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      expect(spies.sendMessage).not.toHaveBeenCalled()
    })

    it('flipping the provider off then on updates Send and the hint live, without remount', async () => {
      const target = mountComposer()
      await tick()
      await typeInto(textareaOf(target), 'hi')
      expect(sendButtonOf(target).disabled).toBe(false)
      expect(target.querySelector('.provider-off-hint')).toBeNull()

      settings.setProvider('off')
      await tick()
      expect(sendButtonOf(target).disabled).toBe(true)
      expect(target.querySelector('.provider-off-hint')).not.toBeNull()

      settings.setProvider('local')
      await tick()
      expect(sendButtonOf(target).disabled).toBe(false)
      expect(target.querySelector('.provider-off-hint')).toBeNull()
    })
  })
})
