/**
 * "Copy share link": which row it acts on, and what each outcome says. The link
 * itself never comes back from Rust, so a toast is all the frontend has.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({ copyShareLink: vi.fn() }))
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPaneVolumeId: vi.fn(() => 's3-photos'),
}))
vi.mock('$lib/file-explorer/pane/volume-capabilities', () => ({
  rowCanShareLink: vi.fn((_volumeId: string, row: { isDirectory: boolean }) => !row.isDirectory),
}))

import { addToast } from '$lib/ui/toast'
import { copyShareLink } from '$lib/tauri-commands'
import { shareLinkHandlers } from './share-link-handlers'
import type { CommandHandlerContext } from './types'

const toast = vi.mocked(addToast)
const mint = vi.mocked(copyShareLink)

function onRow(row: { name: string; path: string; isDirectory: boolean } | null): CommandHandlerContext {
  const explorerRef = { getCursorRowForTerminal: () => Promise.resolve(row) }
  return { explorerRef, ctx: {}, dispatchArgs: undefined } as unknown as CommandHandlerContext
}

const FILE = { name: 'a.jpg', path: 's3://AKIA@host:443/photos/a.jpg', isDirectory: false }

beforeEach(() => {
  toast.mockClear()
  mint.mockReset()
})

describe('Copy share link', () => {
  it('mints a seven-day link for the file under the cursor and says how long it works', async () => {
    mint.mockResolvedValue({ ok: true })
    await shareLinkHandlers['file.copyShareLink'](onRow(FILE))

    expect(mint).toHaveBeenCalledWith('s3-photos', FILE.path, 'sevenDays')
    expect(toast).toHaveBeenCalledWith(
      'Copied a share link. It works for seven days.',
      expect.objectContaining({ level: 'success' }),
    )
  })

  it('passes the expiry each id names', async () => {
    mint.mockResolvedValue({ ok: true })
    await shareLinkHandlers['file.copyShareLinkOneDay'](onRow(FILE))
    await shareLinkHandlers['file.copyShareLinkOneHour'](onRow(FILE))

    expect(mint.mock.calls.map((call) => call[2])).toEqual(['oneDay', 'oneHour'])
  })

  it('mints nothing on a folder, and says what it needs', async () => {
    await shareLinkHandlers['file.copyShareLink'](
      onRow({ name: '2019', path: 's3://AKIA@host:443/photos/2019', isDirectory: true }),
    )

    expect(mint).not.toHaveBeenCalled()
    expect(toast).toHaveBeenCalledWith(
      'Move the cursor to a file in an S3 bucket to copy a share link.',
      expect.objectContaining({ level: 'warn' }),
    )
  })

  it('words a refusal from the typed error', async () => {
    mint.mockResolvedValue({ ok: false, error: { type: 'deviceDisconnected', data: 's3-photos' } })
    await shareLinkHandlers['file.copyShareLink'](onRow(FILE))

    const [text, options] = toast.mock.calls[0]
    expect(String(text)).toMatch(/^Couldn’t make a share link\. /)
    expect(options).toEqual(expect.objectContaining({ level: 'error' }))
  })
})
