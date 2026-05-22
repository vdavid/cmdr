/**
 * IPC contract tests for the paths-by-value clipboard commands added in M8d.
 *
 * Pins the wire shape (`{ paths }`) for `copy_paths_to_clipboard` and
 * `cut_paths_to_clipboard` so a rename on the Rust side won't silently break
 * the search-results pane's source-side copy/cut flow.
 *
 * See `apps/desktop/src/lib/ipc/CLAUDE.md` § "IPC contract testing" for the
 * rules around when to write these.
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

describe('commands.copyPathsToClipboard', () => {
  it('forwards the paths array verbatim and returns the count', async () => {
    const ipc = installIpcMock()
    ipc.mock('copy_paths_to_clipboard', () => 3)

    const out = await commands.copyPathsToClipboard(['/a/x', '/a/y', '/a/z'])

    expect(out).toEqual({ status: 'ok', data: 3 })
    expect(ipc.lastCall('copy_paths_to_clipboard')?.payload).toEqual({
      paths: ['/a/x', '/a/y', '/a/z'],
    })
  })

  it('surfaces a string error when the backend rejects an empty list', async () => {
    const ipc = installIpcMock()
    ipc.mock('copy_paths_to_clipboard', () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- mockIPC requires throwing the raw wire shape to test the wire contract
      throw 'No files to copy'
    })

    const out = await commands.copyPathsToClipboard([])

    expect(out.status).toBe('error')
    if (out.status === 'error') expect(out.error).toBe('No files to copy')
  })
})

describe('commands.cutPathsToClipboard', () => {
  it('forwards the paths array verbatim and returns the count', async () => {
    const ipc = installIpcMock()
    ipc.mock('cut_paths_to_clipboard', () => 2)

    const out = await commands.cutPathsToClipboard(['/a/x', '/a/y'])

    expect(out).toEqual({ status: 'ok', data: 2 })
    expect(ipc.lastCall('cut_paths_to_clipboard')?.payload).toEqual({
      paths: ['/a/x', '/a/y'],
    })
  })
})
