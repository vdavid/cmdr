/**
 * What the user sees when a run of chained renames doesn't apply, checked
 * against the REAL toast store.
 *
 * That store is why this file is separate from `rename-chain.test.ts` (which
 * stubs it): it holds five toasts and silently DROPS a new one once every slot
 * is persistent. One toast per kept name therefore loses everything past the
 * fifth without a trace, which is the one thing a feature that silently drops
 * names must never do.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const {
  executeRenameSaveSpy,
  checkPermissionSpy,
  getSettingSpy,
  validateFilenameSpy,
  pathInsideArchiveSpy,
  tStringSpy,
} = vi.hoisted(() => ({
  executeRenameSaveSpy: vi.fn(),
  checkPermissionSpy: vi.fn<() => Promise<string | null>>(),
  getSettingSpy: vi.fn<(id: string) => unknown>(),
  validateFilenameSpy: vi.fn(),
  pathInsideArchiveSpy: vi.fn<() => boolean>(),
  tStringSpy: vi.fn((key: string, _params?: Record<string, unknown>) => key),
}))

vi.mock('$lib/tauri-commands', () => ({
  getFileAt: vi.fn(),
  getFileRange: vi.fn().mockResolvedValue([]),
  refreshListing: vi.fn(),
  getIpcErrorMessage: (e: unknown) => String(e),
  isIpcError: () => false,
  moveToTrash: vi.fn(),
}))
vi.mock('$lib/utils/filename-validation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/utils/filename-validation')>()),
  validateFilename: validateFilenameSpy,
}))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('../rename/rename-operations', () => ({
  executeRenameSave: executeRenameSaveSpy,
  performRename: vi.fn(),
  checkPermission: checkPermissionSpy,
}))
vi.mock('$lib/settings', () => ({ getSetting: getSettingSpy }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: tStringSpy }))
vi.mock('./volume-capabilities', () => ({ pathInsideArchive: pathInsideArchiveSpy }))

import { refreshListing } from '$lib/tauri-commands'
import { clearAllToasts, dismissToast, getToasts } from '$lib/ui/toast'
import { buildFlow, chainListing } from './test-rename-flow'

/** The X button on a toast: what `ToastContainer.handleUserDismiss` does. */
function dismissAsUser(): void {
  const toast = getToasts()[0]
  toast.onDismiss?.()
  dismissToast(toast.id)
}

/** The params the message about kept names was last built from. */
function lastKeptNamesParams(): Record<string, unknown> | undefined {
  const calls = tStringSpy.mock.calls.filter(([key]) => key.startsWith('fileExplorer.rename.chainKept'))
  return calls[calls.length - 1]?.[1]
}

/** The params the message about unconfirmed renames was last built from. */
function lastUnconfirmedParams(): Record<string, unknown> | undefined {
  const calls = tStringSpy.mock.calls.filter(([key]) => key.startsWith('fileExplorer.rename.unconfirmed'))
  return calls[calls.length - 1]?.[1]
}

/** One pane, and a way to run the editor down a run of rows typing unusable names. */
function paneWithRows(rowCount: number) {
  const names = Array.from({ length: rowCount }, (_, i) => `f${String(i)}.txt`)
  const listing = chainListing(names)
  // A chain here can be followed by another one, and a rename started outside a
  // chain opens on the row the cursor has since moved to.
  const { rename, flow } = buildFlow(listing.entryUnderCursor, true, listing.deps)

  /** Opens the editor and steps down `steps` times, dropping a name at each row. */
  function chainThrough(steps: number) {
    flow.startRename()
    for (let step = 0; step < steps; step++) {
      flow.handleRenameInput(`bad-${String(step)}/name.txt`)
      flow.handleRenameStep('down', rename.sessionId)
    }
  }

  return { rename, flow, chainThrough }
}

beforeEach(() => {
  vi.clearAllMocks()
  clearAllToasts()
  checkPermissionSpy.mockResolvedValue(null)
  pathInsideArchiveSpy.mockReturnValue(false)
  validateFilenameSpy.mockReturnValue({ severity: 'error', message: 'unusable' })
  getSettingSpy.mockImplementation((id) => (id === 'fileOperations.allowFileExtensionChanges' ? 'ask' : undefined))
})

describe('telling the user which names a chain did not apply', () => {
  it('names the one file that kept its name, and why', () => {
    paneWithRows(4).chainThrough(1)

    expect(getToasts()).toHaveLength(1)
    expect(getToasts()[0].content).toBe('fileExplorer.rename.chainKeptOriginalName')
    expect(lastKeptNamesParams()).toMatchObject({ name: 'f0.txt', reason: 'unusable' })
    expect(getToasts()[0]).toMatchObject({ level: 'warn', dismissal: 'persistent', originPane: 'left' })
  })

  it('holds six kept names in ONE toast, counted, rather than losing the tail', () => {
    paneWithRows(8).chainThrough(6)

    // Six separate persistent toasts would fill the stack at five, and the sixth
    // would vanish with nothing said.
    expect(getToasts()).toHaveLength(1)
    expect(getToasts()[0].content).toBe('fileExplorer.rename.chainKeptOriginalNameAndOthers')
    expect(lastKeptNamesParams()).toMatchObject({ name: 'f5.txt', reason: 'unusable', others: 5 })
  })

  it('outlives the typing that follows it, which clears this pane’s transient toasts', () => {
    const { flow, chainThrough } = paneWithRows(6)
    chainThrough(2)

    flow.handleRenameInput('the-next-name.txt')

    expect(getToasts()).toHaveLength(1)
  })

  it('keeps counting through a new chain while the toast is still up', () => {
    const { chainThrough } = paneWithRows(8)
    chainThrough(2)
    chainThrough(1)

    // Nothing was acknowledged in between, so all three files are still waiting
    // to be heard about.
    expect(getToasts()).toHaveLength(1)
    expect(lastKeptNamesParams()).toMatchObject({ others: 2 })
  })

  it('starts counting again once the user has dismissed it', () => {
    const { chainThrough } = paneWithRows(8)
    chainThrough(3)
    dismissAsUser()

    chainThrough(1)

    expect(getToasts()).toHaveLength(1)
    expect(getToasts()[0].content).toBe('fileExplorer.rename.chainKeptOriginalName')
  })
})

describe('a chained save the backend turns down', () => {
  /** Runs the editor down a run of rows typing names the backend refuses. */
  function chainAgainstARefusingBackend(steps: number) {
    validateFilenameSpy.mockReturnValue({ severity: 'ok', message: '' })
    executeRenameSaveSpy.mockResolvedValue({ type: 'error', message: "You don't have permission to rename this file" })
    const names = Array.from({ length: steps + 2 }, (_, i) => `f${String(i)}.txt`)
    const listing = chainListing(names)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    for (let step = 0; step < steps; step++) {
      flow.handleRenameInput(`renamed-${String(step)}.txt`)
      flow.handleRenameStep('down', rename.sessionId)
    }
    return { flow }
  }

  it('survives the typing that follows it, which clears this pane’s transient toasts', async () => {
    const { flow } = chainAgainstARefusingBackend(1)

    await vi.waitFor(() => {
      expect(getToasts()).toHaveLength(1)
    })
    // The user is typing the next name the instant the refusal lands, and that
    // keystroke wipes this pane's transient toasts.
    flow.handleRenameInput('the-next-name.txt')

    expect(getToasts()).toHaveLength(1)
  })

  it('holds six refusals in ONE toast, counted, rather than losing the tail', async () => {
    chainAgainstARefusingBackend(6)

    await vi.waitFor(() => {
      expect(lastKeptNamesParams()).toMatchObject({ others: 5 })
    })
    // Six separate toasts would fill the five-slot stack and drop the last one.
    expect(getToasts()).toHaveLength(1)
    expect(getToasts()[0].content).toBe('fileExplorer.rename.chainKeptOriginalNameAndOthers')
    expect(lastKeptNamesParams()).toMatchObject({ name: 'f5.txt' })
  })
})

describe('a chained save the volume never answers', () => {
  /** Names that pass validation, one per step. */
  function typedNames(count: number): string[] {
    return Array.from({ length: count }, (_, i) => `renamed-${String(i)}.txt`)
  }

  /**
   * Runs the editor down a run of rows against a volume that never confirms.
   * A typed name starting with `bad-` is unusable, so it's dropped at the
   * keypress and never reaches the volume.
   */
  function chainAgainstASilentVolume(typed: string[]) {
    validateFilenameSpy.mockImplementation((value: string) =>
      value.startsWith('bad-') ? { severity: 'error', message: 'unusable' } : { severity: 'ok', message: '' },
    )
    executeRenameSaveSpy.mockResolvedValue({ type: 'timeout' })
    const names = Array.from({ length: typed.length + 3 }, (_, i) => `f${String(i)}.txt`)
    const listing = chainListing(names)
    const { rename, flow } = buildFlow(listing.staleEntryUnderCursor, true, listing.deps)

    flow.startRename()
    /** Types one name and steps to the next row, the way holding the arrow does. */
    const step = (name: string) => {
      flow.handleRenameInput(name)
      flow.handleRenameStep('down', rename.sessionId)
    }
    typed.forEach(step)
    return { flow, step }
  }

  it('names the one file whose rename went unanswered, without claiming it kept its name', async () => {
    chainAgainstASilentVolume(typedNames(1))

    await vi.waitFor(() => {
      expect(getToasts()).toHaveLength(1)
    })
    expect(getToasts()[0].content).toBe('fileExplorer.rename.unconfirmed')
    expect(lastUnconfirmedParams()).toMatchObject({ name: 'f0.txt' })
    expect(getToasts()[0]).toMatchObject({ level: 'warn', dismissal: 'persistent', originPane: 'left' })
  })

  it('holds six unanswered renames in ONE toast, counted, rather than losing the tail', async () => {
    chainAgainstASilentVolume(typedNames(6))

    await vi.waitFor(() => {
      expect(lastUnconfirmedParams()).toMatchObject({ others: 5 })
    })
    expect(getToasts()).toHaveLength(1)
    expect(getToasts()[0].content).toBe('fileExplorer.rename.unconfirmedAndOthers')
    expect(lastUnconfirmedParams()).toMatchObject({ name: 'f5.txt' })
  })

  it('leaves room for the toast about names the chain dropped', async () => {
    const { step } = chainAgainstASilentVolume(typedNames(6))
    await vi.waitFor(() => {
      expect(lastUnconfirmedParams()).toMatchObject({ others: 5 })
    })

    // A toast per unanswered rename would have filled all five slots by now,
    // and this one, the honest report the chain is built around, would be
    // dropped with nothing said.
    step('bad-6/name.txt')

    expect(getToasts()).toHaveLength(2)
    expect(getToasts().map((toast) => toast.content)).toContain('fileExplorer.rename.chainKeptOriginalName')
    expect(getToasts().map((toast) => toast.content)).toContain('fileExplorer.rename.unconfirmedAndOthers')
  })

  it('asks the struggling volume for ONE refresh, however many renames went unanswered', async () => {
    vi.useFakeTimers()
    try {
      chainAgainstASilentVolume(typedNames(6))

      await vi.advanceTimersByTimeAsync(5000)

      // Six refreshes at a volume already too slow to answer is the storm this
      // feature must not cause.
      expect(refreshListing).toHaveBeenCalledTimes(1)
      expect(refreshListing).toHaveBeenCalledWith('lst-1')
    } finally {
      vi.useRealTimers()
    }
  })

  it('refreshes again for a rename that goes unanswered after the burst settled', async () => {
    vi.useFakeTimers()
    try {
      const { step } = chainAgainstASilentVolume(typedNames(2))
      await vi.advanceTimersByTimeAsync(5000)

      step('renamed-late.txt')
      await vi.advanceTimersByTimeAsync(5000)

      expect(refreshListing).toHaveBeenCalledTimes(2)
    } finally {
      vi.useRealTimers()
    }
  })
})
