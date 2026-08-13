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
import { operationStartIsBlocked, mcpOperationBlockedMessage } from './operation-start-gate'
import { markDialogOpen, _resetOpenDialogsForTesting } from '$lib/ui/open-dialogs.svelte'
import { addToast } from '$lib/ui/toast'

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))

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
