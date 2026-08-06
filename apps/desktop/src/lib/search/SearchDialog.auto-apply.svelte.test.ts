/**
 * Auto-apply: what a keystroke does, and what it must not do.
 *
 * Pins:
 *   - One search per 1 s debounce, IME-gated, and only when `search.autoApply` is on
 *     (live-toggled from the settings window mid-session).
 *   - AI mode never auto-applies, whatever the setting says (a translate costs money).
 *   - The ⏎ run button runs the mode's own path, and the "Press Enter to search" hint
 *     shows exactly when a keystroke won't do it for you.
 *
 * Shared mount + IPC fixture: `test-search-dialog-harness.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'
import type { TranslateResult } from '$lib/ipc/bindings'
import { setQuery, setMode } from './search-state.svelte'
import {
  mountDialog,
  resetSearchDialogTest,
  searchFilesMock,
  setAutoApplyForTest,
  testSettings,
  translateSearchQueryMock,
  unmountAllDialogs,
} from './test-search-dialog-harness'

vi.mock('$lib/tauri-commands', async () => (await import('./test-search-dialog-harness')).tauriCommandsMock())
vi.mock('../../routes/viewer/media-view', async () => (await import('./test-search-dialog-harness')).mediaViewMock())
vi.mock('$lib/settings', async () => (await import('./test-search-dialog-harness')).settingsMock())
vi.mock('$lib/indexing', async () => (await import('./test-search-dialog-harness')).indexingMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-search-dialog-harness')).iconCacheMock())

afterEach(unmountAllDialogs)

describe('SearchDialog auto-apply', () => {
  beforeEach(async () => {
    await resetSearchDialogTest()
    searchFilesMock.mockClear()
    translateSearchQueryMock.mockClear()
  })

  it('fires exactly one search after the 1 s debounce when typing in filename mode', async () => {
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()

      // The dialog's `handleQueryInput` calls `setQuery` + `scheduleSearch`. We simulate a few
      // keystrokes back to back, each resetting the debounce timer.
      const input = document.body.querySelector('.query-bar input.text-field-control') as HTMLInputElement
      input.value = 'p'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.value = 'ph'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.value = 'pho'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      // 200 ms (the old debounce) is not enough; 1000 ms is.
      vi.advanceTimersByTime(200)
      expect(searchFilesMock).not.toHaveBeenCalled()
      vi.advanceTimersByTime(900)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(1)
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not auto-apply when search.autoApply is off', async () => {
    testSettings.autoApply = false
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()
      const input = document.body.querySelector('.query-bar input.text-field-control') as HTMLInputElement
      input.value = '*.pdf'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      // Even far past the debounce window, nothing fires automatically.
      vi.advanceTimersByTime(5_000)
      await Promise.resolve()
      expect(searchFilesMock).not.toHaveBeenCalled()
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('live-applies a setting toggle from on to off and back', async () => {
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()
      const input = document.body.querySelector('.query-bar input.text-field-control') as HTMLInputElement

      // 1) Auto-apply on: type, advance 1 s, search fires.
      input.value = '*.pdf'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      vi.advanceTimersByTime(1_000)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(1)

      // 2) Toggle the setting off. Subsequent typing must not auto-apply.
      setAutoApplyForTest(false)
      input.value = '*.txt'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      vi.advanceTimersByTime(5_000)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(1)

      // 3) Toggle the setting back on. The next keystroke does auto-apply.
      setAutoApplyForTest(true)
      input.value = '*.txt!'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      vi.advanceTimersByTime(1_000)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(2)
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not auto-apply in AI mode regardless of the setting', async () => {
    testSettings.aiProvider = 'cloud'
    testSettings.autoApply = true
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()
      translateSearchQueryMock.mockClear()
      setMode('ai')

      const input = document.body.querySelector('.query-bar input.text-field-control') as HTMLInputElement
      input.value = 'big screenshots'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      vi.advanceTimersByTime(5_000)
      await Promise.resolve()
      expect(translateSearchQueryMock).not.toHaveBeenCalled()
      expect(searchFilesMock).not.toHaveBeenCalled()
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('suppresses auto-apply during IME composition and fires exactly once on compositionend', async () => {
    const { cleanup } = await mountDialog()
    vi.useFakeTimers()
    try {
      searchFilesMock.mockClear()
      const input = document.body.querySelector('.query-bar input.text-field-control') as HTMLInputElement

      // Start a composition. Each `input` during composition is one keystroke; we mustn't fire.
      input.dispatchEvent(new CompositionEvent('compositionstart'))
      input.value = 'ｐ'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.value = 'ｐｈ'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      vi.advanceTimersByTime(2_000)
      await Promise.resolve()
      expect(searchFilesMock).not.toHaveBeenCalled()

      // End composition: the parent resets the debounce and we should get exactly one fire after
      // SEARCH_AUTO_APPLY_DEBOUNCE_MS.
      input.dispatchEvent(new CompositionEvent('compositionend'))
      vi.advanceTimersByTime(999)
      expect(searchFilesMock).not.toHaveBeenCalled()
      vi.advanceTimersByTime(1)
      await Promise.resolve()
      expect(searchFilesMock).toHaveBeenCalledTimes(1)
      cleanup()
    } finally {
      vi.useRealTimers()
    }
  })

  it('clicking the ⏎ run button triggers a search in filename mode', async () => {
    testSettings.autoApply = false
    const { cleanup } = await mountDialog()
    searchFilesMock.mockClear()
    setQuery('*.pdf')
    await tick()

    const runButton = document.body.querySelector('.query-bar button.btn') as HTMLButtonElement
    expect(runButton).not.toBeNull()
    runButton.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(searchFilesMock).toHaveBeenCalledTimes(1)
    cleanup()
  })

  it('clicking the ⏎ run button triggers an AI search in AI mode', async () => {
    testSettings.aiProvider = 'cloud'
    testSettings.autoApply = true
    translateSearchQueryMock.mockResolvedValueOnce({
      display: {},
      query: {},
    } as TranslateResult)
    const { cleanup } = await mountDialog()
    translateSearchQueryMock.mockClear()
    setMode('ai')
    setQuery('large screenshots')
    await tick()

    const runButton = document.body.querySelector('.query-bar button.btn') as HTMLButtonElement
    runButton.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    // Second arg is the current type filter as context (both → null at the start).
    expect(translateSearchQueryMock).toHaveBeenCalledWith('large screenshots', null)
    cleanup()
  })

  it('shows the "Press Enter to search" hint when auto-apply is off and the query changed', async () => {
    testSettings.autoApply = false
    const { cleanup } = await mountDialog()
    setQuery('photos')
    await tick()

    const hint = document.body.querySelector('.run-hint')
    expect(hint).not.toBeNull()
    expect(hint?.textContent).toMatch(/Press Enter to search/i)
    cleanup()
  })

  it('shows the hint in AI mode (even with auto-apply on) when the query is unsent', async () => {
    testSettings.aiProvider = 'cloud'
    testSettings.autoApply = true
    const { cleanup } = await mountDialog()
    setMode('ai')
    setQuery('big files this week')
    await tick()

    const hint = document.body.querySelector('.run-hint')
    expect(hint).not.toBeNull()
    cleanup()
  })

  it('hides the hint when auto-apply is on and mode is filename/regex', async () => {
    testSettings.autoApply = true
    const { cleanup } = await mountDialog()
    setMode('filename')
    setQuery('*.pdf')
    await tick()

    expect(document.body.querySelector('.run-hint')).toBeNull()
    cleanup()
  })
})
