/**
 * Headless tests for the progress dialog's DONE state (#484): with "Keep the
 * progress window open when a file operation finishes" on, a completed
 * operation stays on screen until the person closes it, and the parent still
 * hears about the completion right away (pane refresh, selection) rather than
 * on close.
 *
 * Shared mocks and lifecycle: `test-transfer-progress-harness.svelte.ts`.
 */

import { describe, it, expect, vi } from 'vitest'
import { flushSync } from 'svelte'

vi.mock('#lib/tauri-commands/index.js', async () =>
  (await import('./test-transfer-progress-harness.svelte')).tauriCommandsMock(),
)
vi.mock('#lib/file-operations/queue/queue-window.js', async () =>
  (await import('./test-transfer-progress-harness.svelte')).queueWindowMock(),
)
vi.mock('#lib/ui/toast/index.js', async () => (await import('./test-transfer-progress-harness.svelte')).toastMock())
vi.mock('#lib/settings/index.js', async () => (await import('./test-transfer-progress-harness.svelte')).settingsMock())
vi.mock('#lib/intl/messages.svelte.js', async () =>
  (await import('./test-transfer-progress-harness.svelte')).messagesMock(),
)
vi.mock('../progress-readout', async (importOriginal) =>
  (await import('./test-transfer-progress-harness.svelte')).progressReadoutMock(
    await importOriginal<typeof import('../progress-readout')>(),
  ),
)
vi.mock('#lib/logging/logger.js', async () => (await import('./test-transfer-progress-harness.svelte')).loggerMock())

import { createTransferProgressState } from './transfer-progress-state.svelte'
import { listeners, snapshot, useTransferProgressState } from './test-transfer-progress-harness.svelte'

const { startedState } = useTransferProgressState(createTransferProgressState)

const PAYLOAD = {
  filesProcessed: 5,
  filesSkipped: 1,
  bytesProcessed: 999,
  appearedDuringMove: null,
  topLevelSkipped: null,
  refused: null,
  metadataLeftBehind: null,
  lockedKept: null,
  durabilityUnconfirmed: null,
}

function complete(): void {
  if (!listeners.complete) throw new Error('complete subscriber never registered')
  listeners.complete({
    operationId: 'op-1',
    operationType: 'copy',
    filesProcessed: 5,
    filesSkipped: 1,
    bytesProcessed: 999,
  })
  flushSync()
}

describe('createTransferProgressState: the done state', () => {
  it('without the setting, closes as before and says the dialog went away', async () => {
    const { state, config } = await startedState({ keepOpenWhenDone: () => false })
    complete()
    vi.advanceTimersByTime(450)

    expect(state.done).toBeNull()
    expect(config.onComplete).toHaveBeenCalledWith(PAYLOAD, { keptOpen: false })
  })

  it('with the setting, stays open and reports the completion at once', async () => {
    const onDoneClosed = vi.fn()
    const { state, config } = await startedState({ keepOpenWhenDone: () => true, onDoneClosed })
    complete()

    // No anti-flicker floor to wait out: the dialog isn't going anywhere, and
    // the panes should refresh now, not when the person gets round to closing.
    expect(config.onComplete).toHaveBeenCalledTimes(1)
    expect(config.onComplete).toHaveBeenCalledWith(PAYLOAD, { keptOpen: true })
    expect(state.done?.event.filesProcessed).toBe(5)
    vi.advanceTimersByTime(5_000)
    expect(onDoneClosed).not.toHaveBeenCalled()
    expect(config.onComplete).toHaveBeenCalledTimes(1)
  })

  it('closes on the person’s say-so, once', async () => {
    const onDoneClosed = vi.fn()
    const { state } = await startedState({ keepOpenWhenDone: () => true, onDoneClosed })
    complete()

    state.closeDone()
    state.closeDone()

    expect(onDoneClosed).toHaveBeenCalledTimes(1)
  })

  it('treats the × (a detach) as Close once the operation is done', async () => {
    const onDoneClosed = vi.fn()
    const { state, config } = await startedState({ keepOpenWhenDone: () => true, onDoneClosed })
    complete()

    state.detach()

    expect(onDoneClosed).toHaveBeenCalledTimes(1)
    expect(config.onCancelled).not.toHaveBeenCalled()
    expect(config.onQueue).not.toHaveBeenCalled()
  })

  it('times the operation from when the backend says it started', async () => {
    vi.setSystemTime(100_000)
    const { state } = await startedState({ keepOpenWhenDone: () => true, onDoneClosed: vi.fn() })
    if (!listeners.opsChanged) throw new Error('operations-changed subscriber never registered')
    listeners.opsChanged({ operations: [{ ...snapshot('op-1', 'running'), startedAt: 40_000 }] })
    complete()

    expect(state.done?.elapsedMs).toBe(60_000)
  })

  it('has no time to report when the backend never said when it started', async () => {
    const { state } = await startedState({ keepOpenWhenDone: () => true, onDoneClosed: vi.fn() })
    complete()

    expect(state.done?.elapsedMs).toBeNull()
  })
})
