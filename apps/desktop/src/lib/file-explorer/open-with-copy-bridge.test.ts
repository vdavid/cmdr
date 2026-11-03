/**
 * Tests for the "Open with" notices: what the person is told when they picked an app
 * for a file inside an archive, and either nothing opened or the app got a read-only copy,
 * and when the file has no path any app can open.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type {
  OpenWithCopyLaunched,
  OpenWithCopyRefusal,
  OpenWithCopyRefused,
  OpenWithCopySource,
  OpenWithNotOnThisMac,
} from '#lib/ipc/bindings.js'

/** Only the fields these cells assert on; the real options type is wider. */
type ToastOptions = { level: string; timeoutMs: number; id: string }

const addToast = vi.hoisted(() => vi.fn<(message: string, options: ToastOptions) => string>())
vi.mock('#lib/ui/toast/index.js', () => ({ addToast }))

let emitRefused: (payload: OpenWithCopyRefused) => void
let emitLaunched: (payload: OpenWithCopyLaunched) => void
const unlisten = vi.fn()
const unlistenLaunched = vi.fn()
let emitNotOnThisMac: (payload: OpenWithNotOnThisMac) => void
const unlistenNotOnThisMac = vi.fn()
vi.mock('#lib/tauri-commands/index.js', () => ({
  onOpenWithNotOnThisMac: (handler: (payload: OpenWithNotOnThisMac) => void) => {
    emitNotOnThisMac = handler
    return Promise.resolve(unlistenNotOnThisMac)
  },
  onOpenWithCopyRefused: (handler: (payload: OpenWithCopyRefused) => void) => {
    emitRefused = handler
    return Promise.resolve(unlisten)
  },
  onOpenWithCopyLaunched: (handler: (payload: OpenWithCopyLaunched) => void) => {
    emitLaunched = handler
    return Promise.resolve(unlistenLaunched)
  },
}))

import { formatByteSize } from '#lib/units/index.js'
import { startOpenWithCopyBridge } from './open-with-copy-bridge'

const TWO_GIB = 2 * 1024 * 1024 * 1024

function refused(reason: OpenWithCopyRefusal, source: OpenWithCopySource = 'archive'): OpenWithCopyRefused {
  return { fileName: 'holiday.mov', appName: 'QuickTime Player', reason, source }
}

beforeEach(async () => {
  addToast.mockClear()
  unlisten.mockClear()
  unlistenLaunched.mockClear()
  await startOpenWithCopyBridge()
})

describe('the open-with refusal notice', () => {
  it.each<[string, OpenWithCopyRefusal]>([
    ['too large', { kind: 'tooLarge', cap: TWO_GIB }],
    ['locked', { kind: 'needsPassword' }],
    ['out of disk space', { kind: 'notEnoughSpace', needed: TWO_GIB }],
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

  it('says a too-big file sits in a backup, not an archive', () => {
    emitRefused(refused({ kind: 'tooLarge', cap: TWO_GIB }, 'backup'))

    const [message] = addToast.mock.calls[0]
    expect(message).toContain('backup')
    expect(message).not.toContain('archive')
    expect(message).toContain('holiday.mov')
  })

  it('says how big a copy the disk has no room for', () => {
    emitRefused(refused({ kind: 'notEnoughSpace', needed: TWO_GIB }))

    const [message] = addToast.mock.calls[0]
    expect(message).toContain(formatByteSize(TWO_GIB))
    expect(message).toContain('space')
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
    const stop = await startOpenWithCopyBridge()
    stop()
    expect(unlisten).toHaveBeenCalled()
    expect(unlistenLaunched).toHaveBeenCalled()
    expect(unlistenNotOnThisMac).toHaveBeenCalled()
  })
})

describe('the open-with read-only copy notice', () => {
  it('says the app got a read-only copy whose changes stay there', () => {
    emitLaunched({ fileName: 'notes.txt', appName: 'TextEdit', copyCount: 1 })

    expect(addToast).toHaveBeenCalledTimes(1)
    const [message, options] = addToast.mock.calls[0]
    expect(message).toContain('TextEdit')
    expect(message).toContain('notes.txt')
    expect(message).toContain('read-only copy')
    expect(message).toContain('won’t go back')
    expect(options.level).toBe('info')
  })

  it('counts the copies when several rows were copied', () => {
    emitLaunched({ fileName: 'a.pdf', appName: 'Preview', copyCount: 3 })

    const [message] = addToast.mock.calls[0]
    expect(message).toContain('3 files')
    expect(message).not.toContain('a.pdf')
  })
})

describe('the open-with not-on-this-Mac notice', () => {
  // A direct SMB share whose macOS mount is gone, a phone, a server: the app would
  // have opened on a path that leads nowhere.
  it('names the app and points at the viewer instead', () => {
    emitNotOnThisMac({ appName: 'Preview' })

    expect(addToast).toHaveBeenCalledTimes(1)
    const [message, options] = addToast.mock.calls[0]
    expect(message).toContain('Preview')
    expect(message).toContain('F3')
    expect(options.level).toBe('info')
  })
})
