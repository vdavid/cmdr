/**
 * The rendered "these items are Locked" question (#408): an honest count, the
 * names it can show, and three buttons that each reach their answer. The host's
 * decisions have their own tests (`locked-sources.svelte.test.ts`).
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot, WriteLockedSourcesEvent } from '#lib/tauri-commands/index.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import LockedSourcesDialog from './LockedSourcesDialog.svelte'
import { buttonLabel } from './test-button-label'

function question(over: Partial<WriteLockedSourcesEvent> = {}): WriteLockedSourcesEvent {
  return {
    operationId: 'op-1',
    operationType: 'move',
    lockedCount: 7,
    lockedSample: ['/Docs/a.pdf', '/Docs/b.pdf', '/Docs/c.pdf', '/Docs/d.pdf', '/Docs/e.pdf'],
    ...over,
  }
}

const snapshot: OperationSnapshot = {
  operationId: 'op-1',
  operationType: 'move',
  status: 'running',
  source: 'Docs (7 items)',
  destination: 'Archive',
  rollbackSupport: 'crossVolumeMove',
  reverses: null,
  error: null,
  canRetry: false,
  conflictPolicy: null,
  startedAt: null,
  items: null,
}

let component: ReturnType<typeof mount> | null = null

function render(props: { event?: WriteLockedSourcesEvent; answering?: boolean } = {}) {
  const onMoveAnyway = vi.fn()
  const onSkip = vi.fn()
  const onCancel = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(LockedSourcesDialog, {
    target,
    props: {
      event: props.event ?? question(),
      snapshot,
      answering: props.answering ?? false,
      onMoveAnyway,
      onSkip,
      onCancel,
    },
  })
  flushSync()
  return { onMoveAnyway, onSkip, onCancel }
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

describe('LockedSourcesDialog', () => {
  it('says how many items are locked, what moving them does, and which move is asking', () => {
    render()
    const text = document.body.textContent
    expect(text).toContain('7 items you’re moving are locked.')
    expect(text).toContain('lock them again where they land')
    expect(text).toContain('Moving to Archive')
  })

  it('names the Locked items it knows, and counts the rest', () => {
    render()
    const names = [...document.body.querySelectorAll('.locked-name')].map((li) => li.textContent)
    expect(names).toEqual(['a.pdf', 'b.pdf', 'c.pdf', 'd.pdf', 'e.pdf'])
    expect(document.body.querySelector('.locked-more')?.textContent.trim()).toBe('and 2 more')
  })

  it('words one item as one', () => {
    render({ event: question({ lockedCount: 1, lockedSample: ['/Docs/a.pdf'] }) })
    expect(document.body.textContent).toContain('1 item you’re moving is locked.')
    expect(document.body.querySelector('.locked-more')).toBeNull()
  })

  it('reaches each answer from its button', () => {
    const { onMoveAnyway, onSkip, onCancel } = render()
    button('Move anyway').click()
    expect(onMoveAnyway).toHaveBeenCalledOnce()
    button('Skip locked items').click()
    expect(onSkip).toHaveBeenCalledOnce()
    button('Cancel').click()
    expect(onCancel).toHaveBeenCalledOnce()
  })

  it('disables every answer while one is in flight', () => {
    render({ answering: true })
    expect(buttons().every((b) => b.disabled)).toBe(true)
  })

  it('has no close button: every way out is a decision', () => {
    render()
    expect(document.body.querySelector('[aria-label="Close"], .close-button')).toBeNull()
  })
})
