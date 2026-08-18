/**
 * What a save says when the backend turns the name down.
 *
 * The chained rename composes these into a sentence about the file that kept
 * its name, so a message built in the module's own English would land halfway
 * through a translated sentence. They come from the catalog, and from the SAME
 * keys the editor's live validation uses, so the two never drift apart.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { checkRenameValiditySpy, renameFileSpy } = vi.hoisted(() => ({
  checkRenameValiditySpy: vi.fn(),
  renameFileSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  checkRenameValidity: checkRenameValiditySpy,
  checkRenamePermission: vi.fn(),
  renameFile: renameFileSpy,
  getIpcErrorMessage: (e: unknown) => String(e),
  isIpcError: () => false,
}))

import { tString } from '$lib/intl/messages.svelte'
import { executeRenameSave } from './rename-operations'
import type { RenameTarget } from './rename-state.svelte'

const FILE: RenameTarget = {
  path: '/dir/notes.txt',
  originalName: 'notes.txt',
  parentPath: '/dir',
  isDirectory: false,
}
const FOLDER: RenameTarget = { ...FILE, path: '/dir/notes', originalName: 'notes', isDirectory: true }

/** Runs a save the backend refuses with `error`, and hands back what it said. */
async function messageFor(error: unknown, target: RenameTarget = FILE): Promise<string> {
  checkRenameValiditySpy.mockResolvedValue({ valid: false, error, hasConflict: false, isCaseOnlyRename: false })
  const result = await executeRenameSave(target, 'whatever.txt', 'yes')
  expect(result.type).toBe('error')
  return result.type === 'error' ? result.message : ''
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('a name the backend turns down', () => {
  it('reads exactly like the message the editor shows while typing', async () => {
    const message = await messageFor({ kind: 'nameTooLong', bytes: 300, max: 255 })

    expect(message).toBe(
      tString('fileOperations.validation.nameTooLong', { kind: 'file', byteCount: '300', maxBytes: '255' }),
    )
  })

  it('calls a folder a folder', async () => {
    const message = await messageFor({ kind: 'empty' }, FOLDER)

    expect(message).toBe(tString('fileOperations.validation.empty', { kind: 'folder' }))
    expect(message).toContain('older')
  })

  it('says something usable even when the backend names no rule', async () => {
    const message = await messageFor(null)

    expect(message).toBe(tString('fileOperations.validation.nameNotUsable', { kind: 'file' }))
  })

  it('never ships an unfilled placeholder, whichever rule the name broke', async () => {
    const errors = [
      { kind: 'empty' },
      { kind: 'disallowedCharacter', character: '/' },
      { kind: 'nameTooLong', bytes: 300, max: 255 },
      { kind: 'pathTooLong', bytes: 2000, max: 1024 },
      null,
    ]

    for (const error of errors) {
      const message = await messageFor(error)
      expect(message).not.toContain('{')
      expect(message.length).toBeGreaterThan(0)
    }
  })
})
