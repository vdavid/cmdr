/**
 * The short "Opening…" toast Enter and double-click raise for a file handed to its
 * default app: named after the app when LaunchServices knows it, nameless otherwise,
 * and never in the way of the launch itself.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'
import type { FileEntry } from '../types'

const { ipc, toast } = vi.hoisted<{ ipc: { defaultAppName: Mock }; toast: { addToast: Mock } }>(() => ({
  ipc: { defaultAppName: vi.fn() },
  toast: { addToast: vi.fn() },
}))

vi.mock('#lib/tauri-commands/index.js', () => ({ defaultAppName: ipc.defaultAppName }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: toast.addToast }))

import { announceOpening, OPENING_TOAST_MS } from './opening-toast'

function entryOf(over: Partial<FileEntry> = {}): FileEntry {
  return {
    name: 'report.pdf',
    path: '/Volumes/testshare/report.pdf',
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'user',
    group: 'staff',
    iconId: 'file',
    ...over,
  }
}

describe('announceOpening', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('names the file and the app it opens in, for two seconds', async () => {
    ipc.defaultAppName.mockResolvedValue('Preview')
    await announceOpening(entryOf())
    expect(ipc.defaultAppName).toHaveBeenCalledWith('/Volumes/testshare/report.pdf')
    expect(toast.addToast).toHaveBeenCalledWith(
      'report.pdf is opening in Preview…',
      expect.objectContaining({ dismissal: 'transient', timeoutMs: OPENING_TOAST_MS, id: 'opening-file' }),
    )
    expect(OPENING_TOAST_MS).toBe(2000)
  })

  it('says only the file when no app claims it', async () => {
    ipc.defaultAppName.mockResolvedValue(null)
    await announceOpening(entryOf({ name: 'notes' }))
    expect(toast.addToast).toHaveBeenCalledWith('Opening notes…', expect.anything())
  })

  it('doesn’t ask which app opens a bundle: the bundle IS the app', async () => {
    await announceOpening(entryOf({ name: 'Safari.app', path: '/Applications/Safari.app', isDirectory: true }))
    expect(ipc.defaultAppName).not.toHaveBeenCalled()
    expect(toast.addToast).toHaveBeenCalledWith('Opening Safari.app…', expect.anything())
  })
})
