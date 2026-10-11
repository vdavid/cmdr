/**
 * MCP `dialog confirm`'s reply for a BACKGROUND confirm: the open dialog's
 * Background button is pressed, and the agent hears whether it was. A permanent
 * delete refuses, typed (`refusal: 'permanentDelete'`), so the backend can say so
 * without reading a sentence. A plain confirm carries no request id and replies
 * nothing: the backend acks it on the dialog going away.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import type { ExplorerAPI } from './explorer-api'
import { confirmDialogForMcp } from './mcp-dialog-confirm'

function explorerAnswering(verdict: ReturnType<ExplorerAPI['confirmDialog']>) {
  return { confirmDialog: vi.fn(() => verdict) } as unknown as ExplorerAPI & {
    confirmDialog: ReturnType<typeof vi.fn>
  }
}

beforeEach(() => {
  vi.mocked(emit).mockClear()
  vi.mocked(emit).mockResolvedValue(undefined)
})

describe('a background confirm over MCP', () => {
  it('presses the Background button and answers that it did', async () => {
    const explorer = explorerAnswering({ pressed: true })

    await confirmDialogForMcp({
      explorer,
      type: 'transfer-confirmation',
      onConflict: 'skip_all',
      startInBackground: true,
      requestId: 'req-1',
    })

    expect(explorer.confirmDialog).toHaveBeenCalledWith('transfer-confirmation', 'skip_all', {
      startInBackground: true,
    })
    expect(emit).toHaveBeenCalledWith('mcp-response', { requestId: 'req-1', ok: true })
  })

  it('carries the permanent-delete refusal as a typed field', async () => {
    const explorer = explorerAnswering({ pressed: false, refusal: 'permanentDelete' })

    await confirmDialogForMcp({ explorer, type: 'delete-confirmation', startInBackground: true, requestId: 'req-2' })

    expect(emit).toHaveBeenCalledWith('mcp-response', { requestId: 'req-2', ok: false, refusal: 'permanentDelete' })
  })

  it('fails the round-trip when no dialog was there to press', async () => {
    const explorer = explorerAnswering({ pressed: false, refusal: 'notReady' })

    await confirmDialogForMcp({ explorer, type: 'delete-confirmation', startInBackground: true, requestId: 'req-3' })

    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-3',
      ok: false,
      error: expect.any(String) as unknown,
    })
  })

  it('fails the round-trip when no explorer is mounted', async () => {
    await confirmDialogForMcp({
      explorer: undefined,
      type: 'transfer-confirmation',
      startInBackground: true,
      requestId: 'req-4',
    })

    expect(emit).toHaveBeenCalledWith('mcp-response', expect.objectContaining({ requestId: 'req-4', ok: false }))
  })

  it('replies nothing for a plain confirm, which carries no request id', async () => {
    const explorer = explorerAnswering({ pressed: true })

    await confirmDialogForMcp({ explorer, type: 'delete-confirmation', requestId: undefined })

    expect(explorer.confirmDialog).toHaveBeenCalledWith('delete-confirmation', undefined, {
      startInBackground: undefined,
    })
    expect(emit).not.toHaveBeenCalled()
  })
})
