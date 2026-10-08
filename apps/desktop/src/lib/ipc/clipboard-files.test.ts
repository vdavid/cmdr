/**
 * IPC contract test for `paste_clipboard_as_file` (paste clipboard content as a
 * file). Pins the wire shape so a Rust-side rename or serde change can't silently
 * break the paste-as-file flow:
 *  - args serialize as `{ volumeId, directory }` (volumeId nullable → JSON null),
 *  - an in-time answer is `{ type: 'done', file }`, the created file as
 *    `{ name, kind }` with the camelCase `kind` discriminator (`text` | `image` |
 *    `pdf`), and "nothing pasteable" as `file: null` (NOT an error), which the FE
 *    treats as the no-op warn path,
 *  - a slow write is `{ type: 'stillRunning', pendingId }` (camelCase field), its
 *    end following on `clipboard-paste-settled`.
 *
 * See `apps/desktop/src/lib/ipc/CLAUDE.md` § "IPC contract testing".
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

describe('commands.pasteClipboardAsFile', () => {
  it('forwards { volumeId, directory } and returns the created file with its kind', async () => {
    const ipc = installIpcMock()
    ipc.mock('paste_clipboard_as_file', () => ({ type: 'done', file: { name: 'pasted (2).png', kind: 'image' } }))

    const out = await commands.pasteClipboardAsFile('root', '/dest/dir')

    expect(out).toEqual({ status: 'ok', data: { type: 'done', file: { name: 'pasted (2).png', kind: 'image' } } })
    expect(ipc.lastCall('paste_clipboard_as_file')?.payload).toEqual({ volumeId: 'root', directory: '/dest/dir' })
  })

  it('passes a null volumeId through (backend defaults it to root)', async () => {
    const ipc = installIpcMock()
    ipc.mock('paste_clipboard_as_file', () => ({ type: 'done', file: null }))

    const out = await commands.pasteClipboardAsFile(null, '/dest')

    // `file: null` is the typed "nothing pasteable" no-op — an ok result, not an error.
    expect(out).toEqual({ status: 'ok', data: { type: 'done', file: null } })
    expect(ipc.lastCall('paste_clipboard_as_file')?.payload).toEqual({ volumeId: null, directory: '/dest' })
  })

  it('round-trips each kind discriminator verbatim (text | image | pdf)', async () => {
    for (const kind of ['text', 'image', 'pdf'] as const) {
      const ipc = installIpcMock()
      ipc.mock('paste_clipboard_as_file', () => ({ type: 'done', file: { name: `pasted.${kind}`, kind } }))

      const out = await commands.pasteClipboardAsFile('root', '/d')

      expect(out).toEqual({ status: 'ok', data: { type: 'done', file: { name: `pasted.${kind}`, kind } } })
      clearIpcMocks()
    }
  })

  it('carries a slow write as stillRunning with its pendingId', async () => {
    const ipc = installIpcMock()
    ipc.mock('paste_clipboard_as_file', () => ({ type: 'stillRunning', pendingId: 'p-1' }))

    const out = await commands.pasteClipboardAsFile('root', '/dest')

    expect(out).toEqual({ status: 'ok', data: { type: 'stillRunning', pendingId: 'p-1' } })
  })

  it('surfaces a real write failure as a typed error result (not a no-op)', async () => {
    const ipc = installIpcMock()
    ipc.mock('paste_clipboard_as_file', () => {
      throw { type: 'parentNotWritable', path: '/dest' }
    })

    const out = await commands.pasteClipboardAsFile('root', '/dest')

    expect(out.status).toBe('error')
    if (out.status === 'error') expect(out.error).toEqual({ type: 'parentNotWritable', path: '/dest' })
  })
})
