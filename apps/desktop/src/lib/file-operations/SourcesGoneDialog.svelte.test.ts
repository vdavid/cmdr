/**
 * The rendered "sources left while this queued" question (ERR-962JD): an honest
 * count, the names it can show, the operation's own verb on the button, and
 * buttons that reach the host. The host's decisions have their own tests
 * (`sources-gone.svelte.test.ts`).
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteSourcesGoneEvent } from '#lib/tauri-commands/index.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import SourcesGoneDialog from './SourcesGoneDialog.svelte'
import { buttonLabel } from './test-button-label'

function question(over: Partial<WriteSourcesGoneEvent> = {}): WriteSourcesGoneEvent {
  return {
    operationId: 'op-1',
    operationType: 'move',
    totalCount: 22,
    goneCount: 7,
    goneSample: [
      '/Music/CD1/01.mp3',
      '/Music/CD1/02.mp3',
      '/Music/CD1/03.mp3',
      '/Music/CD1/04.mp3',
      '/Music/CD1/05.mp3',
    ],
    next: 'awaiting_answer',
    ...over,
  }
}

const snapshot: OperationSnapshot = {
  operationId: 'op-1',
  operationType: 'move',
  status: 'running',
  source: 'CD1 (22 items)',
  destination: 'Music',
  rollbackSupport: 'crossVolumeMove',
  reverses: null,
  error: null,
  canRetry: false,
  conflictPolicy: null,
  startedAt: null,
  items: null,
}

let component: ReturnType<typeof mount> | null = null

function render(props: { event?: WriteSourcesGoneEvent; answering?: boolean } = {}) {
  const onContinue = vi.fn()
  const onCancel = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(SourcesGoneDialog, {
    target,
    props: {
      event: props.event ?? question(),
      snapshot,
      answering: props.answering ?? false,
      onContinue,
      onCancel,
    },
  })
  flushSync()
  return { onContinue, onCancel }
}

function buttons(): HTMLButtonElement[] {
  return [...document.body.querySelectorAll<HTMLButtonElement>('.modal-dialog button')]
}

function button(label: string): HTMLButtonElement {
  const found = buttons().find((b) => buttonLabel(b) === label)
  if (!found) throw new Error(`no "${label}" button among: ${buttons().map(buttonLabel).join(', ')}`)
  return found
}

afterEach(() => {
  if (component) void unmount(component)
  component = null
  document.body.innerHTML = ''
})

describe('SourcesGoneDialog', () => {
  it('says how many of how many items are gone, and which operation is asking', () => {
    render()
    const text = document.body.textContent
    expect(text).toContain('7 of the 22 items this move was waiting on were moved or deleted since you queued it.')
    expect(text).toContain('Moving to Music')
  })

  it('names the gone items it knows, and counts the rest', () => {
    render()
    const names = [...document.body.querySelectorAll('.gone-name')].map((li) => li.textContent)
    expect(names).toEqual(['01.mp3', '02.mp3', '03.mp3', '04.mp3', '05.mp3'])
    expect(document.body.querySelector('.gone-more')?.textContent.trim()).toBe('and 2 more')
  })

  it('offers the operation’s own verb, and each button reaches its answer', () => {
    const { onContinue, onCancel } = render()
    button('Move the rest').click()
    expect(onContinue).toHaveBeenCalledOnce()
    button('Cancel').click()
    expect(onCancel).toHaveBeenCalledOnce()
  })

  it('words a copy as a copy', () => {
    render({ event: question({ operationType: 'copy', goneCount: 1, totalCount: 3, goneSample: ['/a/b.txt'] }) })
    expect(document.body.textContent).toContain('1 of the 3 items this copy was waiting on was moved or deleted')
    expect(button('Copy the rest')).toBeTruthy()
    expect(document.body.querySelector('.gone-more')).toBeNull()
  })

  it('disables both answers while one is in flight', () => {
    render({ answering: true })
    expect(buttons().every((b) => b.disabled)).toBe(true)
  })

  it('has no close button: both ways out are decisions', () => {
    render()
    expect(document.body.querySelector('[aria-label="Close"], .close-button')).toBeNull()
  })
})
