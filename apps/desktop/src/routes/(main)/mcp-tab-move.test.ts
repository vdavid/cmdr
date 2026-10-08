/**
 * MCP `tab move`'s reply: it names what the move did with a typed `outcome`, and a move
 * that went through replies only after the new tab lists reached the backend, so the
 * agent's next `cmdr://state` read shows the tab where the reply says it is.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import type { MoveTabResult } from '$lib/file-explorer/tabs/tab-state-manager.svelte'
import type { ExplorerAPI } from './explorer-api'
import { moveTabForMcp } from './mcp-tab-move'
import { tabHandlers } from './command-handlers/tab-handlers'

function fakeExplorer(result: MoveTabResult) {
  const calls: string[] = []
  const explorer = {
    moveTab: vi.fn((): MoveTabResult => {
      calls.push('move')
      return result
    }),
    syncTabsToMcp: vi.fn(() => {
      calls.push('sync')
      return Promise.resolve()
    }),
  }
  vi.mocked(emit).mockImplementation(() => {
    calls.push('reply')
    return Promise.resolve()
  })
  return { explorer: explorer as unknown as ExplorerAPI & typeof explorer, calls }
}

const args = { pane: 'left', action: 'move', tabId: 't1', toPane: 'right', toIndex: 1, mcpRequestId: 'req-1' } as const

beforeEach(() => {
  vi.mocked(emit).mockReset()
})

describe('moveTabForMcp', () => {
  it('moves the tab it was asked to, where it was asked to', async () => {
    const { explorer } = fakeExplorer({ moved: true, toIndex: 1, wasActive: false })

    await moveTabForMcp({ explorer, args })

    expect(explorer.moveTab).toHaveBeenCalledExactlyOnceWith({
      fromPane: 'left',
      tabId: 't1',
      toPane: 'right',
      toIndex: 1,
    })
  })

  it('replies `moved` with the index the tab landed on, after the tab lists reached the backend', async () => {
    const { explorer, calls } = fakeExplorer({ moved: true, toIndex: 3, wasActive: true })

    await moveTabForMcp({ explorer, args })

    expect(calls).toEqual(['move', 'sync', 'reply'])
    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-1',
      ok: true,
      outcome: 'moved',
      toIndex: 3,
    })
  })

  it('replies `unchanged` for a tab that is already there, with nothing to sync', async () => {
    const { explorer } = fakeExplorer({ moved: false, reason: 'unchanged' })

    await moveTabForMcp({ explorer, args })

    expect(explorer.syncTabsToMcp).not.toHaveBeenCalled()
    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', { requestId: 'req-1', ok: true, outcome: 'unchanged' })
  })

  it.each(['pinned', 'onlyTab', 'targetFull', 'notFound'] as const)(
    'replies the `%s` refusal as a typed outcome, not as a sentence to parse',
    async (reason) => {
      const { explorer } = fakeExplorer({ moved: false, reason })

      await moveTabForMcp({ explorer, args })

      expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', { requestId: 'req-1', ok: false, outcome: reason })
    },
  )

  it('says so when no explorer is mounted, sparing the backend its full wait', async () => {
    fakeExplorer({ moved: false, reason: 'unchanged' })

    await moveTabForMcp({ explorer: undefined, args })

    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-1',
      ok: false,
      error: 'Explorer is not ready',
    })
  })

  it('is where the `tab.mcpAction` command sends a move, and nothing else', async () => {
    const { explorer } = fakeExplorer({ moved: true, toIndex: 1, wasActive: false })
    const handleMcpTabAction = vi.fn()
    const explorerRef = Object.assign(explorer, { handleMcpTabAction })
    const run = tabHandlers['tab.mcpAction'] as (ctx: { explorerRef: ExplorerAPI; dispatchArgs: unknown }) => unknown

    await run({ explorerRef, dispatchArgs: args })
    expect(explorer.moveTab).toHaveBeenCalledOnce()
    expect(handleMcpTabAction).not.toHaveBeenCalled()

    await run({ explorerRef, dispatchArgs: { pane: 'left', action: 'close', tabId: 't1' } })
    expect(handleMcpTabAction).toHaveBeenCalledExactlyOnceWith('left', 'close', 't1', undefined)
    expect(explorer.moveTab).toHaveBeenCalledOnce()
  })

  it('still moves the tab for a caller that sent no request id, and replies to nobody', async () => {
    const { explorer } = fakeExplorer({ moved: true, toIndex: 0, wasActive: false })

    await moveTabForMcp({ explorer, args: { ...args, mcpRequestId: undefined } })

    expect(explorer.moveTab).toHaveBeenCalledOnce()
    expect(emit).not.toHaveBeenCalled()
  })
})
