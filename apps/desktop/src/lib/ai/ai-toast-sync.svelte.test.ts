/**
 * Integration test for `ai-toast-sync.svelte.ts`.
 *
 * Bug 2 (the "X-undone-by-effect" bug): the user clicks X on the downloading toast,
 * but the `$effect` re-runs and calls `addToast` again, so the toast pops back. The
 * fix tracks user dismissal in `aiState.downloadToastUserDismissed` and skips
 * `addToast` while the flag is set. The flag clears on the next download run so the
 * toast shows again, and other state transitions (offer, ready, etc.) ignore it.
 *
 * This file uses Svelte runes (`$effect.root`), so the filename has the `.svelte.`
 * infix that vite-plugin-svelte's compile-module looks for.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'

vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
}))

import { addToast, dismissToast } from '$lib/ui/toast'
import { initAiToastSync } from './ai-toast-sync.svelte'
import { getAiState, markDownloadToastDismissed, resetForTesting } from './ai-state.svelte'

describe('ai-toast-sync', () => {
  let dispose: (() => void) | undefined

  beforeEach(() => {
    vi.clearAllMocks()
    resetForTesting()
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  function startSync(): void {
    // `$effect` only works inside a reactive root. `$effect.root` gives us a
    // standalone scope we can dispose at the end of the test.
    dispose = $effect.root(() => {
      initAiToastSync()
    })
    flushSync()
  }

  it('adds the AI toast with closeTooltip and onDismiss when state is downloading', () => {
    const state = getAiState()
    startSync()
    expect(addToast).not.toHaveBeenCalled()

    state.notificationState = 'downloading'
    flushSync()

    expect(addToast).toHaveBeenCalledTimes(1)
    const options = vi.mocked(addToast).mock.calls[0][1]
    expect(options?.id).toBe('ai')
    expect(options?.dismissal).toBe('persistent')
    expect(options?.closeTooltip).toBe('Close this notification — the download will continue in the background')
    expect(typeof options?.onDismiss).toBe('function')
  })

  it('does not re-add the toast after the user dismisses (dismiss persists across re-runs)', () => {
    const state = getAiState()
    startSync()

    state.notificationState = 'downloading'
    flushSync()
    expect(addToast).toHaveBeenCalledTimes(1)

    // Simulate the user clicking X. The toast's `onDismiss` callback flips the flag,
    // which the effect reads, so the effect re-runs. The early return inside the
    // `'downloading'` branch is what keeps the toast gone.
    markDownloadToastDismissed()
    flushSync()

    // No new addToast call: the effect re-ran but skipped the add.
    expect(addToast).toHaveBeenCalledTimes(1)
  })

  it('shows the toast again on a fresh download run after dismissal', () => {
    const state = getAiState()
    startSync()

    state.notificationState = 'downloading'
    flushSync()
    markDownloadToastDismissed()
    flushSync()
    expect(addToast).toHaveBeenCalledTimes(1)

    // User goes back to offer, then starts a new download. `handleDownload` clears the
    // flag, then sets the state. Mirror that order here.
    state.notificationState = 'offer'
    flushSync()
    state.downloadToastUserDismissed = false
    state.notificationState = 'downloading'
    flushSync()

    // 1 from the first downloading, 1 from the offer transition, 1 from the new run.
    expect(addToast).toHaveBeenCalledTimes(3)
  })

  it('renders other transitions (ready) without the close-tooltip metadata', () => {
    const state = getAiState()
    startSync()

    state.notificationState = 'downloading'
    flushSync()
    markDownloadToastDismissed()
    flushSync()
    expect(addToast).toHaveBeenCalledTimes(1)

    state.notificationState = 'ready'
    flushSync()
    expect(addToast).toHaveBeenCalledTimes(2)
    const readyOptions = vi.mocked(addToast).mock.calls[1][1]
    expect(readyOptions?.closeTooltip).toBeUndefined()
    expect(readyOptions?.onDismiss).toBeUndefined()
  })

  it('dismisses the AI toast when state goes back to hidden', () => {
    const state = getAiState()
    startSync()

    state.notificationState = 'offer'
    flushSync()
    expect(addToast).toHaveBeenCalledTimes(1)

    state.notificationState = 'hidden'
    flushSync()
    expect(dismissToast).toHaveBeenCalledWith('ai')
  })
})
