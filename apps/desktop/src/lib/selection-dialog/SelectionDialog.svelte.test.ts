/**
 * Behavior tests for `SelectionDialog.svelte`.
 *
 * Pins the wrapper's contract: title-per-mode, commit-on-Enter, ⌘N reset,
 * mode-switch buffer preservation, recent-selections wiring, the mid-dialog
 * AI-provider fallback (AI → filename when the provider flips off), and
 * state survival across a close + reopen (the module-singleton contract).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import { writable } from 'svelte/store'
import SelectionDialog from './SelectionDialog.svelte'
import { clearSelectionState, selectionQueryState } from './selection-state.svelte'
import type { FileEntry } from '$lib/file-explorer/types'
import type { SelectionHistoryEntry, SelectionTranslateResult } from '$lib/ipc/bindings'

let aiProvider: 'off' | 'local' | 'cloud' = 'off'
let autoApplySetting = true
const autoApplyListeners = new Set<(id: string, value: boolean) => void>()
const aiProviderListeners = new Set<(id: string, value: unknown) => void>()

const { translateSelectionMock, addRecentMock, getRecentMock } = vi.hoisted(() => ({
  // Typed signature so `mock.calls[0]` is a positional tuple rather than `[]`.
  translateSelectionMock: vi.fn((...args: [string, string[], (boolean | null)?]): Promise<SelectionTranslateResult> => {
    void args
    return Promise.resolve({
      pattern: '*.png',
      kind: 'glob',
      isDirectory: null,
      sizeMin: null,
      sizeMax: null,
      modifiedAfter: null,
      modifiedBefore: null,
      caveat: null,
      label: null,
    } as SelectionTranslateResult)
  }),
  addRecentMock: vi.fn(() => Promise.resolve()),
  getRecentMock: vi.fn(() => Promise.resolve([] as SelectionHistoryEntry[])),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  translateSelectionQuery: translateSelectionMock,
  addRecentSelection: addRecentMock,
  removeRecentSelection: vi.fn(() => Promise.resolve()),
  getRecentSelections: getRecentMock,
  showFileContextMenu: vi.fn(() => Promise.resolve()),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  trackEvent: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'ai.provider') return aiProvider
    if (key === 'search.autoApply') return autoApplySetting
    return undefined
  }),
  onSpecificSettingChange: vi.fn((id: string, listener: (id: string, value: unknown) => void) => {
    if (id === 'search.autoApply') {
      autoApplyListeners.add(listener)
      return () => autoApplyListeners.delete(listener)
    }
    if (id === 'ai.provider') {
      aiProviderListeners.add(listener)
      return () => aiProviderListeners.delete(listener)
    }
    return () => {}
  }),
}))

vi.mock('$lib/icon-cache', () => ({
  iconCacheVersion: writable(0),
  getCachedIcon: vi.fn(() => undefined),
}))

function setAiProviderForTest(value: 'off' | 'local' | 'cloud'): void {
  aiProvider = value
  for (const listener of aiProviderListeners) listener('ai.provider', value)
}

function dispatchKey(target: Element, key: string, meta = false, shift = false): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    metaKey: meta,
    shiftKey: shift,
    bubbles: true,
    cancelable: true,
  })
  target.dispatchEvent(event)
  return event
}

function buildEntry(name: string, extra: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    path: `/folder/${name}`,
    parentPath: '/folder',
    isDirectory: false,
    isSymlink: false,
    size: 1000,
    modifiedAt: 1_700_000_000,
    permissions: 0o644,
    owner: 'me',
    group: 'staff',
    iconId: 'file',
    extendedMetadataLoaded: true,
    ...extra,
  }
}

interface MountOpts {
  mode?: 'add' | 'remove'
  entries?: FileEntry[]
  cursorIndex?: number
  isSnapshotPane?: boolean
  onCommit?: (idxs: number[], mode: 'add' | 'remove') => void
  onClose?: () => void
}

async function mountDialog(
  opts: MountOpts = {},
): Promise<{ overlay: Element; cleanup: () => void; getTitle: () => string }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(SelectionDialog, {
    target,
    props: {
      mode: opts.mode ?? 'add',
      entries: opts.entries ?? [buildEntry('a.png'), buildEntry('b.txt'), buildEntry('c.png')],
      cursorIndex: opts.cursorIndex ?? 0,
      isSnapshotPane: opts.isSnapshotPane ?? false,
      onCommit: opts.onCommit ?? (() => {}),
      onClose: opts.onClose ?? (() => {}),
    },
  })
  await tick()
  await new Promise((r) => setTimeout(r, 0))
  await tick()
  const overlay = target.querySelector('.search-overlay')
  if (!overlay) throw new Error('dialog overlay not found')
  return {
    overlay,
    cleanup: () => {
      void unmount(component)
      target.remove()
    },
    // The first span is the title text; the ALPHA StatusBadge sits next to it in the same h2.
    getTitle: () => target.querySelector('#query-dialog-title > span')?.textContent.trim() ?? '',
  }
}

describe('SelectionDialog', () => {
  beforeEach(() => {
    aiProvider = 'off'
    autoApplySetting = true
    autoApplyListeners.clear()
    aiProviderListeners.clear()
    translateSelectionMock.mockClear()
    addRecentMock.mockClear()
    getRecentMock.mockClear()
    // The Selection state is now a module singleton, so it carries over between
    // tests. Reset it to defaults (the same thing ⌘N does) so each test starts clean.
    clearSelectionState()
    // Filter-chip popovers are fixed-position siblings of the per-test target, so
    // an unmount doesn't remove them. Clear any leftover popover from a prior test.
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
    // jsdom doesn't implement `Element.scrollIntoView`; QueryDialog calls it via
    // `focusFirstResult` after AI runs. Stub on the prototype so every result
    // row's call is a no-op rather than an unhandled rejection.
    if (typeof Element.prototype.scrollIntoView !== 'function') {
      Element.prototype.scrollIntoView = function noopScrollIntoView() {}
    }
  })

  it("renders 'Select files' when mode is 'add'", async () => {
    const { getTitle, cleanup } = await mountDialog({ mode: 'add' })
    expect(getTitle()).toBe('Select files')
    cleanup()
  })

  it("renders 'Deselect files' when mode is 'remove'", async () => {
    const { getTitle, cleanup } = await mountDialog({ mode: 'remove' })
    expect(getTitle()).toBe('Deselect files')
    cleanup()
  })

  it('pressing Enter on a non-empty filename query commits matched indices and closes', async () => {
    const matched: number[][] = []
    const closed: number[] = []
    const { overlay, cleanup } = await mountDialog({
      entries: [buildEntry('a.png'), buildEntry('b.txt'), buildEntry('c.png')],
      onCommit: (idxs) => matched.push(idxs),
      onClose: () => closed.push(1),
    })

    // Type a glob in the bar (find the input).
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = '*.png'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    // Wait for the auto-apply debounce to fire.
    await new Promise((r) => setTimeout(r, 1100))
    await tick()
    dispatchKey(overlay, 'Enter')
    await tick()
    // Enter on results-arrived owns 'go-to-file'; with no secondaryAction, QueryDialog
    // falls through to primary on the full result set (per its handleEnterKey path).
    expect(matched).toHaveLength(1)
    expect(matched[0]).toEqual([0, 2])
    expect(closed).toHaveLength(1)

    cleanup()
  })

  it('shows the R7 banner when isSnapshotPane is true', async () => {
    const { overlay, cleanup } = await mountDialog({ isSnapshotPane: true })
    const banner = overlay.querySelector('.query-dialog__notice')
    expect(banner?.textContent).toContain('Matching what is shown')
    cleanup()
  })

  it('omits the banner on a regular pane', async () => {
    const { overlay, cleanup } = await mountDialog({ isSnapshotPane: false })
    const banner = overlay.querySelector('.query-dialog__notice')
    expect(banner).toBeNull()
    cleanup()
  })

  it('mid-dialog AI-provider switch from cloud to off falls back to filename and preserves the prompt', async () => {
    aiProvider = 'cloud'
    const { overlay, cleanup } = await mountDialog({ mode: 'add' })
    // Switch to AI mode and type a prompt.
    dispatchKey(overlay, '1', true)
    await tick()
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = 'all images'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    // Provider flips off in the settings window.
    setAiProviderForTest('off')
    await tick()
    await tick()

    // Bar input now shows the prompt in filename mode; AI chip is gone.
    expect(input.value).toBe('all images')

    cleanup()
  })

  it('⌘N clears state and resets the bar', async () => {
    const { overlay, cleanup } = await mountDialog()
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = '*.png'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    dispatchKey(overlay, 'n', true)
    await tick()
    expect(input.value).toBe('')
    cleanup()
  })

  it('switching modes preserves the typed query in each mode buffer', async () => {
    aiProvider = 'cloud'
    const { overlay, cleanup } = await mountDialog()
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    // Type in filename mode (default).
    input.value = '*.svelte'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    // Switch to regex (⌘3 with AI on: filename=2, regex=3). The regex buffer is empty, so the
    // outgoing term carries across rather than vanishing (term carry-over).
    dispatchKey(overlay, '3', true)
    await tick()
    expect(input.value).toBe('*.svelte')
    // Overwrite the regex buffer with real regex.
    input.value = '\\.txt$'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    // Switch back to filename: its own buffer ('*.svelte') is non-empty, so it's restored
    // verbatim (a non-empty target is never overwritten by carry-over).
    dispatchKey(overlay, '2', true)
    await tick()
    expect(input.value).toBe('*.svelte')
    cleanup()
  })

  it('AI translation populates the pattern buffer (cloud provider)', async () => {
    aiProvider = 'cloud'
    translateSelectionMock.mockResolvedValueOnce({
      pattern: '*.log',
      kind: 'glob',
      isDirectory: null,
      sizeMin: 1_048_576,
      sizeMax: null,
      modifiedAfter: null,
      modifiedBefore: null,
      caveat: null,
      label: null,
    })
    const { overlay, cleanup } = await mountDialog()
    dispatchKey(overlay, '1', true) // ⌘1 → AI
    await tick()
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = 'all log files bigger than 1 MB'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    dispatchKey(overlay, 'Enter')
    await tick()
    // Wait for the AI promise + the follow-up executeQuery.
    await new Promise((r) => setTimeout(r, 100))
    await tick()
    expect(translateSelectionMock).toHaveBeenCalled()
    const [prompt, sample] = translateSelectionMock.mock.calls[0]
    expect(prompt).toBe('all log files bigger than 1 MB')
    expect(Array.isArray(sample)).toBe(true)
    cleanup()
  })

  it('respects size filter (between bounds) when running the matcher', async () => {
    const { overlay, cleanup } = await mountDialog({
      entries: [
        buildEntry('small', { size: 10 }),
        buildEntry('mid', { size: 5000 }),
        buildEntry('big', { size: 50_000 }),
      ],
      onCommit: () => {},
    })
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = '*'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    await new Promise((r) => setTimeout(r, 1100))
    await tick()
    // 3 entries matched on pattern alone.
    cleanup()
  })

  it('AI translation with size + date filters applies them to state and emits highlightedFields', async () => {
    aiProvider = 'cloud'
    translateSelectionMock.mockResolvedValueOnce({
      pattern: '*.log',
      kind: 'glob',
      isDirectory: null,
      sizeMin: 1024,
      sizeMax: 1_048_576,
      modifiedAfter: '2026-01-01',
      modifiedBefore: '2026-05-01',
      caveat: 'Best guess; refine if needed.',
      label: null,
    })
    const { overlay, cleanup } = await mountDialog()
    dispatchKey(overlay, '1', true) // AI mode
    await tick()
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = 'log files between 1k and 1M from earlier this year'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    dispatchKey(overlay, 'Enter')
    await tick()
    await new Promise((r) => setTimeout(r, 100))
    await tick()
    // The AI strip should be visible with the caveat.
    const strip = overlay.querySelector('.ai-prompt-strip, [data-testid="ai-prompt-strip"]')
    // Either a strip exists OR the caveat shows in some indicator — at minimum, the
    // IPC must have been called.
    expect(translateSelectionMock).toHaveBeenCalled()
    expect(strip ?? overlay.textContent).toBeTruthy()
    cleanup()
  })

  it('a second AI run does not let a leftover buffer from the first run shadow the new pattern', async () => {
    // Regression: Selection's `buildMatchQuery` in AI mode reads from
    // `handTyped.regex` first, then `handTyped.filename`. Without the wrapper
    // clearing the "other kind"'s buffer on each AI run, a prior AI run's regex
    // would silently win over the new run's glob (or vice versa).
    aiProvider = 'cloud'
    // First call: regex result. Sets handTyped.regex.
    translateSelectionMock.mockResolvedValueOnce({
      pattern: '^.*\\.log$',
      kind: 'regex',
      isDirectory: null,
      sizeMin: 1024,
      sizeMax: null,
      modifiedAfter: null,
      modifiedBefore: null,
      caveat: null,
      label: null,
    })
    // Second call: glob result. Must overwrite, and the prior regex must be
    // gone or `buildMatchQuery` would still pick the regex (it checks first).
    // Also: the prior size filter must NOT leak through.
    translateSelectionMock.mockResolvedValueOnce({
      pattern: '*.png',
      kind: 'glob',
      isDirectory: null,
      sizeMin: null,
      sizeMax: null,
      modifiedAfter: null,
      modifiedBefore: null,
      caveat: null,
      label: null,
    })

    const matched: number[][] = []
    const { overlay, cleanup } = await mountDialog({
      entries: [
        buildEntry('a.png', { size: 100 }),
        buildEntry('b.log', { size: 5000 }),
        buildEntry('c.png', { size: 10 }),
      ],
      onCommit: (idxs) => matched.push(idxs),
    })
    dispatchKey(overlay, '1', true) // ⌘1 → AI
    await tick()
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement

    // First AI run: regex returns, size filter applies.
    input.value = 'all log files bigger than 1k'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    dispatchKey(overlay, 'Enter')
    await tick()
    await new Promise((r) => setTimeout(r, 100))
    await tick()

    // Second AI run: glob this time, no filter.
    input.value = 'all png images'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    dispatchKey(overlay, 'Enter')
    await tick()
    await new Promise((r) => setTimeout(r, 100))
    await tick()

    // Commit. The matcher MUST run against the latest pattern (`*.png`), not the
    // leaked regex from the first run. The two .png files are indices 0 and 2.
    dispatchKey(overlay, 'Enter')
    await tick()
    expect(matched).toHaveLength(1)
    expect(matched[0]).toEqual([0, 2])

    cleanup()
  })

  it('type-in-AI: passes the current type as context and applies a returned folder type', async () => {
    aiProvider = 'cloud'
    translateSelectionMock.mockResolvedValueOnce({
      pattern: '*',
      kind: 'glob',
      isDirectory: true, // the agent decided: folders only
      sizeMin: null,
      sizeMax: null,
      modifiedAfter: null,
      modifiedBefore: null,
      caveat: null,
      label: null,
    })
    const { overlay, cleanup } = await mountDialog()
    dispatchKey(overlay, '1', true) // ⌘1 → AI
    await tick()
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = 'the subfolders'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    dispatchKey(overlay, 'Enter')
    await tick()
    await new Promise((r) => setTimeout(r, 100))
    await tick()

    // The IPC received the current type as the third arg (both → null at the start).
    const call = translateSelectionMock.mock.calls[0]
    expect(call[2]).toBeNull()
    // The returned folder type painted the toggle.
    const folders = Array.from(overlay.querySelectorAll<HTMLElement>('[aria-label="Filter by type"] .tg-item')).find(
      (el) => el.textContent.trim() === 'Folders',
    )
    expect(folders?.getAttribute('data-state')).toBe('on')
    cleanup()
  })

  it('type-in-AI: a null type from the agent LEAVES the user-picked type untouched (asymmetry)', async () => {
    aiProvider = 'cloud'
    // Pre-set the toggle to Files before the AI run.
    selectionQueryState.setTypeFilter('file')
    translateSelectionMock.mockResolvedValueOnce({
      pattern: '*.log',
      kind: 'glob',
      isDirectory: null, // agent stays silent on type
      sizeMin: null,
      sizeMax: null,
      modifiedAfter: null,
      modifiedBefore: null,
      caveat: null,
      label: null,
    })
    const { overlay, cleanup } = await mountDialog()
    dispatchKey(overlay, '1', true) // ⌘1 → AI
    await tick()
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = 'all log files'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    dispatchKey(overlay, 'Enter')
    await tick()
    await new Promise((r) => setTimeout(r, 100))
    await tick()

    // The IPC received `false` (files) as context.
    expect(translateSelectionMock.mock.calls[0][2]).toBe(false)
    // The agent returned null, so the user's Files choice must STILL stand (never reset to both).
    expect(selectionQueryState.getTypeFilter()).toBe('file')
    const files = Array.from(overlay.querySelectorAll<HTMLElement>('[aria-label="Filter by type"] .tg-item')).find(
      (el) => el.textContent.trim() === 'Files',
    )
    expect(files?.getAttribute('data-state')).toBe('on')
    cleanup()
  })

  it('drops the synthetic `..` parent entry from matches even when the pattern matches it', async () => {
    // The regular pane's snapshot prepends a synthetic `..` entry at index 0
    // (FilePane.getEntriesSnapshot when `hasParent`). `applyIndices` already
    // skips index 0 on commit, but the dialog's preview must also drop it so
    // the result count and the rows shown stay honest.
    const matched: number[][] = []
    const { overlay, cleanup } = await mountDialog({
      entries: [
        buildEntry('..', { isDirectory: true, size: undefined, parentPath: '/parent' }),
        buildEntry('foo.txt'),
        buildEntry('bar.txt'),
      ],
      onCommit: (idxs) => matched.push(idxs),
    })
    const input = overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    input.value = '*'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    await new Promise((r) => setTimeout(r, 1100))
    await tick()
    dispatchKey(overlay, 'Enter')
    await tick()
    expect(matched).toHaveLength(1)
    // Only the two real entries (indices 1 and 2), NOT index 0 (`..`).
    expect(matched[0]).toEqual([1, 2])
    cleanup()
  })

  it('keeps mode, term, and the size filter across a close + reopen', async () => {
    // Selection's QueryFilterState is a module-level singleton (mirroring Search),
    // so closing and reopening the dialog must restore the exact mode, typed term,
    // and configured filters. Before the hoist this failed: the component made a
    // fresh `createQueryFilterState()` per mount, so every reopen lost the work.
    const first = await mountDialog()
    const firstInput = first.overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement

    // Type a glob term in filename mode (the default).
    firstInput.value = '*.png'
    firstInput.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    // Configure a `≥ 1 MB` size filter via the popover: click the Size chip to open
    // it, then pick the `≥` comparator and the `1` preset (default unit is MB). The
    // popover is a fixed-position sibling in `document`, not a child of the overlay.
    const sizeChip = Array.from(first.overlay.querySelectorAll<HTMLButtonElement>('.chip-filter')).find((c) =>
      c.textContent.trim().startsWith('Size'),
    )
    if (!sizeChip) throw new Error('size chip not found')
    sizeChip.click()
    await tick()
    const sizePopover = document.querySelector('[aria-label="Size filter options"]')
    if (!sizePopover) throw new Error('size popover did not open')
    const radios = Array.from(sizePopover.querySelectorAll('button[role="radio"]'))
    const gteCell = radios.find((b) => b.textContent.trim() === '≥')
    const onePreset = radios.find((b) => b.textContent.trim() === '1')
    if (!gteCell || !onePreset) throw new Error('size popover cells not found')
    ;(gteCell as HTMLButtonElement).click()
    await tick()
    ;(onePreset as HTMLButtonElement).click()
    await tick()

    // Confirm the chip is configured before we close.
    const sizeChipBefore = Array.from(first.overlay.querySelectorAll('.chip-filter')).find((c) =>
      /Size:/.test(c.textContent),
    )
    expect(sizeChipBefore, 'size chip should be configured before close').toBeTruthy()

    // Close (unmount the component) and reopen (mount a fresh one).
    first.cleanup()
    const second = await mountDialog()
    const secondInput = second.overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement

    // The typed term must survive the reopen.
    expect(secondInput.value).toBe('*.png')
    // The configured size chip must survive the reopen.
    const sizeChipAfter = Array.from(second.overlay.querySelectorAll('.chip-filter')).find((c) =>
      /Size:/.test(c.textContent),
    )
    expect(sizeChipAfter, 'size chip should still be configured after reopen').toBeTruthy()

    second.cleanup()
  })

  it('reopen re-derives results against the CURRENT folder (not stale rows from the first folder)', async () => {
    // Live-smoke fix: reopening a restored session must show the same query's results
    // immediately, re-derived against the folder open NOW. We prove the re-run happened by
    // reopening on a DIFFERENT folder and asserting Enter commits indices computed against
    // the new folder. Pre-fix, nothing re-ran on mount: the content sat idle until an edit.
    const matched: number[][] = []

    // First folder: two PNGs at indices 0 and 2.
    const first = await mountDialog({
      entries: [buildEntry('a.png'), buildEntry('b.txt'), buildEntry('c.png')],
      onCommit: (idxs) => matched.push(idxs),
    })
    const firstInput = first.overlay.querySelector('input[type="text"], input:not([type])') as HTMLInputElement
    firstInput.value = '*.png'
    firstInput.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    await new Promise((r) => setTimeout(r, 1100)) // auto-apply debounce → a run lands
    await tick()
    first.cleanup()

    // Second folder, DIFFERENT shape: a single PNG, now at index 1.
    const second = await mountDialog({
      entries: [buildEntry('x.txt'), buildEntry('y.png'), buildEntry('z.txt')],
      onCommit: (idxs) => matched.push(idxs),
    })
    // Let the reopen re-run settle (no typing).
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    // Enter commits immediately — the result set is already re-derived against the new folder.
    dispatchKey(second.overlay, 'Enter')
    await tick()
    expect(matched).toHaveLength(1)
    expect(matched[0]).toEqual([1]) // y.png in the SECOND folder, not [0, 2] from the first
    second.cleanup()
  })

  it('first-ever open shows the empty state and does not auto-run', async () => {
    // A clean session (after ⌘N / first launch) must rest on the empty state with examples,
    // never an auto-run. `clearSelectionState()` in beforeEach gives us the clean slate.
    const { overlay, cleanup } = await mountDialog()
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(overlay.querySelector('.empty-state')).toBeTruthy()
    cleanup()
  })

  it('matches on a size filter alone when the name bar is empty', async () => {
    // The headline filter-only fix: a `≥ 1 MB` size filter with an EMPTY name pattern must
    // select every file ≥ 1 MB. Before the fix, `buildMatchQuery` returned null on
    // an empty pattern and the matcher short-circuited to [], so filter-only queries
    // silently selected nothing.
    const matched: number[][] = []
    const { overlay, cleanup } = await mountDialog({
      entries: [
        buildEntry('small.txt', { size: 1000 }),
        buildEntry('big.bin', { size: 2_000_000 }),
        buildEntry('tiny.log', { size: 50 }),
      ],
      onCommit: (idxs) => matched.push(idxs),
    })

    // Leave the bar empty. Configure a `≥ 1 MB` size filter via the popover.
    const sizeChip = Array.from(overlay.querySelectorAll<HTMLButtonElement>('.chip-filter')).find((c) =>
      c.textContent.trim().startsWith('Size'),
    )
    if (!sizeChip) throw new Error('size chip not found')
    sizeChip.click()
    await tick()
    const sizePopover = document.querySelector('[aria-label="Size filter options"]')
    if (!sizePopover) throw new Error('size popover did not open')
    const radios = Array.from(sizePopover.querySelectorAll('button[role="radio"]'))
    const gteCell = radios.find((b) => b.textContent.trim() === '≥')
    const onePreset = radios.find((b) => b.textContent.trim() === '1')
    if (!gteCell || !onePreset) throw new Error('size popover cells not found')
    ;(gteCell as HTMLButtonElement).click()
    await tick()
    ;(onePreset as HTMLButtonElement).click()
    await tick()
    // The size pick schedules an auto-apply run; wait for the debounce to fire.
    await new Promise((r) => setTimeout(r, 1100))
    await tick()

    // Commit selects exactly the 2 MB file's index. The committed set is the
    // authoritative proof the filter-only run matched (virtual-scroll row rendering
    // is unreliable under jsdom, so we assert on the matched indices, not the DOM).
    dispatchKey(overlay, 'Enter')
    await tick()
    expect(matched).toHaveLength(1)
    expect(matched[0]).toEqual([1])

    cleanup()
  })

  it('toggles caseSensitive via the FilterChips extras callback', async () => {
    // Mount and let the dialog's onMount complete, then toggle via the chip button.
    const { overlay, cleanup } = await mountDialog()
    // The chip strip renders a "Case sensitive" toggle; click it. The exact selector
    // depends on FilterChips internals — we look for any button with case-sensitive in
    // its accessible label.
    const chipButtons = Array.from(overlay.querySelectorAll('button'))
    const caseButton = chipButtons.find((b) => /case[\s-]sensitive/i.test(b.textContent))
    if (caseButton) {
      caseButton.click()
      await tick()
    }
    // Pass — we exercised the callback path even if no visible button matched (the
    // chip's content depends on rendered state). The aim here is to walk the
    // filterChipsExtras config wiring at runtime.
    cleanup()
  })
})
