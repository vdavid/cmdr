/**
 * The group, the expand, and the preview as they render: a header that toggles and hands focus
 * back before a collapse drops it, an expand that only a stored result allows, a preview that
 * shows rows and notes and copies the raw JSON, and the bubble's one announcement per group.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick } from 'svelte'
import type { ToolResultDetailView } from '#lib/ipc/bindings.js'
import type { AssistantItem, RailToolCall } from './ask-cmdr-messages'
import type { DisplayLine } from './ask-cmdr-rounds'

const { copyToClipboard, askCmdrToolResult, returnFocusToPane } = vi.hoisted(() => ({
  copyToClipboard: vi.fn<(text: string) => Promise<void>>(),
  askCmdrToolResult: vi.fn(),
  returnFocusToPane: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ copyToClipboard, askCmdrToolResult }))
vi.mock('./ask-cmdr-trigger.svelte', () => ({ returnFocusToPane, undoRename: vi.fn() }))

import AskCmdrToolGroup from './AskCmdrToolGroup.svelte'
import AskCmdrToolLine from './AskCmdrToolLine.svelte'
import AskCmdrToolPreview from './AskCmdrToolPreview.svelte'
import AskCmdrMessage from './AskCmdrMessage.svelte'
import { askCmdrState } from './ask-cmdr-state.svelte'
import { _resetToolGroupsForTests, setGroupOpen, type ToolDetailState } from './ask-cmdr-tool-groups.svelte'

function call(callId: string, overrides: Partial<RailToolCall> = {}): RailToolCall {
  return {
    callId,
    tool: 'list_dir',
    running: false,
    subject: { kind: 'folder', path: { text: `/Users/me/${callId}`, linkable: true } },
    outcome: { kind: 'items', returned: 2, total: 2, lowerBound: false },
    repeatOf: null,
    ...overrides,
  }
}

function line(callId: string, overrides: Partial<RailToolCall> = {}, messageId: number | null = 5): DisplayLine {
  return {
    key: `${messageId === null ? 'r0' : String(messageId)}:${callId}`,
    messageId,
    occurrence: 0,
    call: call(callId, overrides),
  }
}

function target(): HTMLElement {
  const element = document.createElement('div')
  document.body.appendChild(element)
  return element
}

const revealPath = vi.fn(() => Promise.resolve())

beforeEach(() => {
  _resetToolGroupsForTests()
  copyToClipboard.mockReset()
  askCmdrToolResult.mockReset()
  returnFocusToPane.mockReset()
  revealPath.mockClear()
  document.body.innerHTML = ''
})

describe('AskCmdrToolGroup', () => {
  it('sums up its lines in one header that opens and closes the group', async () => {
    const element = target()
    mount(AskCmdrToolGroup, { target: element, props: { lines: [line('a'), line('b')], live: false, revealPath } })
    flushSync()
    const toggle = element.querySelector<HTMLButtonElement>('.group-toggle')
    expect(toggle?.textContent.trim()).toBe('Worked through: 2 folders')
    expect(toggle?.getAttribute('aria-expanded')).toBe('false')
    expect(element.querySelectorAll('.tool-line')).toHaveLength(0)

    toggle?.click()
    await tick()
    expect(toggle?.getAttribute('aria-expanded')).toBe('true')
    expect(element.querySelectorAll('.tool-line')).toHaveLength(2)
  })

  it('can’t be toggled while its first round has no row yet', () => {
    const element = target()
    const lines = [line('a', { running: true }, null), line('b', { running: true }, null)]
    mount(AskCmdrToolGroup, { target: element, props: { lines, live: true, revealPath } })
    flushSync()
    const toggle = element.querySelector<HTMLButtonElement>('.group-toggle')
    expect(toggle?.disabled).toBe(true)
    expect(toggle?.getAttribute('aria-expanded')).toBe('true')
  })

  it('moves focus to its toggle before a collapse removes the focused line', async () => {
    const element = target()
    mount(AskCmdrToolGroup, { target: element, props: { lines: [line('a'), line('b')], live: true, revealPath } })
    flushSync()
    const link = element.querySelector<HTMLButtonElement>('.group-body button.link-button')
    link?.focus()
    expect(document.activeElement).toBe(link)

    setGroupOpen('5:a', false)
    await tick()
    expect(element.querySelector('.group-body')).toBeNull()
    expect(document.activeElement).toBe(element.querySelector('.group-toggle'))
  })
})

describe('the expand on a tool line', () => {
  function expandOf(displayLine: DisplayLine): HTMLButtonElement {
    const element = target()
    mount(AskCmdrToolLine, { target: element, props: { line: displayLine, revealPath } })
    flushSync()
    const button = element.querySelector<HTMLButtonElement>('button.expand')
    if (!button) throw new Error('no expand')
    return button
  }

  it.each([
    ['a running line', line('a', { running: true, outcome: null })],
    ['a line whose round has no row yet', line('a', {}, null)],
    ['a line settled with no answer recorded', line('a', { outcome: null })],
  ])('is disabled on %s', (_, displayLine) => {
    expect(expandOf(displayLine).disabled).toBe(true)
  })

  it('fetches and shows the preview of a stored result', async () => {
    askCmdrState.conversationId = 3
    const detail: ToolResultDetailView = {
      rawJson: '{}',
      preview: { rows: [], rowsGivenToModel: null, total: null, lowerBound: false, problem: null, setAside: false },
    }
    askCmdrToolResult.mockResolvedValue({ found: true, detail })
    const button = expandOf(line('a'))
    button.click()
    await vi.waitFor(() => {
      expect(document.querySelector('.preview')?.textContent).toContain('Copy has the whole result')
    })
    expect(button.getAttribute('aria-expanded')).toBe('true')
    expect(askCmdrToolResult).toHaveBeenCalledWith(3, 5, 'a', 0)
  })

  it('fetches its own result when its round names the same call id twice', () => {
    askCmdrState.conversationId = 3
    askCmdrToolResult.mockReturnValue(new Promise(() => {}))
    expandOf({ ...line('a'), key: '5:a#1', occurrence: 1 }).click()
    expect(askCmdrToolResult).toHaveBeenCalledWith(3, 5, 'a', 1)
  })
})

describe('AskCmdrToolPreview', () => {
  function render(detail: ToolDetailState | undefined): HTMLElement {
    const element = target()
    mount(AskCmdrToolPreview, { target: element, props: { id: 'p', detail, revealPath } })
    flushSync()
    return element
  }

  const ready = (preview: Partial<ToolResultDetailView['preview']>): ToolDetailState => ({
    status: 'ready',
    detail: {
      rawJson: '{\n  "total": 40\n}',
      preview: {
        rows: [],
        rowsGivenToModel: null,
        total: null,
        lowerBound: false,
        problem: null,
        setAside: false,
        ...preview,
      },
    },
  })

  const row = (name: string) => ({
    name,
    path: `/Users/me/${name}`,
    kind: 'file' as const,
    sizeBytes: null,
    line: null,
    snippet: null,
    status: null,
  })

  it('lists the rows, each path a link, and says how much the model got', async () => {
    const element = render(ready({ rows: [row('a.txt'), row('b.txt')], rowsGivenToModel: 3, total: 40 }))
    expect(element.textContent).toContain('Showed the model 3 of 40')
    expect(element.textContent).toContain('Here are the first 2. Copy has the whole result.')
    element.querySelector<HTMLButtonElement>('button.link-button')?.click()
    await tick()
    expect(revealPath).toHaveBeenCalledWith({ path: '/Users/me/a.txt', kind: 'file' })
    await vi.waitFor(() => {
      expect(returnFocusToPane).toHaveBeenCalledTimes(1)
    })
  })

  it('says "so far" for a lower bound', () => {
    const element = render(ready({ rows: [], rowsGivenToModel: 20, total: 31, lowerBound: true }))
    expect(element.textContent).toContain('Showed the model 20 of 31 found so far')
  })

  it('shows a refusal’s sentence verbatim, labeled, and a set-aside note', () => {
    expect(render(ready({ problem: 'list_dir needs path.' })).textContent).toContain(
      'What the model was told: list_dir needs path.',
    )
    expect(render(ready({ setAside: true })).textContent).toContain('set aside to make room')
  })

  it('says so calmly when the result is gone, and spins while it loads', () => {
    expect(render({ status: 'notFound' }).textContent).toContain('This result isn’t here anymore.')
    expect(render(undefined).querySelector('[role="status"]')?.getAttribute('aria-label')).toBe(
      'Getting what came back',
    )
  })

  it('copies the raw JSON, and says so when the clipboard refuses', async () => {
    copyToClipboard.mockResolvedValueOnce(undefined)
    const element = render(ready({}))
    const copy = element.querySelector<HTMLButtonElement>('.copy button')
    copy?.click()
    await vi.waitFor(() => {
      expect(copy?.textContent.trim()).toBe('Copied')
    })
    expect(copyToClipboard).toHaveBeenCalledWith('{\n  "total": 40\n}')

    copyToClipboard.mockRejectedValueOnce(new Error('no clipboard'))
    copy?.click()
    await vi.waitFor(() => {
      expect(element.textContent).toContain('The clipboard didn’t take it.')
    })
  })
})

describe('the bubble’s group announcement', () => {
  function bubble(running: boolean, streaming: boolean): AssistantItem {
    return {
      kind: 'assistant',
      id: 5,
      turnId: 4,
      rounds: [{ messageId: 5, text: '', calls: [call('a', { running }), call('b', { running })] }],
      thinking: false,
      stalled: false,
      streaming,
      stats: null,
    }
  }

  function statusOf(element: HTMLElement): string {
    return element.querySelector('.sr-only[role="status"]')?.textContent ?? ''
  }

  it('says a live group’s summary once it settles, and never for a reloaded one', async () => {
    const message = $state(bubble(true, true))
    const element = target()
    mount(AskCmdrMessage, { target: element, props: { message, revealPath } })
    flushSync()
    expect(statusOf(element)).toBe('')

    for (const settled of message.rounds[0].calls) settled.running = false
    await tick()
    expect(statusOf(element)).toBe('Worked through: 2 folders')

    const reloaded = target()
    mount(AskCmdrMessage, { target: reloaded, props: { message: bubble(false, false), revealPath } })
    flushSync()
    expect(statusOf(reloaded)).toBe('')
  })
})
