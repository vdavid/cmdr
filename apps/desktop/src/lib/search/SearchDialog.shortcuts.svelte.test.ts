/**
 * The dialog's keyboard contract, from Search's side.
 *
 * Pins:
 *   - `⌘1` / `⌘2` / `⌘3` switch modes, and the numbering shifts when AI is off.
 *   - Switching mode swaps the bar to that mode's hand-typed buffer.
 *   - `⌘⏎` and `⇧⏎` are no-ops; bare Enter is the only key that runs a search.
 *   - `⌥←` / `⌥→` stay the text field's native move-by-word.
 *
 * Shared mount + IPC fixture: `test-search-dialog-harness.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'
import { getQuery, setQuery, getMode, setMode, setCursorIndex } from './search-state.svelte'
import {
  dispatchKey,
  mountDialog,
  resetSearchDialogTest,
  searchFilesMock,
  translateSearchQueryMock,
  unmountAllDialogs,
} from './test-search-dialog-harness'

vi.mock('$lib/tauri-commands', async () => (await import('./test-search-dialog-harness')).tauriCommandsMock())
vi.mock('../../routes/viewer/media-view', async () => (await import('./test-search-dialog-harness')).mediaViewMock())
vi.mock('$lib/settings', async () => (await import('./test-search-dialog-harness')).settingsMock())
vi.mock('$lib/indexing', async () => (await import('./test-search-dialog-harness')).indexingMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-search-dialog-harness')).iconCacheMock())

afterEach(unmountAllDialogs)

describe('SearchDialog mode shortcuts (AI on)', () => {
  beforeEach(async () => {
    await resetSearchDialogTest({ aiProvider: 'cloud' })
    translateSearchQueryMock.mockClear()
  })

  it('⌘1 switches to AI mode', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    dispatchKey(overlay, '1', true)
    await tick()
    expect(getMode()).toBe('ai')
    cleanup()
  })

  it('⌘2 switches to filename mode', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('ai')
    dispatchKey(overlay, '2', true)
    await tick()
    expect(getMode()).toBe('filename')
    cleanup()
  })

  it('⌘3 switches to regex mode', async () => {
    const { overlay, cleanup } = await mountDialog()
    dispatchKey(overlay, '3', true)
    await tick()
    expect(getMode()).toBe('regex')
    cleanup()
  })

  it("switching mode swaps the bar to the target mode's hand-typed buffer (carrying into an empty target)", async () => {
    // Each mode owns its own input buffer. Switching from AI to filename restores filename's
    // last hand-typed value; when that buffer is empty, the outgoing term carries across so
    // the user's words don't vanish (term carry-over). The AI prompt stays available via
    // `getLastAiPrompt()` for the transparency strip regardless.
    const { overlay, cleanup } = await mountDialog()
    setMode('ai')
    setQuery('big files')
    dispatchKey(overlay, '2', true)
    await tick()
    expect(getMode()).toBe('filename')
    // Filename's buffer was empty, so the outgoing 'big files' carries into the bar.
    expect(getQuery()).toBe('big files')
    cleanup()
  })

  // R4: ⌘⏎ and ⇧⏎ are no-ops in the search dialog. Bare Enter is the only path
  // that runs a search or opens the cursor row. The earlier "⌘Enter triggers AI"
  // shortcut is gone per David's request.
  it('R4: ⌘Enter is a no-op (does not run AI even when AI is enabled)', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    setQuery('large screenshots')
    dispatchKey(overlay, 'Enter', true)
    await tick()
    expect(translateSearchQueryMock).not.toHaveBeenCalled()
    expect(searchFilesMock).not.toHaveBeenCalled()
    cleanup()
  })

  it('R4: ⇧Enter is a no-op (does not run a search)', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    setQuery('foo')
    dispatchKey(overlay, 'Enter', false, true)
    await tick()
    expect(searchFilesMock).not.toHaveBeenCalled()
    cleanup()
  })

  it('R4: bare Enter still runs the search', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    setQuery('foo')
    dispatchKey(overlay, 'Enter')
    // Enter takes the live path, which installs its event listeners before asking, so
    // the ask lands a few microtasks later than the one-shot path's did.
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    expect(searchFilesMock).toHaveBeenCalled()
    cleanup()
  })
})

describe('SearchDialog mode shortcuts (AI off)', () => {
  beforeEach(async () => {
    await resetSearchDialogTest()
    translateSearchQueryMock.mockClear()
  })

  it('⌘1 switches to filename when AI is off', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('regex')
    dispatchKey(overlay, '1', true)
    await tick()
    expect(getMode()).toBe('filename')
    cleanup()
  })

  it('⌘2 switches to regex when AI is off', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    dispatchKey(overlay, '2', true)
    await tick()
    expect(getMode()).toBe('regex')
    cleanup()
  })

  it('⌘3 is a no-op when AI is off', async () => {
    const { overlay, cleanup } = await mountDialog()
    setMode('filename')
    dispatchKey(overlay, '3', true)
    await tick()
    // mode stayed put
    expect(getMode()).toBe('filename')
    cleanup()
  })

  it('⌘Enter does not call AI when AI is off', async () => {
    const { overlay, cleanup } = await mountDialog()
    setQuery('whatever')
    dispatchKey(overlay, 'Enter', true)
    await tick()
    expect(translateSearchQueryMock).not.toHaveBeenCalled()
    cleanup()
  })
})

describe('SearchDialog ⌥← / ⌥→ pass through to the text field', () => {
  beforeEach(async () => {
    await resetSearchDialogTest()
    searchFilesMock.mockReset()
  })

  function dispatchAltKey(target: Element, key: string): KeyboardEvent {
    const event = new KeyboardEvent('keydown', {
      key,
      altKey: true,
      bubbles: true,
      cancelable: true,
    })
    target.dispatchEvent(event)
    return event
  }

  async function seedResultsAndMount(): Promise<{ overlay: Element; navigated: string[]; cleanup: () => void }> {
    // searchFilesMock's inferred resolved type is `{ entries: never[]; totalCount: number }`
    // (since the default mock returns an empty array literal). Cast to the broader shape
    // expected at runtime so the seeded row's fields type-check.
    searchFilesMock.mockResolvedValueOnce({
      entries: [
        {
          name: 'photo.jpg',
          path: '/Users/test/pictures/photo.jpg',
          parentPath: '/Users/test/pictures',
          isDirectory: false,
          size: 1000,
          modifiedAt: 1_700_000_000,
          iconId: 'ext:jpg',
        },
      ],
      totalCount: 1,
    })

    const navigated: string[] = []
    const { overlay, cleanup } = await mountDialog({
      onNavigate: (path: string) => {
        navigated.push(path)
      },
    })

    // Drive a search to populate results + set cursor to row 0.
    setQuery('photo*')
    setMode('filename')
    dispatchKey(overlay, 'Enter')
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    setCursorIndex(0)

    return { overlay, navigated, cleanup }
  }

  // ⌥← / ⌥→ are macOS's native move-by-word in a text field. The dialog must not
  // steal them: it leaves them unhandled so the focused query input gets them. Path
  // pills stay mouse-only (see query-ui/DETAILS.md § Path pills).
  it("⌥← doesn't navigate, so the focused text field keeps move-by-word", async () => {
    const { overlay, navigated, cleanup } = await seedResultsAndMount()
    const event = dispatchAltKey(overlay, 'ArrowLeft')
    await tick()
    expect(navigated).toEqual([])
    expect(event.defaultPrevented).toBe(false)
    cleanup()
  })

  it("⌥→ doesn't navigate, so the focused text field keeps move-by-word", async () => {
    const { overlay, navigated, cleanup } = await seedResultsAndMount()
    const event = dispatchAltKey(overlay, 'ArrowRight')
    await tick()
    expect(navigated).toEqual([])
    expect(event.defaultPrevented).toBe(false)
    cleanup()
  })
})
