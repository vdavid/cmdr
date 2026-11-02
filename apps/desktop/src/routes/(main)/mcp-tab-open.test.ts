import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({ emit: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ emit: mocks.emit }))

import { openTabForMcp } from './mcp-tab-open'
import type { ExplorerAPI } from './explorer-api'

const args = { pane: 'right', action: 'new', mcpRequestId: 'r1' } as const

function explorer(opened: boolean): ExplorerAPI {
  return { openTabInPane: vi.fn(() => opened) } as unknown as ExplorerAPI
}

describe('openTabForMcp', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.emit.mockResolvedValue(undefined)
  })

  it('opens the tab in the pane it was asked about and says so', async () => {
    const api = explorer(true)
    await openTabForMcp({ explorer: api, args })
    expect(api.openTabInPane).toHaveBeenCalledWith('right')
    expect(mocks.emit).toHaveBeenCalledWith('mcp-response', { requestId: 'r1', ok: true, outcome: 'opened' })
  })

  it('names a pane at its limit, which is an answer and not a failure', async () => {
    await openTabForMcp({ explorer: explorer(false), args })
    expect(mocks.emit).toHaveBeenCalledWith('mcp-response', { requestId: 'r1', ok: true, outcome: 'atLimit' })
  })

  it('declines when no explorer is mounted, with no outcome to misread', async () => {
    await openTabForMcp({ explorer: undefined, args })
    expect(mocks.emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'r1',
      ok: false,
      error: 'Explorer is not ready',
    })
  })
})
