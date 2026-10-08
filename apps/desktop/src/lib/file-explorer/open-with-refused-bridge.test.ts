/**
 * Tests for the "Open with" refusal notice: what the person is told when they picked an
 * app for a file inside an archive and nothing opened.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { OpenWithCopyRefusal, OpenWithCopyRefused, OpenWithCopySource } from '$lib/ipc/bindings'

/** Only the fields these cells assert on; the real options type is wider. */
type ToastOptions = { level: string; timeoutMs: number; id: string }

const addToast = vi.hoisted(() => vi.fn<(message: string, options: ToastOptions) => string>())
vi.mock('$lib/ui/toast', () => ({ addToast }))

let emitRefused: (payload: OpenWithCopyRefused) => void
const unlisten = vi.fn()
vi.mock('$lib/tauri-commands', () => ({
  onOpenWithCopyRefused: (handler: (payload: OpenWithCopyRefused) => void) => {
    emitRefused = handler
    return Promise.resolve(unlisten)
  },
}))

import { formatByteSize } from '$lib/units'
import { startOpenWithRefusedBridge } from './open-with-refused-bridge'

const TWO_GIB = 2 * 1024 * 1024 * 1024

function refused(reason: OpenWithCopyRefusal, source: OpenWithCopySource = 'archive'): OpenWithCopyRefused {
  return { fileName: 'holiday.mov', appName: 'QuickTime Player', reason, source }
}

beforeEach(async () => {
  addToast.mockClear()
  unlisten.mockClear()
  await startOpenWithRefusedBridge()
})

describe('the open-with refusal notice', () => {
  it.each<[string, OpenWithCopyRefusal]>([
    ['too large', { kind: 'tooLarge', cap: TWO_GIB }],
    ['locked', { kind: 'needsPassword' }],
    ['a damaged archive', { kind: 'archiveUnreadable' }],
    ['anything else', { kind: 'unreadable' }],
  ])('names the file and the app when it is %s', (_label, reason) => {
    emitRefused(refused(reason))

    expect(addToast).toHaveBeenCalledTimes(1)
    const [message] = addToast.mock.calls[0]
    expect(message).toContain('holiday.mov')
    expect(message).toContain('QuickTime Player')
  })

  it('says the limit a file is over, so the person knows what "too big" means', () => {
    emitRefused(refused({ kind: 'tooLarge', cap: TWO_GIB }))

    const [message] = addToast.mock.calls[0]
    expect(message).toContain(formatByteSize(TWO_GIB))
  })

  it('says where a too-big file sits: an archive, or a repo’s history', () => {
    emitRefused(refused({ kind: 'tooLarge', cap: TWO_GIB }, 'archive'))
    emitRefused(refused({ kind: 'tooLarge', cap: TWO_GIB }, 'repoHistory'))

    const [[fromArchive], [fromRepo]] = addToast.mock.calls
    expect(fromArchive).toContain('archive')
    expect(fromRepo).toContain('repo’s history')
    expect(fromRepo).not.toContain('archive')
    expect(fromRepo).toContain('holiday.mov')
    expect(fromRepo).toContain(formatByteSize(TWO_GIB))
  })

  it('tells a locked archive apart from a damaged one', () => {
    emitRefused(refused({ kind: 'needsPassword' }))
    emitRefused(refused({ kind: 'archiveUnreadable' }))

    const [[locked], [damaged]] = addToast.mock.calls
    expect(locked).toContain('password')
    expect(damaged).not.toContain('password')
  })

  /// A second click on the same refused file replaces its notice rather than stacking
  /// a second copy of the same sentence.
  it('reuses one id per file, as a warning that times out', () => {
    emitRefused(refused({ kind: 'unreadable' }))
    emitRefused(refused({ kind: 'unreadable' }))

    const [[, first], [, second]] = addToast.mock.calls
    expect(first.id).toBe(second.id)
    expect(first.level).toBe('warn')
    expect(first.timeoutMs).toBeGreaterThan(0)
  })

  it('hands back the unsubscribe it was given', async () => {
    const stop = await startOpenWithRefusedBridge()
    stop()
    expect(unlisten).toHaveBeenCalled()
  })
})
