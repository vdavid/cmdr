/**
 * Behavior tests for `AskCmdrComposer.svelte`: Enter sends, the button flips to Stop while
 * streaming, Escape returns focus to the panes, and the AI-provider gate (a setup gap in the
 * slot status ⇒ Send disabled, live as the status changes). The trigger store is mocked so the
 * composer mounts without the full app chain; the slot status is the real `$state`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { state, spies } = vi.hoisted(() => ({
  state: {
    streaming: false,
    attachments: [] as unknown[],
    composerFocusRequest: 0,
    composerDraft: null as string | null,
  },
  spies: {
    sendMessage: vi.fn(),
    stopStreaming: vi.fn(),
    markRailFocused: vi.fn(),
    returnFocusToPane: vi.fn(),
    addAttachments: vi.fn(),
    removeAttachment: vi.fn(),
  },
}))

vi.mock('./ask-cmdr-trigger.svelte', () => ({
  askCmdrState: state,
  sendMessage: spies.sendMessage,
  stopStreaming: spies.stopStreaming,
  markRailFocused: spies.markRailFocused,
  returnFocusToPane: spies.returnFocusToPane,
  addAttachments: spies.addAttachments,
  removeAttachment: spies.removeAttachment,
}))

const { addToast } = vi.hoisted(() => ({ addToast: vi.fn() }))
vi.mock('#lib/tauri-commands/index.js', () => ({}))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast }))

import AskCmdrComposer from './AskCmdrComposer.svelte'
import { slotStatusState } from './ask-cmdr-slot-status.svelte'
import type { SelectionAttachmentsRead } from '#lib/file-explorer/pane/selection-attachments-read.js'
import type { ProviderReadiness, SlotStatus } from '#lib/tauri-commands/index.js'

function statusWith(readiness: ProviderReadiness): SlotStatus {
  return { readiness, nextCall: null, baseline: null }
}

function mountComposer(props: { readSelection?: () => Promise<SelectionAttachmentsRead> } = {}): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const readSelection = props.readSelection ?? (() => Promise.resolve({ kind: 'refs' as const, refs: [], leftOut: 0 }))
  mount(AskCmdrComposer, { target, props: { readSelection } })
  return target
}

function attachButtonOf(target: HTMLElement): HTMLButtonElement {
  const btn = target.querySelector<HTMLButtonElement>('.composer-button.ghost')
  if (btn === null) throw new Error('expected an attach button')
  return btn
}

const rows = ['/x/row1', '/x/row2', '/x/row3'].map((path) => ({ path, kind: 'file' as const }))

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
  slotStatusState.status = statusWith('ready')
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

  it('takes a handed-over draft once, unsent, and clears it', async () => {
    state.composerDraft = 'Tell me more about this.'
    const target = mountComposer()
    await tick()
    expect(textareaOf(target).value).toBe('Tell me more about this.')
    expect(state.composerDraft).toBeNull()
    expect(spies.sendMessage).not.toHaveBeenCalled()
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

  // Send and Stop swap as a turn starts and ends: a focused one leaving the page would drop
  // focus to <body> (the focus watchdog's "activeElement=body" lines).
  it('Stop hands focus back to the prompt box', async () => {
    state.streaming = true
    const target = mountComposer()
    await tick()
    const stop = target.querySelector<HTMLButtonElement>('.composer-button.stop')
    if (stop === null) throw new Error('expected a stop button while streaming')
    stop.focus()
    stop.click()
    expect(document.activeElement).toBe(textareaOf(target))
  })

  it('a send from the Send button hands focus back to the prompt box', async () => {
    const target = mountComposer()
    await tick()
    const ta = textareaOf(target)
    await typeInto(ta, 'hello')
    const send = sendButtonOf(target)
    send.focus()
    send.click()
    expect(spies.sendMessage).toHaveBeenCalledWith('hello')
    expect(document.activeElement).toBe(ta)
  })

  describe('Ask about selection', () => {
    beforeEach(() => {
      spies.addAttachments.mockReset()
      addToast.mockReset()
    })

    it('attaches every selected row, loaded on screen or not', async () => {
      const target = mountComposer({ readSelection: () => Promise.resolve({ kind: 'refs', refs: rows, leftOut: 0 }) })
      await tick()
      attachButtonOf(target).click()
      await vi.waitFor(() => {
        expect(spies.addAttachments).toHaveBeenCalled()
      })

      expect(spies.addAttachments).toHaveBeenCalledWith(rows)
      expect(addToast).not.toHaveBeenCalled()
    })

    it('says how many rows a capped selection left out', async () => {
      const target = mountComposer({
        readSelection: () => Promise.resolve({ kind: 'refs', refs: rows, leftOut: 1_234 }),
      })
      await tick()
      attachButtonOf(target).click()
      await vi.waitFor(() => {
        expect(addToast).toHaveBeenCalledTimes(1)
      })

      expect(spies.addAttachments).toHaveBeenCalledWith(rows)
      expect(String(addToast.mock.calls[0][0])).toContain('1,237')
    })

    it('attaches nothing when the rows never settled', async () => {
      const target = mountComposer({ readSelection: () => Promise.resolve({ kind: 'changed' }) })
      await tick()
      attachButtonOf(target).click()
      await tick()
      await tick()

      expect(spies.addAttachments).not.toHaveBeenCalled()
    })
  })

  describe('AI-provider gate', () => {
    it('with a provider ready, typing enables Send', async () => {
      const target = mountComposer()
      await tick()
      await typeInto(textareaOf(target), 'hi')
      expect(sendButtonOf(target).disabled).toBe(false)
    })

    it('with a setup gap, Send stays disabled and Enter does nothing', async () => {
      slotStatusState.status = statusWith('cloudNeedsKey')
      const target = mountComposer()
      await tick()
      const ta = textareaOf(target)
      await typeInto(ta, 'hi')
      expect(sendButtonOf(target).disabled).toBe(true)
      ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      expect(spies.sendMessage).not.toHaveBeenCalled()
    })

    it('before the slot status is read, Send is allowed (the backend refuses an unset provider itself)', async () => {
      slotStatusState.status = null
      const target = mountComposer()
      await tick()
      await typeInto(textareaOf(target), 'hi')
      expect(sendButtonOf(target).disabled).toBe(false)
    })

    it('a provider set up while the rail is open enables Send live, without remount', async () => {
      slotStatusState.status = statusWith('off')
      const target = mountComposer()
      await tick()
      await typeInto(textareaOf(target), 'hi')
      expect(sendButtonOf(target).disabled).toBe(true)

      slotStatusState.status = statusWith('ready')
      await tick()
      expect(sendButtonOf(target).disabled).toBe(false)
    })
  })
})
