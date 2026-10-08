import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { openDebugLog } from './open-debug-log'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

const mocks = vi.hoisted(() => ({
  getDebugLogPath: vi.fn<() => Promise<string | null>>(),
  openFileViewer: vi.fn<() => Promise<void>>(),
  addToast: vi.fn(),
  logError: vi.fn(),
}))

vi.mock('$lib/tauri-commands', async () => await import('../tauri-commands/logging'))
vi.mock('$lib/file-viewer/open-viewer', () => ({ openFileViewer: mocks.openFileViewer }))
vi.mock('$lib/ui/toast', () => ({ addToast: mocks.addToast }))
vi.mock('./logger', () => ({ getAppLogger: () => ({ error: mocks.logError }) }))

beforeEach(() => {
  vi.resetAllMocks()
  mocks.openFileViewer.mockResolvedValue(undefined)
  installIpcMock().mock('get_debug_log_path', mocks.getDebugLogPath)
})

afterEach(clearIpcMocks)

describe('view debug log', () => {
  it('opens the backend-resolved path in a tailed viewer', async () => {
    const path = '/tmp/cmdr-worktree/logs/cmdr.log'
    mocks.getDebugLogPath.mockResolvedValue(path)
    await openDebugLog()
    expect(mocks.openFileViewer).toHaveBeenCalledWith(path, 'root', { tail: true })
    expect(mocks.addToast).not.toHaveBeenCalled()
  })

  it('explains disabled storage instead of opening a viewer without a file', async () => {
    mocks.getDebugLogPath.mockResolvedValue(null)
    await openDebugLog()
    expect(mocks.openFileViewer).not.toHaveBeenCalled()
    expect(mocks.addToast).toHaveBeenCalledWith(expect.any(String), { level: 'info' })
  })

  it.each(['path query', 'viewer launch'])(
    'reports a rejected %s without leaking the diagnostic into UI',
    async (step) => {
      const diagnostic = new Error('private diagnostic')
      mocks.getDebugLogPath.mockResolvedValue('/tmp/cmdr.log')
      if (step === 'path query') mocks.getDebugLogPath.mockRejectedValue(diagnostic)
      else mocks.openFileViewer.mockRejectedValue(diagnostic)
      await expect(openDebugLog()).resolves.toBeUndefined()
      expect(mocks.logError).toHaveBeenCalled()
      expect(mocks.addToast).toHaveBeenCalledWith('Couldn’t open the debug log. Try again.', { level: 'error' })
    },
  )
})
