import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

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
  executeRenameSaveSpy: vi.fn(),
  checkPermissionSpy: vi.fn<() => Promise<string | null>>(),
  getSettingSpy: vi.fn<(id: string) => unknown>(),
  validateFilenameSpy: vi.fn(),
  pathInsideArchiveSpy: vi.fn<() => boolean>(),
  addToastSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  getFileRange: vi.fn(),
  refreshListing: vi.fn(),
  getIpcErrorMessage: (e: unknown) => String(e),
  isIpcError: () => false,
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
vi.mock('$lib/ui/toast', () => ({ addToast: addToastSpy, dismissTransientToasts: vi.fn() }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (k: string) => k }))
vi.mock('./volume-capabilities', () => ({ pathInsideArchive: pathInsideArchiveSpy }))

import { createRenameFlow } from './rename-flow.svelte'
import { createRenameState } from '../rename/rename-state.svelte'

type Entry = { name: string; path: string; isDirectory: boolean }
const PASTED: Entry = { name: 'pasted.txt', path: '/dir/pasted.txt', isDirectory: false }

function buildFlow(getEntry: () => Entry | undefined = () => PASTED) {
  const rename = createRenameState()
  const flow = createRenameFlow({
    rename,
    getListingId: () => 'lst-1',
    getTotalCount: () => 0, // 0 → loadSiblingNames returns [] without hitting getFileRange
    getIncludeHidden: () => false,
    getCurrentPath: () => '/dir',
    getCursorIndex: () => 0,
    getShowHiddenFiles: () => true,
    getVolumeId: () => 'root',
    getEntryUnderCursor: () => getEntry() as never,
    onRequestFocus: () => {},
  })
  return { rename, flow }
}

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
