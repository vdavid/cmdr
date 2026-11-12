/**
 * Tests for the kept-leftovers notice bridge: what the person is told when a
 * drive comes back holding the working folder of a move that never finished.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { LeftoversKeptEvent } from '#lib/ipc/bindings.js'

/** Only the fields these cells assert on; the real options type is wider. */
type ToastOptions = { level: string; timeoutMs: number; id: string }

const addToast = vi.hoisted(() => vi.fn<(message: string, options: ToastOptions) => string>())
vi.mock('#lib/ui/toast/index.js', () => ({ addToast }))

let emit: (payload: LeftoversKeptEvent) => void
const unlisten = vi.fn()
vi.mock('#lib/tauri-commands/index.js', () => ({
  onLeftoversKept: (handler: (payload: LeftoversKeptEvent) => void) => {
    emit = handler
    return Promise.resolve(unlisten)
  },
}))

import { startLeftoverNoticeBridge } from './leftover-notice-bridge'

const FOLDER = '.cmdr-staging-0199a2c4-7b31-7c2a-9f10-4e1d8c6b2a55'

function folderKept(folderName: string): LeftoversKeptEvent {
  return { volumeName: 'Tëstkamera', kept: { kind: 'stagingFolder', folderName } }
}

beforeEach(async () => {
  addToast.mockClear()
  unlisten.mockClear()
  await startLeftoverNoticeBridge()
})

describe('the kept-leftovers notice', () => {
  it('names the drive and the folder, so the files have somewhere findable', () => {
    emit(folderKept(FOLDER))

    expect(addToast).toHaveBeenCalledTimes(1)
    const [message] = addToast.mock.calls[0]
    expect(message).toContain('Tëstkamera')
    expect(message).toContain(FOLDER)
  })

  /// Nothing is asked of the person and nothing of theirs is at risk, so it
  /// reads as information and goes away on its own.
  it('is an info toast that times out', () => {
    emit(folderKept(FOLDER))

    const [, options] = addToast.mock.calls[0]
    expect(options.level).toBe('info')
    expect(options.timeoutMs).toBeGreaterThan(0)
  })

  it('keys per folder, so two folders on one drive each get their own notice', () => {
    const second = '.cmdr-staging-0199a2c4-7b31-7c2a-9f10-4e1d8c6b2a56'
    emit(folderKept(FOLDER))
    emit(folderKept(second))

    const ids = addToast.mock.calls.map(([, options]) => options.id)
    expect(new Set(ids).size).toBe(2)
  })

  /// A re-plug meets the same folder again. One notice replacing itself is the
  /// honest reading; two stacked copies of the same sentence is not.
  it('reuses one id for the same folder, so a re-plug replaces its notice', () => {
    emit(folderKept(FOLDER))
    emit(folderKept(FOLDER))

    const ids = addToast.mock.calls.map(([, options]) => options.id)
    expect(ids[0]).toBe(ids[1])
  })

  it('hands back the unsubscribe it was given', async () => {
    const stop = await startLeftoverNoticeBridge()
    stop()
    expect(unlisten).toHaveBeenCalled()
  })
})

describe('the kept unfinished landing notice', () => {
  const landing = (keptName: string): LeftoversKeptEvent => ({
    volumeName: 'Testnas',
    kept: { kind: 'unfinishedLanding', fileName: 'notes.txt', keptName },
  })

  it('names the drive, the file, and where it is kept', () => {
    emit(landing('notes (recovered).txt'))

    expect(addToast).toHaveBeenCalledTimes(1)
    const [message, options] = addToast.mock.calls[0]
    expect(message).toContain('Testnas')
    expect(message).toContain('notes.txt')
    expect(message).toContain('notes (recovered).txt')
    expect(options.level).toBe('info')
  })

  it('keys per kept file, so a re-plug replaces its notice', () => {
    emit(landing('notes (recovered).txt'))
    emit(landing('notes (recovered).txt'))
    emit(landing('notes (recovered 1).txt'))

    const ids = addToast.mock.calls.map(([, options]) => options.id)
    expect(ids[0]).toBe(ids[1])
    expect(ids[2]).not.toBe(ids[0])
  })
})
