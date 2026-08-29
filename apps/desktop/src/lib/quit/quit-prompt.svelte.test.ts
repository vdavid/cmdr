/**
 * The main window's mirror of a held quit: it opens on the backend's event,
 * counts down for display, and answers exactly once.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
// `vi.mock` is hoisted above this, so the subject imports the stubbed IPC layer.
import { quitPrompt, initQuitPrompt, cleanupQuitPrompt } from './quit-prompt.svelte'
import type { OperationSnapshot, QuitRequested } from '$lib/ipc/bindings'

let requested: ((event: QuitRequested) => void) | null = null
let calledOff: (() => void) | null = null
const quitConfirm = vi.fn(() => Promise.resolve('answered'))
const quitCancel = vi.fn(() => Promise.resolve('answered'))

vi.mock('$lib/tauri-commands', () => ({
  onQuitRequested: vi.fn((cb: (event: QuitRequested) => void) => {
    requested = cb
    return Promise.resolve(() => {
      requested = null
    })
  }),
  onQuitCalledOff: vi.fn((cb: () => void) => {
    calledOff = cb
    return Promise.resolve(() => {
      calledOff = null
    })
  }),
  quitConfirm: () => quitConfirm(),
  quitCancel: () => quitCancel(),
}))

function operation(): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'running',
    source: 'Holiday.mov',
    destination: 'Backup',
    supportsRollback: true,
    reverses: null,
    error: null,
  }
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(new Date('2026-08-10T12:00:00Z'))
  quitConfirm.mockClear()
  quitCancel.mockClear()
  initQuitPrompt()
})

afterEach(() => {
  quitPrompt.keepWorking()
  cleanupQuitPrompt()
  vi.useRealTimers()
})

describe('the quit prompt', () => {
  it('stays closed until the backend holds a quit', () => {
    expect(quitPrompt.open).toBe(false)
  })

  it('opens with the operations and the deadline the backend set', () => {
    requested?.({ operations: [operation()], countdownMs: 15_000 })
    expect(quitPrompt.open).toBe(true)
    expect(quitPrompt.operations).toHaveLength(1)
    expect(quitPrompt.secondsLeft).toBe(15)
  })

  it('counts down against the wall clock, so a stalled webview never shows a stale number', () => {
    requested?.({ operations: [operation()], countdownMs: 15_000 })

    vi.advanceTimersByTime(1_000)
    expect(quitPrompt.secondsLeft).toBe(14)

    // A frozen webview misses ticks; the next one it does run must land on the
    // honest number, not on "one less than last time".
    vi.advanceTimersByTime(9_000)
    expect(quitPrompt.secondsLeft).toBe(5)
  })

  it('never counts past zero', () => {
    requested?.({ operations: [operation()], countdownMs: 2_000 })
    vi.advanceTimersByTime(30_000)
    expect(quitPrompt.secondsLeft).toBe(0)
  })

  it('"keep working" closes the prompt and tells the backend to drop the countdown', () => {
    requested?.({ operations: [operation()], countdownMs: 15_000 })
    quitPrompt.keepWorking()

    expect(quitPrompt.open).toBe(false)
    expect(quitCancel).toHaveBeenCalledOnce()
    expect(quitConfirm).not.toHaveBeenCalled()

    // And the ticker is gone with it, not merely hidden: the number stops where
    // it stood instead of running on behind a closed prompt.
    vi.advanceTimersByTime(30_000)
    expect(quitPrompt.secondsLeft).toBe(15)
  })

  it('confirming asks the backend to quit and leaves the prompt up', () => {
    requested?.({ operations: [operation()], countdownMs: 15_000 })
    quitPrompt.confirm()

    expect(quitConfirm).toHaveBeenCalledOnce()
    expect(quitCancel).not.toHaveBeenCalled()
    // The app is about to disappear; hiding the dialog first would flash the
    // panes on the way out.
    expect(quitPrompt.open).toBe(true)
  })

  it('a quit called off elsewhere takes the prompt down without answering again', () => {
    // An agent can answer the confirmation over MCP, and the gate answers there.
    // Nothing else closes this prompt, and one left counting toward a quit that
    // will never come is a lie.
    requested?.({ operations: [operation()], countdownMs: 15_000 })
    calledOff?.()

    expect(quitPrompt.open).toBe(false)
    expect(quitPrompt.operations).toHaveLength(0)
    // The gate is already released; answering it again would be noise.
    expect(quitCancel).not.toHaveBeenCalled()
    expect(quitConfirm).not.toHaveBeenCalled()

    vi.advanceTimersByTime(30_000)
    expect(quitPrompt.secondsLeft).toBe(15)
  })

  it('a second hold re-arms the countdown from the new deadline', () => {
    requested?.({ operations: [operation()], countdownMs: 15_000 })
    quitPrompt.keepWorking()
    requested?.({ operations: [operation(), operation()], countdownMs: 15_000 })

    expect(quitPrompt.open).toBe(true)
    expect(quitPrompt.operations).toHaveLength(2)
    expect(quitPrompt.secondsLeft).toBe(15)
  })
})
