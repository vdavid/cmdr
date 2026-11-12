/**
 * Edit and Rename on the Servers volume are "Edit server…": `file.edit`, `file.rename`, and `servers.edit` all hand
 * `editServerInView` what the pane shows (the hub row under the cursor, or the host whose share list is up), whatever
 * keys they're bound to, and keep their file-list meaning everywhere else.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const editServerInView = vi.fn((_view: unknown) => Promise.resolve())
const runServerRowAction = vi.fn((_payload: unknown) => Promise.resolve())
const openInEditorOrExplain = vi.fn(() => Promise.resolve('opened'))
let volumeId = 'root'

vi.mock('#lib/file-explorer/network/servers-hub-actions.js', () => ({
  editServerInView: (view: unknown) => editServerInView(view),
}))
vi.mock('#lib/file-explorer/navigation/server-row-actions.js', () => ({
  runServerRowAction: (payload: unknown) => runServerRowAction(payload),
}))
vi.mock('#lib/file-explorer/pane/editor-open.js', () => ({ openInEditorOrExplain: () => openInEditorOrExplain() }))
vi.mock('#lib/file-explorer/pane/focused-pane-reads.js', () => ({
  getFocusedPanePath: () => '/Users/me',
  getFocusedPaneVolumeId: () => volumeId,
}))
vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: () => [] }))
vi.mock('#lib/tauri-commands/index.js', () => ({ listSavedServers: () => Promise.resolve([]), trackEvent: vi.fn() }))

import { fileHandlers } from './file-handlers'
import { serversHandlers } from './servers-handlers'
import type { CommandHandlerContext } from './types'
import type { HubRow } from '#lib/file-explorer/network/servers-hub-rows.js'
import type { NetworkHost } from '#lib/file-explorer/types.js'

const hubRow = { id: 'manual-nas', kind: 'server', name: 'Testnas' } as unknown as HubRow
const host: NetworkHost = { id: 'h1', name: 'Testnas', port: 445, source: 'discovered' }

function hctx(row: HubRow | null, placesHost: NetworkHost | null = null) {
  const explorerRef = {
    getFocusedPaneHubRow: vi.fn(() => row),
    getFocusedPaneNetworkHost: vi.fn(() => placesHost),
    getFocusedPaneServerRow: vi.fn(() => null),
    startRename: vi.fn(),
    getFileAndPathUnderCursor: vi.fn(() => ({ path: '/Users/me/a.txt', filename: 'a.txt' })),
  }
  return { ctx: { explorerRef, ctx: {}, dispatchArgs: undefined } as unknown as CommandHandlerContext, explorerRef }
}

type Handler = (hctx: CommandHandlerContext) => unknown
const onServersVolume: [string, Handler][] = [
  ['file.rename', fileHandlers['file.rename']],
  ['file.edit', fileHandlers['file.edit']],
  ['servers.edit', serversHandlers['servers.edit']],
]

beforeEach(() => {
  vi.clearAllMocks()
  volumeId = 'root'
})

describe('on the Servers volume', () => {
  beforeEach(() => {
    volumeId = 'network'
  })

  it.each(onServersVolume)('%s edits the hub row under the cursor', (_id, handler) => {
    const { ctx, explorerRef } = hctx(hubRow)
    void handler(ctx)
    expect(editServerInView).toHaveBeenCalledExactlyOnceWith({ row: hubRow, host: null })
    expect(explorerRef.startRename).not.toHaveBeenCalled()
    expect(openInEditorOrExplain).not.toHaveBeenCalled()
  })

  it.each(onServersVolume)('%s edits the host whose share list is up', (_id, handler) => {
    const { ctx } = hctx(null, host)
    void handler(ctx)
    expect(editServerInView).toHaveBeenCalledExactlyOnceWith({ row: null, host })
  })

  /** "Add server…" under the cursor: `editServerInView` says to pick a server, ❌ never silence. */
  it('hands over an empty view too, and never falls back to renaming a file', () => {
    const { ctx, explorerRef } = hctx(null)
    const rename = fileHandlers['file.rename']
    rename(ctx)
    expect(editServerInView).toHaveBeenCalledExactlyOnceWith({ row: null, host: null })
    expect(explorerRef.startRename).not.toHaveBeenCalled()
  })
})

describe('anywhere else', () => {
  it('file.rename renames the file under the cursor', () => {
    const { ctx, explorerRef } = hctx(null)
    const rename = fileHandlers['file.rename']
    rename(ctx)
    expect(explorerRef.startRename).toHaveBeenCalledOnce()
    expect(editServerInView).not.toHaveBeenCalled()
  })

  it('file.edit opens the file under the cursor in the editor', async () => {
    const { ctx } = hctx(null)
    await (fileHandlers['file.edit'] as Handler)(ctx)
    expect(openInEditorOrExplain).toHaveBeenCalledOnce()
    expect(editServerInView).not.toHaveBeenCalled()
  })
})
