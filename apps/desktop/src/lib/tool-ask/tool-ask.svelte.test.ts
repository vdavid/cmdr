/**
 * The question's state and dialog: every way it comes down answers the backend, and it's always
 * under the arming guard.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'

const { answerToolAsk } = vi.hoisted(() => ({
  answerToolAsk: vi.fn<(id: string, response: unknown) => Promise<void>>(() => Promise.resolve()),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  answerToolAsk: (id: string, response: unknown) => answerToolAsk(id, response),
  onToolAskRequested: () => Promise.resolve(() => {}),
  onToolAskDismissed: () => Promise.resolve(() => {}),
  notifyDialogOpened: vi.fn(),
  notifyDialogClosed: vi.fn(),
}))

import ToolAskHost from './ToolAskHost.svelte'
import {
  abandonToolAsk,
  allowToolAsk,
  declineToolAsk,
  getToolAskQuestion,
  receiveToolAsk,
  receiveToolAskDismissed,
} from './tool-ask.svelte'
import { _resetOpenDialogsForTesting, markDialogOpen } from '#lib/ui/open-dialogs.svelte.js'

const QUIT = { requestId: 'r1', facts: { kind: 'quit' as const }, startedBy: 'agent' as const }

function overlay(): HTMLElement | null {
  return document.querySelector('[data-dialog-id="tool-ask"]')
}

beforeEach(() => {
  answerToolAsk.mockClear()
  _resetOpenDialogsForTesting()
})

afterEach(() => {
  receiveToolAskDismissed('r1')
})

describe('answers', () => {
  it.each([
    ['a yes', allowToolAsk, { kind: 'allowed' }],
    ['a no', declineToolAsk, { kind: 'declined' }],
    ['a broken dialog', abandonToolAsk, { kind: 'gone' }],
  ])('%s reaches the backend and takes the question down', (_name, act, expected) => {
    receiveToolAsk(QUIT)
    act()
    expect(answerToolAsk).toHaveBeenCalledExactlyOnceWith('r1', expected)
    expect(getToolAskQuestion()).toBeNull()
  })

  it('a second answer lands nowhere', () => {
    receiveToolAsk(QUIT)
    allowToolAsk()
    declineToolAsk()
    expect(answerToolAsk).toHaveBeenCalledTimes(1)
  })
})

describe('arriving and leaving', () => {
  it('a modal that opened since the backend read says who it is, and no question goes up', () => {
    markDialogOpen('about')
    receiveToolAsk(QUIT)
    expect(answerToolAsk).toHaveBeenCalledExactlyOnceWith('r1', { kind: 'blocked', dialog: 'about' })
    expect(getToolAskQuestion()).toBeNull()
  })

  it('the tool giving up takes it down, and only its own', () => {
    receiveToolAsk(QUIT)
    receiveToolAskDismissed('another')
    expect(getToolAskQuestion()).not.toBeNull()
    receiveToolAskDismissed('r1')
    expect(getToolAskQuestion()).toBeNull()
    expect(answerToolAsk).not.toHaveBeenCalled()
  })
})

describe('the dialog', () => {
  let target: HTMLElement
  let host: ReturnType<typeof mount>

  beforeEach(() => {
    target = document.createElement('div')
    document.body.appendChild(target)
    host = mount(ToolAskHost, { target })
    receiveToolAsk(QUIT)
    flushSync()
  })

  afterEach(async () => {
    await unmount(host)
    target.remove()
  })

  it('opens unarmed, whoever asked', () => {
    expect(overlay()?.getAttribute('data-armed')).toBe('false')
  })

  it('names who asked', () => {
    expect(overlay()?.textContent).toContain('Ask Cmdr started this.')
  })

  it('renders a value as text, never as markup', () => {
    receiveToolAskDismissed('r1')
    flushSync()
    receiveToolAsk({
      requestId: 'r2',
      startedBy: 'aiClient',
      facts: { kind: 'removeServer', hostId: 'h', name: '<img src=x onerror=alert(1)>' },
    })
    flushSync()
    expect(overlay()?.querySelector('img')).toBeNull()
    expect(overlay()?.textContent).toContain('<img src=x onerror=alert(1)>')
    receiveToolAskDismissed('r2')
  })

  it('closing it the way `dialog close` does is a no', () => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    overlay()?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    flushSync()
    expect(answerToolAsk).toHaveBeenCalledWith('r1', { kind: 'declined' })
  })
})
