import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { ExtensionChangePolicy } from '$lib/settings'

// Capture the extension policy `executeFlow` passes to `executeRenameSave` — the
// observable of the suppression plumbing (`effectiveExtensionPolicy()`).
const {
  executeRenameSaveSpy,
  checkPermissionSpy,
  getSettingSpy,
  validateFilenameSpy,
  pathInsideArchiveSpy,
  addToastSpy,
} = vi.hoisted(() => ({
  // Mirrors the real, unconverted `executeRenameSave` (`rename/rename-operations.ts`):
  // `trimmedName` and `volumeId` are both `string` in production too, and its one real
  // call site (`rename-flow.svelte.ts`, untouched, out of scope here) invokes it
  // positionally. An object payload here would misrepresent that positional shape.
  executeRenameSaveSpy:
    vi.fn<
      // eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors real, unconverted executeRenameSave(target, trimmedName, extensionPolicy, skipExtensionCheck?, volumeId?); see comment above
      (
        target: { path: string; originalName: string },
        trimmedName: string,
        extensionPolicy: ExtensionChangePolicy,
        skipExtensionCheck?: boolean,
        volumeId?: string,
      ) => Promise<unknown>
    >(),
  checkPermissionSpy: vi.fn<() => Promise<string | null>>(),
  getSettingSpy: vi.fn<(id: string) => unknown>(),
  validateFilenameSpy: vi.fn(),
  pathInsideArchiveSpy: vi.fn<() => boolean>(),
  addToastSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  getFileAt: vi.fn(),
  getFileRange: vi.fn(),
  refreshListing: vi.fn(),
  moveToTrash: vi.fn(),
}))
vi.mock('$lib/utils/filename-validation', () => ({
  validateFilename: validateFilenameSpy,
  getExtension: (name: string) => {
    const i = name.lastIndexOf('.')
    return i > 0 ? name.slice(i) : ''
  },
}))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('../rename/rename-operations', () => ({
  executeRenameSave: executeRenameSaveSpy,
  performRename: vi.fn(),
  checkPermission: checkPermissionSpy,
}))
vi.mock('$lib/settings', () => ({ getSetting: getSettingSpy }))
vi.mock('$lib/ui/toast', () => ({ addToastForPane: addToastSpy, dismissTransientToastsForPane: vi.fn() }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (k: string) => k }))
vi.mock('./volume-capabilities', () => ({ pathInsideArchive: pathInsideArchiveSpy }))

import { refreshListing } from '$lib/tauri-commands'
import { buildFlow, deferred, PASTED, type Entry } from './test-rename-flow'

const ERROR_VALIDATION = { severity: 'error', message: 'Filename can\'t contain "/" or null characters' }

/** Drives a rename to submit, renaming `pasted.txt` → `notes.md` (an extension change). */
async function renameToMd(
  flow: ReturnType<typeof buildFlow>['flow'],
  options?: { suppressExtensionWarning?: boolean },
) {
  flow.startRename(options)
  flow.handleRenameInput('notes.md')
  flow.handleRenameSubmit()
  await vi.waitFor(() => {
    expect(executeRenameSaveSpy).toHaveBeenCalled()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  checkPermissionSpy.mockResolvedValue(null)
  pathInsideArchiveSpy.mockReturnValue(false)
  validateFilenameSpy.mockReturnValue({ severity: 'ok', message: '' })
  // The user's global extension-change setting; the suppression must override it.
  getSettingSpy.mockImplementation((id) => (id === 'fileOperations.allowFileExtensionChanges' ? 'ask' : undefined))
  executeRenameSaveSpy.mockResolvedValue({ type: 'success', newName: 'notes.md' })
})

describe('rename extension-warning suppression (paste auto-rename)', () => {
  it('an auto-started rename passes policy "yes" (suppresses the extension-change dialog)', async () => {
    const { flow } = buildFlow()

    await renameToMd(flow, { suppressExtensionWarning: true })

    // 3rd arg to executeRenameSave is the effective extension policy.
    expect(executeRenameSaveSpy.mock.calls[0][2]).toBe('yes')
  })

  it('a normal (F2) rename passes the user setting ("ask"), so the dialog still fires', async () => {
    const { flow } = buildFlow()

    await renameToMd(flow) // no options → not suppressed

    expect(executeRenameSaveSpy.mock.calls[0][2]).toBe('ask')
  })

  it('suppression is one-shot: it does NOT leak into the next rename after the paste rename completes', async () => {
    const { flow } = buildFlow()

    // First: the suppressed auto-rename (completes successfully → resets the flag).
    await renameToMd(flow, { suppressExtensionWarning: true })
    expect(executeRenameSaveSpy.mock.calls[0][2]).toBe('yes')

    executeRenameSaveSpy.mockClear()

    // Then a normal F2 rename must warn again (policy back to the user setting).
    await renameToMd(flow)
    expect(executeRenameSaveSpy.mock.calls[0][2]).toBe('ask')
  })

  it('cancelling a suppressed rename also clears the flag (next rename warns)', async () => {
    const { flow } = buildFlow()

    flow.startRename({ suppressExtensionWarning: true })
    flow.cancelRename()

    await renameToMd(flow)
    expect(executeRenameSaveSpy.mock.calls[0][2]).toBe('ask')
  })
})

describe('startRename expectedName guard (auto-rename must land on the new file, not a neighbor)', () => {
  // The guard polls (~50 ms, up to ~2 s) for the entry-under-cursor to become
  // `expectedName` while the synthetic diff lands. Fake timers drive the poll.
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  const ZIP: Entry = { name: 'somezip.zip', path: '/dir/somezip.zip', isDirectory: false }

  it('DATA SAFETY: never activates on a mismatched entry, even after the whole poll window', async () => {
    // The cursor is stuck on the user's zip (diff never lands). Activating here
    // would let the next keystroke rename the WRONG file. It must give up silently.
    const { rename, flow } = buildFlow(() => ZIP)

    flow.startRename({ suppressExtensionWarning: true, expectedName: 'pasted.txt' })
    expect(rename.active).toBe(false) // not on the first (synchronous) check

    await vi.advanceTimersByTimeAsync(2100) // past the ~2 s poll window
    expect(rename.active).toBe(false) // gave up silently — NEVER latched the zip
  })

  it('activates on the RIGHT file once the diff repositions the cursor during the poll', async () => {
    let entry: Entry = ZIP
    const { rename, flow } = buildFlow(() => entry)

    flow.startRename({ suppressExtensionWarning: true, expectedName: 'pasted.txt' })
    expect(rename.active).toBe(false)

    entry = PASTED // the synthetic diff lands, cursor now on pasted.txt
    await vi.advanceTimersByTimeAsync(60) // next poll tick
    expect(rename.active).toBe(true)
    expect(rename.target?.originalName).toBe('pasted.txt')
  })

  it('activates immediately when the entry already matches (no poll needed)', () => {
    const { rename, flow } = buildFlow(() => PASTED)

    flow.startRename({ suppressExtensionWarning: true, expectedName: 'pasted.txt' })

    expect(rename.active).toBe(true)
    expect(rename.target?.originalName).toBe('pasted.txt')
  })

  it('a cancel during the pending poll clears it — a later diff cannot resurrect the rename', async () => {
    // A `loadDirectory` reread in a busy dir cancels the rename mid-poll. Even if
    // pasted.txt later lands under the cursor, the cleared poll must NOT activate.
    let entry: Entry = ZIP
    const { rename, flow } = buildFlow(() => entry)

    flow.startRename({ suppressExtensionWarning: true, expectedName: 'pasted.txt' })
    flow.cancelRename()

    entry = PASTED
    await vi.advanceTimersByTimeAsync(2100)
    expect(rename.active).toBe(false)
  })

  it('F2 (no expectedName) activates immediately on whatever entry is under the cursor', () => {
    const { rename, flow } = buildFlow(() => ({ name: 'anything.txt', path: '/dir/anything.txt', isDirectory: false }))

    flow.startRename()

    expect(rename.active).toBe(true)
    expect(rename.target?.originalName).toBe('anything.txt')
  })
})

describe('Enter (submit) ends the session the way the user asked', () => {
  it('a changed valid name saves', async () => {
    const { rename, flow } = buildFlow()

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameSubmit()

    await vi.waitFor(() => {
      expect(executeRenameSaveSpy).toHaveBeenCalled()
    })
    expect(executeRenameSaveSpy.mock.calls[0][1]).toBe('notes.md')
    await vi.waitFor(() => {
      expect(rename.active).toBe(false)
    })
  })

  it('an unchanged name ends the rename without touching the disk', () => {
    const { rename, flow } = buildFlow()

    flow.startRename()
    flow.handleRenameInput('pasted.txt') // same as the original
    flow.handleRenameSubmit()

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.active).toBe(false)
  })

  it('an invalid name shakes and KEEPS the editor open, so the user can fix it', () => {
    const { rename, flow } = buildFlow()
    validateFilenameSpy.mockReturnValue(ERROR_VALIDATION)

    flow.startRename()
    flow.handleRenameInput('bad/name.txt')
    flow.handleRenameSubmit()

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.active).toBe(true) // still editing
    expect(rename.shaking).toBe(true)
    expect(addToastSpy).toHaveBeenCalled()
  })
})

describe('cancel (Escape, Tab, editor unmount)', () => {
  it('discards the edit and hands focus back to the pane', () => {
    const { rename, flow, onRequestFocus } = buildFlow()

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameCancel(rename.sessionId)

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.active).toBe(false)
    expect(onRequestFocus).toHaveBeenCalled()
  })

  it('the blur from an opening dialog does NOT discard the rename', async () => {
    const { rename, flow } = buildFlow()
    executeRenameSaveSpy.mockResolvedValue({
      type: 'conflict',
      validity: { conflict: { name: 'notes.md' } },
    })

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameSubmit()
    await vi.waitFor(() => {
      expect(flow.conflictDialogState).not.toBeNull()
    })

    flow.handleRenameCancel(rename.sessionId) // the dialog stealing focus blurred the editor
    expect(rename.active).toBe(true)

    // One-shot: the next cancel (a real Escape) still ends the session.
    flow.handleRenameCancel(rename.sessionId)
    expect(rename.active).toBe(false)
  })
})

describe('clicking outside the editor commits (Finder-style), never discards silently', () => {
  it('a changed valid name saves', async () => {
    const { rename, flow } = buildFlow()

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameClickAway()

    await vi.waitFor(() => {
      expect(executeRenameSaveSpy).toHaveBeenCalled()
    })
    expect(executeRenameSaveSpy.mock.calls[0][1]).toBe('notes.md')
    await vi.waitFor(() => {
      expect(rename.active).toBe(false)
    })
  })

  it('the click decides where focus lands, so the flow does not yank it back to the pane', async () => {
    const { flow, onRequestFocus } = buildFlow()

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameClickAway()

    await vi.waitFor(() => {
      expect(executeRenameSaveSpy).toHaveBeenCalled()
    })
    await vi.waitFor(() => {
      expect(onRequestFocus).not.toHaveBeenCalled()
    })
  })

  it('the blur that follows the click does NOT cancel the in-flight save', async () => {
    const { rename, flow } = buildFlow()
    const save = deferred<unknown>()
    executeRenameSaveSpy.mockReturnValue(save.promise)

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameClickAway()

    // The browser moves focus right after the click; the editor blurs.
    flow.handleRenameCancel(rename.sessionId)
    expect(rename.active).toBe(true) // the save owns the session now

    save.resolve({ type: 'success', newName: 'notes.md' })
    await vi.waitFor(() => {
      expect(rename.active).toBe(false)
    })
  })

  it('an unchanged name ends the rename quietly (no save, no toast)', () => {
    const { rename, flow } = buildFlow()

    flow.startRename()
    flow.handleRenameClickAway()

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(addToastSpy).not.toHaveBeenCalled()
    expect(rename.active).toBe(false)
  })

  it('an invalid name keeps the original name and says why (never traps the click)', () => {
    const { rename, flow } = buildFlow()
    validateFilenameSpy.mockReturnValue(ERROR_VALIDATION)

    flow.startRename()
    flow.handleRenameInput('bad/name.txt')
    flow.handleRenameClickAway()

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(rename.active).toBe(false) // the click goes through; no stranded editor
    expect(addToastSpy.mock.calls[0][1]).toContain('keptOriginalName')
  })

  it('a save that comes back with a problem ends the session (nothing left to shake)', async () => {
    const { rename, flow } = buildFlow()
    executeRenameSaveSpy.mockResolvedValue({ type: 'error', message: 'The disk is read-only' })

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameClickAway()

    await vi.waitFor(() => {
      expect(rename.active).toBe(false)
    })
    expect(addToastSpy).toHaveBeenCalled()
  })

  it('a dialog opened by a click-away commit leaves Escape working', async () => {
    // The editor already blurred when the click landed, so the dialog opening
    // costs no second blur — arming the suppression would eat the user's Escape.
    const { rename, flow } = buildFlow()
    executeRenameSaveSpy.mockResolvedValue({
      type: 'conflict',
      validity: { conflict: { name: 'notes.md' } },
    })

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameClickAway()
    await vi.waitFor(() => {
      expect(flow.conflictDialogState).not.toBeNull()
    })

    flow.handleRenameCancel(rename.sessionId)
    expect(rename.active).toBe(false)
  })

  it('ignores clicks while a dialog is up: the dialog owns that decision', async () => {
    const { flow } = buildFlow()
    executeRenameSaveSpy.mockResolvedValue({
      type: 'conflict',
      validity: { conflict: { name: 'notes.md' } },
    })

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameSubmit()
    await vi.waitFor(() => {
      expect(flow.conflictDialogState).not.toBeNull()
    })
    executeRenameSaveSpy.mockClear()

    flow.handleRenameClickAway() // the user pressing a dialog button

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
    expect(flow.conflictDialogState).not.toBeNull()
  })

  it('is a no-op when no rename is running', () => {
    const { flow } = buildFlow()

    flow.handleRenameClickAway()

    expect(executeRenameSaveSpy).not.toHaveBeenCalled()
  })
})

describe('a superseded rename session may speak, never steer', () => {
  const NEXT: Entry = { name: 'next.txt', path: '/dir/next.txt', isDirectory: false }

  /**
   * Sends a save on `pasted.txt`, then activates a second session on `next.txt`
   * while that save is still in flight. The test resolves the save by hand and
   * asserts on what the late result was allowed to touch.
   */
  function supersededSave(showHiddenFiles = true) {
    let entry: Entry = PASTED
    const { rename, flow, onRequestFocus } = buildFlow(() => entry, showHiddenFiles)
    const save = deferred<unknown>()
    executeRenameSaveSpy.mockReturnValue(save.promise)

    flow.startRename()
    flow.handleRenameInput('notes.md')
    flow.handleRenameSubmit()
    const staleSessionId = rename.sessionId

    entry = NEXT
    flow.startRename() // the user has moved on; this session owns the editor now

    /** Lets the save's continuation run before we assert on what it did. */
    const landSave = async (result: unknown) => {
      save.resolve(result)
      await save.promise
      await Promise.resolve()
    }

    return { rename, flow, onRequestFocus, staleSessionId, landSave }
  }

  it('a save landing after the user moved on leaves the live session editing', async () => {
    const { rename, onRequestFocus, landSave } = supersededSave()

    await landSave({ type: 'success', newName: 'notes.md' })

    expect(rename.active).toBe(true)
    expect(rename.target?.path).toBe(NEXT.path)
    expect(onRequestFocus).not.toHaveBeenCalled() // focus belongs to the live editor
  })

  it('a save landing after the user moved on does not drag the cursor back to the file it renamed', async () => {
    const { flow, landSave } = supersededSave()

    await landSave({ type: 'success', newName: 'notes.md' })

    expect(flow.pendingCursorName).toBeNull()
  })

  it('a save landing after the user moved on still says the file went hidden', async () => {
    const { landSave } = supersededSave(false)

    await landSave({ type: 'success', newName: '.notes.md' })

    expect(addToastSpy.mock.calls[0][1]).toContain('hiddenAfterRename')
  })

  it('a problem reported after the user moved on toasts, but never shakes the file that is now being edited', async () => {
    const { rename, landSave } = supersededSave()

    await landSave({ type: 'error', message: 'The disk is read-only' })

    expect(addToastSpy).toHaveBeenCalled()
    expect(rename.shaking).toBe(false)
    expect(rename.active).toBe(true)
  })

  it('a timeout reported after the user moved on still warns, and refreshes once the volume goes quiet', async () => {
    vi.useFakeTimers()
    try {
      const { rename, landSave } = supersededSave()

      await landSave({ type: 'timeout' })

      expect(addToastSpy).toHaveBeenCalled()
      await vi.advanceTimersByTimeAsync(2000)
      expect(refreshListing).toHaveBeenCalledWith('lst-1', false)
      expect(rename.active).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it('a conflict reported after the user moved on never opens a dialog about it', async () => {
    const { rename, flow, landSave } = supersededSave()

    await landSave({ type: 'conflict', validity: { conflict: { name: 'notes.md' } } })

    expect(flow.conflictDialogState).toBeNull()
    expect(rename.active).toBe(true)
  })

  it('the blur from the superseded editor unmounting does not end the live session', async () => {
    const { rename, flow, staleSessionId, landSave } = supersededSave()

    flow.handleRenameCancel(staleSessionId)
    expect(rename.active).toBe(true)
    expect(rename.target?.path).toBe(NEXT.path)

    // The live editor's own Escape still ends it.
    flow.handleRenameCancel(rename.sessionId)
    expect(rename.active).toBe(false)

    await landSave({ type: 'success', newName: 'notes.md' })
  })
})
