/**
 * The entry-point gate: whether a command that would start a file operation runs
 * at all while a dialog is on screen.
 *
 * ⚠️ It reads the open-dialog set, which `ModalDialog` maintains, so it sees every
 * dialog in the window rather than a list someone has to keep up to date. What's
 * worth pinning here is which dialogs it lets through, and that the refusal reaches
 * both audiences.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import {
  answeringDialogOpen,
  mcpOperationBlockedMessage,
  operationStartIsBlocked,
  refuseOpening,
} from './operation-start-gate'
import { markDialogOpen, _resetOpenDialogsForTesting } from '#lib/ui/open-dialogs.svelte.js'
import { addToast } from '#lib/ui/toast/index.js'

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn() }))

beforeEach(() => {
  vi.clearAllMocks()
  _resetOpenDialogsForTesting()
})

describe('starting an operation with nothing on screen', () => {
  it('goes ahead, silently', () => {
    expect(operationStartIsBlocked()).toBe(false)
    expect(addToast).not.toHaveBeenCalled()
  })
})

describe('starting an operation with a dialog up', () => {
  it('refuses while search is open', () => {
    // David's call: search is a dialog for this purpose. It covers the panes, so a
    // copy confirmation would stack over what the user is reading.
    markDialogOpen('search')

    expect(operationStartIsBlocked()).toBe(true)
    expect(addToast).toHaveBeenCalledTimes(1)
  })

  it('refuses while an operation is running', () => {
    markDialogOpen('transfer-progress')

    expect(operationStartIsBlocked()).toBe(true)
  })

  it('fails the MCP round-trip with the dialog as a typed field', () => {
    markDialogOpen('transfer-progress')

    operationStartIsBlocked('req-3')

    expect(emit).toHaveBeenCalledWith(
      'mcp-response',
      expect.objectContaining({ requestId: 'req-3', ok: false, blockedBy: 'transfer-progress' }),
    )
  })

  it('says nothing on the round-trip when a person asked', () => {
    markDialogOpen('transfer-progress')

    operationStartIsBlocked()

    expect(emit).not.toHaveBeenCalledWith('mcp-response', expect.anything())
  })

  it('lets a viewer sheet through: another window, another decision', () => {
    markDialogOpen('viewer-copy-confirm')

    expect(operationStartIsBlocked()).toBe(false)
    expect(addToast).not.toHaveBeenCalled()
  })
})

describe('what the agent is told', () => {
  it('names the dialog and the way out', () => {
    const message = mcpOperationBlockedMessage('search')

    expect(message).toContain('search')
    expect(message).toContain('Close it first')
  })

  it('never uses the words this app refuses to put in front of people', () => {
    // `docs/style-guide.md`: an operation that didn't happen isn't an "error" or a
    // "failure", and the agent's transcript is read by humans too.
    const message = mcpOperationBlockedMessage('transfer-progress').toLowerCase()

    expect(message).not.toContain('error')
    expect(message).not.toContain('failed')
  })
})

describe('answering an MCP tool that opened a dialog', () => {
  it('says ok once the dialog is showing', async () => {
    await answeringDialogOpen('req-1', () => true)

    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', { requestId: 'req-1', ok: true })
  })

  it('fails typed when nothing opened, so the tool never waits out its budget', async () => {
    await answeringDialogOpen('req-2', () => Promise.resolve(false))

    expect(emit).toHaveBeenCalledExactlyOnceWith(
      'mcp-response',
      expect.objectContaining({ requestId: 'req-2', ok: false, refusal: 'nothingOpened' }),
    )
  })

  it('fails with the message when opening threw', async () => {
    await answeringDialogOpen('req-3', () => {
      throw new Error('listing gone')
    })

    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-3',
      ok: false,
      error: 'listing gone',
    })
  })

  it('answers nothing when a person asked', async () => {
    const open = vi.fn(() => true)

    await answeringDialogOpen(undefined, open)

    expect(open).toHaveBeenCalledTimes(1)
    expect(emit).not.toHaveBeenCalled()
  })
})

describe('refusing to open a dialog', () => {
  it('tells a person, and only a person, through the dialog', () => {
    const tellPerson = vi.fn()

    refuseOpening(undefined, { kind: 'readOnly', message: 'Read-only' }, tellPerson)

    expect(tellPerson).toHaveBeenCalledTimes(1)
    expect(emit).not.toHaveBeenCalled()
  })

  it('tells an MCP caller typed, and puts nothing up over the person', () => {
    const tellPerson = vi.fn()

    refuseOpening('req-4', { kind: 'destination', message: 'Not there' }, tellPerson)

    expect(tellPerson).not.toHaveBeenCalled()
    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-4',
      ok: false,
      refusal: 'destination',
      error: 'Not there',
    })
  })
})
