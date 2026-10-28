/**
 * Send and Stop are one slot that swaps as a turn starts and ends. A swap the user didn't click
 * (the turn ended by itself, or a running turn was picked up) takes the focused button off the
 * page, and WebKit then drops focus to `<body>`, where the keyboard reaches nothing. These mount
 * the composer over the REAL rail state, so the swap is the one production makes.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn() }))
vi.mock('#lib/app-status-store.js', () => ({ saveAppStatus: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))
vi.mock('./rail-window', () => ({
  growMainWindowForRail: vi.fn(() => Promise.resolve()),
  shrinkMainWindowForRail: vi.fn(() => Promise.resolve()),
}))

import AskCmdrComposer from './AskCmdrComposer.svelte'
import { askCmdrState } from './ask-cmdr-trigger.svelte'

let target: HTMLDivElement
let component: Record<string, unknown> | null = null

function mountComposer(): void {
  target = document.createElement('div')
  document.body.appendChild(target)
  const readSelection = () => Promise.resolve({ kind: 'refs' as const, refs: [], leftOut: 0 })
  component = mount(AskCmdrComposer, { target, props: { readSelection } })
  flushSync()
}

function swapButton(): HTMLButtonElement {
  const button = target.querySelector<HTMLButtonElement>('.composer-row > button:not(.ghost), .send-wrap > button')
  if (button === null) throw new Error('expected the Send/Stop button')
  return button
}

function textarea(): HTMLTextAreaElement {
  const ta = target.querySelector('textarea')
  if (ta === null) throw new Error('expected the prompt box')
  return ta
}

beforeEach(() => {
  askCmdrState.streaming = false
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  target.remove()
  askCmdrState.streaming = false
})

describe('the Send/Stop swap', () => {
  it('hands focus to the prompt box when a turn ends by itself while Stop has it', () => {
    askCmdrState.streaming = true
    mountComposer()
    swapButton().focus()
    expect(document.activeElement).toBe(swapButton())

    askCmdrState.streaming = false
    flushSync()

    expect(document.activeElement).toBe(textarea())
  })

  it('hands focus to the prompt box when a turn starts under a focused Send', () => {
    mountComposer()
    textarea().value = 'hello'
    textarea().dispatchEvent(new Event('input', { bubbles: true }))
    flushSync()
    swapButton().focus()

    askCmdrState.streaming = true
    flushSync()

    expect(document.activeElement).toBe(textarea())
  })

  it('leaves focus alone when it was elsewhere', () => {
    askCmdrState.streaming = true
    mountComposer()
    const elsewhere = document.createElement('button')
    document.body.appendChild(elsewhere)
    elsewhere.focus()

    askCmdrState.streaming = false
    flushSync()

    expect(document.activeElement).toBe(elsewhere)
    elsewhere.remove()
  })
})
