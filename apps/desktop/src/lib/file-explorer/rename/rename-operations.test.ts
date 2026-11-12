/**
 * What a save says when the backend turns the name down.
 *
 * The chained rename composes these into a sentence about the file that kept
 * its name, so a message built in the module's own English would land halfway
 * through a translated sentence. They come from the catalog, and from the SAME
 * keys the editor's live validation uses, so the two never drift apart.
 */

import { describe, it, expect, vi, beforeEach, onTestFinished } from 'vitest'
import { withCapabilities } from '#lib/platform/test-platform.js'

const { checkRenameValiditySpy, renameFileSpy } = vi.hoisted(() => ({
  checkRenameValiditySpy: vi.fn(),
  renameFileSpy: vi.fn(),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  checkRenameValidity: checkRenameValiditySpy,
  checkRenamePermission: vi.fn(),
  renameFile: renameFileSpy,
}))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { tString } from '#lib/intl/messages.svelte.js'
import { MutationFailure } from '#lib/file-operations/mutation-error.js'
import { renderMutationError } from '#lib/file-operations/mutation-error-messages.js'
import { executeRenameSave, performRename } from './rename-operations'
import type { RenameTarget } from './rename-state.svelte'

const FILE: RenameTarget = {
  path: '/dir/notes.txt',
  originalName: 'notes.txt',
  parentPath: '/dir',
  isDirectory: false,
  nameKind: 'file',
  initiator: 'user',
}
const FOLDER: RenameTarget = {
  ...FILE,
  path: '/dir/notes',
  originalName: 'notes',
  isDirectory: true,
  nameKind: 'folder',
  initiator: 'user',
}

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
      tString('fileOperations.validation.nameTooLong', { kind: 'file', byteCount: '300', maxBytes: '255', max: 255 }),
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

describe("a validity check that can't run at all", () => {
  /** Runs a save whose validity check rejects with `thrown`, and hands back what it said. */
  async function messageForThrow(thrown: unknown): Promise<string> {
    checkRenameValiditySpy.mockRejectedValue(thrown)
    const result = await executeRenameSave(FILE, 'whatever.txt', 'yes')
    expect(result.type).toBe('error')
    return result.type === 'error' ? result.message : ''
  }

  it("speaks the user's language, from the same typed vocabulary the rename itself uses", async () => {
    const message = await messageForThrow(new MutationFailure({ type: 'volumeGone', volumeId: 'smb-testshare' }))

    expect(message).toBe(renderMutationError({ type: 'volumeGone', volumeId: 'smb-testshare' }, 'file'))
  })

  it('keeps a deadline distinguishable from a refusal, without reading either sentence', async () => {
    const timedOut = await messageForThrow(new MutationFailure({ type: 'timedOut' }))
    const gone = await messageForThrow(new MutationFailure({ type: 'volumeGone', volumeId: 'x' }))

    expect(timedOut).not.toBe(gone)
  })

  it("never puts the transport's own English in front of a person", async () => {
    const message = await messageForThrow(new Error('IPC channel closed unexpectedly'))

    expect(message).not.toContain('IPC channel')
    expect(message).toBe(renderMutationError({ type: 'unexpected', detail: '' }, 'file'))
  })
})

describe('a rename that copies', () => {
  const validity = (byMove: unknown) => ({
    valid: true,
    error: null,
    hasConflict: false,
    isCaseOnlyRename: false,
    conflict: null,
    byMove,
  })

  it('asks for the Move dialog when the backend wants an OK first, and renames nothing yet', async () => {
    checkRenameValiditySpy.mockResolvedValue(validity({ files: 5000, bytes: 1, countedAll: false, confirmFirst: true }))

    const result = await executeRenameSave(FOLDER, 'renamed', 'yes', false, 's3-1')

    expect(result).toEqual({ type: 'confirm-move', newName: 'renamed' })
    expect(renameFileSpy).not.toHaveBeenCalled()
  })

  it('renames right away when the copy is small enough to run in the background', async () => {
    checkRenameValiditySpy.mockResolvedValue(validity({ files: 3, bytes: 10, countedAll: true, confirmFirst: false }))
    renameFileSpy.mockResolvedValue(undefined)

    const result = await executeRenameSave(FOLDER, 'renamed', 'yes', false, 's3-1')

    expect(result).toEqual({ type: 'success', newName: 'renamed' })
    expect(renameFileSpy).toHaveBeenCalledWith(
      '/dir/notes',
      '/dir/renamed',
      false,
      's3-1',
      'user',
      expect.objectContaining({ onStillRunning: expect.any(Function) as unknown }),
    )
  })
})

describe('the extension question on save', () => {
  beforeEach(() => {
    checkRenameValiditySpy.mockResolvedValue({ valid: true, error: null, hasConflict: false, isCaseOnlyRename: false })
    renameFileSpy.mockResolvedValue(undefined)
  })

  it('asks before a file changes its extension', async () => {
    const result = await executeRenameSave(FILE, 'notes.json', 'ask')

    expect(result).toEqual({ type: 'extension-ask', oldExtension: 'txt', newExtension: 'json' })
  })

  it("doesn't ask for a folder: the part after its dot is only a name", async () => {
    const folder: RenameTarget = { ...FOLDER, path: '/dir/release.v1.2', originalName: 'release.v1.2' }

    const result = await executeRenameSave(folder, 'release.v1.3', 'ask')

    expect(result.type).not.toBe('extension-ask')
  })

  it('still asks before a macOS package folder loses its extension', async () => {
    const app: RenameTarget = { ...FOLDER, path: '/dir/Cmdr.app', originalName: 'Cmdr.app', nameKind: 'package' }

    const result = await executeRenameSave(app, 'Cmdr.old', 'ask')

    // A package's extension is whatever follows its last dot, before and after.
    expect(result).toEqual({ type: 'extension-ask', oldExtension: 'app', newExtension: 'old' })
  })

  it('asks for a package only macOS knew about, read off the entry', async () => {
    const sketch: RenameTarget = {
      ...FOLDER,
      path: '/dir/Logo.sketchbundle',
      originalName: 'Logo.sketchbundle',
      nameKind: 'package',
      initiator: 'user',
    }

    const result = await executeRenameSave(sketch, 'Logo', 'ask')

    expect(result).toEqual({ type: 'extension-ask', oldExtension: 'sketchbundle', newExtension: '' })
  })

  it('asks before a plain folder turns into a listed package', async () => {
    onTestFinished(withCapabilities('appBundles'))
    const plugins: RenameTarget = { ...FOLDER, path: '/dir/Plugins', originalName: 'Plugins' }

    const result = await executeRenameSave(plugins, 'Plugins.bundle', 'ask')

    expect(result).toEqual({ type: 'extension-ask', oldExtension: '', newExtension: 'bundle' })
  })

  it('reads the extensions by the product rule, so a compound tar is one', async () => {
    const tarball: RenameTarget = { ...FILE, path: '/dir/src.tar.gz', originalName: 'src.tar.gz' }

    const result = await executeRenameSave(tarball, 'src.zip', 'ask')

    expect(result).toEqual({ type: 'extension-ask', oldExtension: 'tar.gz', newExtension: 'zip' })
  })
})

describe('a rename on a slow volume', () => {
  /** A `renameFile` that says it's still running, then waits until the test ends it. */
  function slowRename() {
    let land!: () => void
    let refuse!: (e: unknown) => void
    renameFileSpy.mockImplementationOnce(
      (
        _from: string,
        _to: string,
        _force: boolean,
        _volumeId?: string,
        _initiator?: unknown,
        wait?: { onStillRunning?: () => void },
      ) => {
        wait?.onStillRunning?.()
        return new Promise<void>((resolve, reject) => {
          land = resolve
          refuse = reject
        })
      },
    )
    return {
      land: () => {
        land()
      },
      refuse: (e: unknown) => {
        refuse(e)
      },
    }
  }

  it('hands back at the deadline as still renaming, never as a failure, and settles as a success', async () => {
    const volume = slowRename()

    const result = await performRename(FILE, 'renamed.txt', false, 'smb-testshare')

    expect(result.type).toBe('still-renaming')
    volume.land()
    if (result.type === 'still-renaming') {
      expect(await result.settled).toEqual({ type: 'success', newName: 'renamed.txt' })
    }
  })

  it("settles with the volume's own reason when the slow rename is refused", async () => {
    const volume = slowRename()

    const result = await performRename(FILE, 'renamed.txt', false, 'smb-testshare')
    volume.refuse(new MutationFailure({ type: 'parentNotWritable', path: '/dir' }))

    expect(result.type).toBe('still-renaming')
    if (result.type === 'still-renaming') {
      expect(await result.settled).toEqual({
        type: 'error',
        message: renderMutationError({ type: 'parentNotWritable', path: '/dir' }, 'file', 'renamed.txt'),
      })
    }
  })
})
