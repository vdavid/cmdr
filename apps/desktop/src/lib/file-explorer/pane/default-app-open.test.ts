/**
 * Enter's default-app open: a row with no file another app can open is refused on
 * screen before any launch, the backend's own refusal (a direct SMB share whose macOS
 * mount is gone) gets the same toast, and macOS saying no is no longer silent.
 *
 * The capability check is the real one, over an empty volume list, so the ids below
 * classify off their shape alone (as `editor-open.test.ts` does).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { OpenPathError } from '#lib/ipc/bindings.js'
import type { FileEntry } from '../types'

const m = vi.hoisted(() => ({
  openFile: vi.fn<(path: string, volumeId: string | null) => Promise<void>>(),
  announceOpening: vi.fn<() => Promise<void>>(),
  addToast: vi.fn(),
}))

/** Stands in for `OpenPathFailure`: a thrown value carrying the typed reason. */
class Refused extends Error {
  readonly failure: OpenPathError
  constructor(failure: OpenPathError) {
    super(failure.type)
    this.failure = failure
  }
}

vi.mock('#lib/tauri-commands/index.js', () => ({
  openFile: m.openFile,
  asOpenPathError: (error: unknown) => (error instanceof Refused ? error.failure : null),
}))
vi.mock('./opening-toast', () => ({ announceOpening: m.announceOpening, OPENING_TOAST_ID: 'opening-file' }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: m.addToast }))
// The key and its params, so a test reads which message and what it named.
vi.mock(import('#lib/intl/messages.svelte.js'), async (importOriginal) => ({
  ...(await importOriginal()),
  tString: (key: string, params?: Record<string, unknown>) => `${key} ${JSON.stringify(params ?? {})}`,
}))
vi.mock(import('#lib/shortcuts/index.js'), async (importOriginal) => ({
  ...(await importOriginal()),
  getEffectiveShortcuts: () => ['F3'],
  toDisplayShortcut: (combo: string) => combo,
}))
vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: () => [] }))

import { openWithDefaultAppOrExplain } from './default-app-open'

const { openFile, announceOpening, addToast } = m

function fileAt(path: string): FileEntry {
  return {
    name: path.slice(path.lastIndexOf('/') + 1),
    path,
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'user',
    group: 'staff',
    iconId: 'file',
  }
}

/** The one toast raised, as `[text, options]`. */
function onlyToast(): [string, { id?: string; level?: string }] {
  expect(addToast).toHaveBeenCalledOnce()
  return addToast.mock.calls[0] as [string, { id?: string; level?: string }]
}

beforeEach(() => {
  vi.clearAllMocks()
  openFile.mockResolvedValue(undefined)
  announceOpening.mockResolvedValue(undefined)
})

describe('a row with no file another app can open', () => {
  it('refuses a phone’s file with a toast pointing at the viewer, and launches nothing', async () => {
    await expect(openWithDefaultAppOrExplain('adb-r58m', fileAt('adb://R58M/sdcard/notes.txt'))).resolves.toBe(
      'refusedNotOnThisMac',
    )

    expect(openFile).not.toHaveBeenCalled()
    expect(announceOpening).not.toHaveBeenCalled()
    const [text, options] = onlyToast()
    expect(text).toContain('fileExplorer.open.notOnThisMac')
    expect(text).toContain('F3')
    expect(options.level).toBe('info')
  })

  it('refuses a backup snapshot’s file, whose path looks like a plain one', async () => {
    await expect(
      openWithDefaultAppOrExplain('root', fileAt('/Users/test_user_10/repo/Browse snapshots/2026-10-01/notes.txt')),
    ).resolves.toBe('refusedNotOnThisMac')

    expect(openFile).not.toHaveBeenCalled()
  })
})

describe('a row that looks openable', () => {
  it('opens it on its own volume, announcing the launch', async () => {
    const entry = fileAt('/Users/test_user_10/notes.txt')

    await expect(openWithDefaultAppOrExplain('root', entry)).resolves.toBe('opened')

    expect(openFile).toHaveBeenCalledExactlyOnceWith('/Users/test_user_10/notes.txt', 'root')
    expect(announceOpening).toHaveBeenCalledExactlyOnceWith(entry)
    expect(addToast).not.toHaveBeenCalled()
  })

  // The reported bug: a direct SMB share whose macOS mount is gone lists
  // `/Volumes/…` rows nothing can open, and Enter on one did nothing at all.
  it('says why when the volume itself refuses, replacing the "Opening…" toast', async () => {
    openFile.mockRejectedValue(new Refused({ type: 'notOnThisMac' }))

    await expect(openWithDefaultAppOrExplain('smb-nas-public', fileAt('/Volumes/public/report.pdf'))).resolves.toBe(
      'refusedNotOnThisMac',
    )

    const [text, options] = onlyToast()
    expect(text).toContain('fileExplorer.open.notOnThisMac')
    expect(options.id).toBe('opening-file')
  })

  it('says the file didn’t open when macOS refuses it', async () => {
    openFile.mockRejectedValue(new Refused({ type: 'notOpened', exitCode: 1 }))

    await expect(openWithDefaultAppOrExplain('root', fileAt('/Users/test_user_10/report.pdf'))).resolves.toBe(
      'notOpened',
    )

    const [text, options] = onlyToast()
    expect(text).toContain('fileExplorer.open.didNotOpen')
    expect(text).toContain('report.pdf')
    expect(options).toMatchObject({ id: 'opening-file', level: 'warn' })
  })

  it('stays quiet when macOS didn’t answer in time, since the app may still open it', async () => {
    openFile.mockRejectedValue(new Refused({ type: 'timedOut' }))

    await expect(openWithDefaultAppOrExplain('root', fileAt('/Users/test_user_10/report.pdf'))).resolves.toBe(
      'notOpened',
    )

    expect(addToast).not.toHaveBeenCalled()
  })

  it('raises the reason only after the "Opening…" toast has landed, so the reason wins', async () => {
    let landOpeningToast = (): void => {}
    announceOpening.mockReturnValue(
      new Promise<void>((resolve) => {
        landOpeningToast = resolve
      }),
    )
    openFile.mockRejectedValue(new Refused({ type: 'notOpened', exitCode: 1 }))

    const opening = openWithDefaultAppOrExplain('root', fileAt('/Users/test_user_10/report.pdf'))
    await Promise.resolve()
    await Promise.resolve()
    expect(addToast).not.toHaveBeenCalled()

    landOpeningToast()
    await opening
    expect(addToast).toHaveBeenCalledOnce()
  })
})
