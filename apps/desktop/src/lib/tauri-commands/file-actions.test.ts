/**
 * The text editor wrappers: `openInEditor` carries the backend's typed refusal across
 * the throw, so the toast can word it, and `listTextEditors` hands the timed answer
 * over as it came, so the settings row can tell "timed out" from "nothing listed".
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    openInEditor: vi.fn(),
    listTextEditors: vi.fn(),
  },
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }))

import { commands } from '$lib/ipc/bindings'
import { asOpenInEditorError, listTextEditors, openInEditor, OpenInEditorFailure } from './file-actions'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('openInEditor', () => {
  it('sends the file, the stored choice, and whether to ask about other editors, and answers the report', async () => {
    const report = { outcome: 'opened', openedInName: 'Sublime Text', otherEditorsInstalled: true } as const
    vi.mocked(commands.openInEditor).mockResolvedValueOnce({ status: 'ok', data: report })

    expect(await openInEditor('/Users/dave/notes.txt', 'com.sublimetext.4', true)).toEqual(report)
    expect(commands.openInEditor).toHaveBeenCalledExactlyOnceWith('/Users/dave/notes.txt', 'com.sublimetext.4', true)
  })

  it('throws the typed refusal, and asOpenInEditorError reads it back', async () => {
    vi.mocked(commands.openInEditor).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'launchRefused', errno: 2 },
    })

    const caught: unknown = await openInEditor('/Users/dave/notes.txt', 'system', false).catch((e: unknown) => e)

    expect(caught).toBeInstanceOf(OpenInEditorFailure)
    expect(asOpenInEditorError(caught)).toEqual({ type: 'launchRefused', errno: 2 })
  })

  it('reads anything that is not its refusal as no refusal at all', () => {
    expect(asOpenInEditorError(new Error('IPC down'))).toBeNull()
  })
})

describe('listTextEditors', () => {
  it('passes the stored choice down and the timed answer straight back', async () => {
    const answer = {
      data: { defaultAppName: 'TextEdit', defaultAppIcon: null, apps: [], chosenId: 'system' },
      timedOut: false,
    }
    vi.mocked(commands.listTextEditors).mockResolvedValueOnce(answer)

    expect(await listTextEditors('system')).toEqual(answer)
    expect(commands.listTextEditors).toHaveBeenCalledExactlyOnceWith('system')
  })
})
