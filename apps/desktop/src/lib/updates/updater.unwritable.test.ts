/**
 * An update found on an install this user can't write: in practice a `.pkg` install, owned by `root`. Admin rights
 * don't help (macOS 13+ App Management refuses an admin-run helper too, `src-tauri/src/updater/DETAILS.md` § A bundle
 * that can't be written), so the check stops before the download and says, once per version, what the person CAN do.
 *
 * The `osascript` admin prompt this replaces fired from the background timer, every interval, and always ended in the
 * same refusal. These tests pin that nothing here downloads, prompts, or files an error report.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { SecurityNotice } from '#lib/ipc/bindings.js'
import { UpdateInstallFailure } from './update-install-failure'

type Blocker = 'translocated' | 'readOnlyVolume' | 'notWritableAdminUser' | 'notWritableStandardUser'

const {
  addToastMock,
  checkForUpdateMock,
  downloadUpdateMock,
  installUpdateMock,
  updateWriteBlockerMock,
  updateBundlePathMock,
  trackEventMock,
  settings,
  setSettingMock,
} = vi.hoisted(() => {
  const settings = new Map<string, unknown>()
  return {
    addToastMock: vi.fn(),
    checkForUpdateMock: vi.fn(),
    downloadUpdateMock: vi.fn(() => Promise.resolve()),
    installUpdateMock: vi.fn<() => Promise<{ securityNotice: SecurityNotice | null }>>(() =>
      Promise.resolve({ securityNotice: null }),
    ),
    updateWriteBlockerMock: vi.fn<() => Promise<Blocker | null>>(() => Promise.resolve(null)),
    updateBundlePathMock: vi.fn<() => Promise<string | null>>(() => Promise.resolve('/Applications/Cmdr.app')),
    trackEventMock: vi.fn(),
    settings,
    setSettingMock: vi.fn((id: string, value: unknown) => {
      settings.set(id, value)
    }),
  }
})

vi.mock('#lib/tauri-commands/index.js', () => ({
  checkForUpdate: checkForUpdateMock,
  downloadUpdate: downloadUpdateMock,
  installUpdate: installUpdateMock,
  updateWriteBlocker: updateWriteBlockerMock,
  updateBundlePath: updateBundlePathMock,
  updateCheckDueIn: vi.fn(() => Promise.resolve(0)),
  recordUpdateCheck: vi.fn(() => Promise.resolve()),
  trackEvent: trackEventMock,
}))

vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: addToastMock, dismissToast: vi.fn() }))

vi.mock('#lib/settings/settings-store.js', () => ({
  getSetting: vi.fn((id: string) => {
    if (settings.has(id)) return settings.get(id)
    if (id === 'onboarding.completed') return false
    if (id === 'updates.autoCheck') return true
    if (id === 'updates.unwritableNoticeVersion') return ''
    return 60 * 60 * 1000
  }),
  setSetting: setSettingMock,
  forceSave: vi.fn(() => Promise.resolve(true)),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(() => Promise.resolve('0.52.0')) }))

const { errorLogMock } = vi.hoisted(() => ({ errorLogMock: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: errorLogMock }),
}))

import {
  _resetUpdaterStateForTest,
  checkForUpdates,
  notifyOnboardingComplete,
  setOnboardingShowing,
  updateBlockerNotice,
  updateState,
} from './updater.svelte'
import UpdateUnwritableToastContent from './UpdateUnwritableToastContent.svelte'

const anUpdate = { kind: 'available', version: '0.54.0' } as const

/** The calls that raised the unwritable notice, oldest first. */
function unwritableToasts(): unknown[][] {
  return addToastMock.mock.calls.filter(([, options]) => (options as { id?: string }).id === 'update-unwritable')
}

describe('an update found on an install this user can’t write', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    settings.clear()
    addToastMock.mockClear()
    setSettingMock.mockClear()
    checkForUpdateMock.mockReset()
    downloadUpdateMock.mockClear()
    installUpdateMock.mockReset()
    installUpdateMock.mockResolvedValue({ securityNotice: null })
    updateWriteBlockerMock.mockReset()
    updateWriteBlockerMock.mockResolvedValue(null)
    updateBundlePathMock.mockClear()
    trackEventMock.mockClear()
    errorLogMock.mockClear()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('tells an administrator to replace the app, and downloads nothing', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    updateWriteBlockerMock.mockResolvedValueOnce('notWritableAdminUser')

    await checkForUpdates('poll')

    expect(downloadUpdateMock).not.toHaveBeenCalled()
    expect(installUpdateMock).not.toHaveBeenCalled()
    expect(unwritableToasts()).toEqual([
      [
        UpdateUnwritableToastContent,
        expect.objectContaining({
          id: 'update-unwritable',
          level: 'warn',
          dismissal: 'persistent',
          props: { version: '0.54.0', blocker: 'notWritableAdminUser', bundlePath: '/Applications/Cmdr.app' },
        }),
      ],
    ])
    // The "move Cmdr to Applications" dialog is for where the app sits; this is about who owns it.
    expect(updateBlockerNotice.blocker).toBeNull()
    expect(updateState.unwritable).toEqual({ version: '0.54.0', blocker: 'notWritableAdminUser' })
    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      trigger: 'poll',
      outcome: 'blocked',
      failure: 'not_writable_admin_user',
      staged_version: 'none',
    })
  })

  it('tells a standard user to ask whoever manages the Mac', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    updateWriteBlockerMock.mockResolvedValueOnce('notWritableStandardUser')

    await checkForUpdates('startup')

    expect(unwritableToasts()).toHaveLength(1)
    expect((unwritableToasts()[0][1] as { props: { blocker: string } }).props.blocker).toBe('notWritableStandardUser')
    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      trigger: 'startup',
      outcome: 'blocked',
      failure: 'not_writable_standard_user',
      staged_version: 'none',
    })
  })

  it('says it once per version, across checks and relaunches', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValue(anUpdate)
    updateWriteBlockerMock.mockResolvedValue('notWritableStandardUser')

    await checkForUpdates('poll')
    expect(setSettingMock).toHaveBeenCalledWith('updates.unwritableNoticeVersion', '0.54.0')

    // The stamp outlives the process: a relaunch reads it back from settings.
    _resetUpdaterStateForTest()
    await notifyOnboardingComplete()
    await checkForUpdates('startup')
    await checkForUpdates('poll')

    expect(unwritableToasts()).toHaveLength(1)
    // The check itself keeps running: it's what keeps the install counted as active.
    expect(checkForUpdateMock).toHaveBeenCalledTimes(3)
  })

  it('says it again for a newer version', async () => {
    await notifyOnboardingComplete()
    updateWriteBlockerMock.mockResolvedValue('notWritableAdminUser')
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    await checkForUpdates('poll')

    checkForUpdateMock.mockResolvedValueOnce({ kind: 'available', version: '0.55.0' })
    await checkForUpdates('poll')

    expect(unwritableToasts().map(([, options]) => (options as { props: { version: string } }).props.version)).toEqual([
      '0.54.0',
      '0.55.0',
    ])
  })

  it('holds the notice back during onboarding, then raises it when the wizard closes', async () => {
    setOnboardingShowing(true)
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    updateWriteBlockerMock.mockResolvedValueOnce('notWritableAdminUser')

    await checkForUpdates('startup')
    expect(unwritableToasts()).toHaveLength(0)
    expect(setSettingMock).not.toHaveBeenCalledWith('updates.unwritableNoticeVersion', expect.anything())

    await notifyOnboardingComplete()
    setOnboardingShowing(false)
    await vi.waitFor(() => {
      expect(unwritableToasts()).toHaveLength(1)
    })
  })

  it('raises the same notice when the install itself is refused, without an error report', async () => {
    // The safety net under the pre-flight probe: a write the sync was refused mid-way.
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    installUpdateMock.mockRejectedValueOnce(
      new UpdateInstallFailure({ type: 'unwritable', blocker: 'notWritableStandardUser' }),
    )

    await checkForUpdates('poll')

    expect(unwritableToasts()).toHaveLength(1)
    expect(updateState.failure).toBeNull()
    expect(updateState.status).toBe('idle')
    expect(errorLogMock).not.toHaveBeenCalled()
    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      trigger: 'poll',
      outcome: 'blocked',
      failure: 'not_writable_standard_user',
      staged_version: 'none',
    })
  })

  it('still raises the move dialog for a bundle on a read-only volume', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    installUpdateMock.mockRejectedValueOnce(new UpdateInstallFailure({ type: 'unwritable', blocker: 'readOnlyVolume' }))

    await checkForUpdates('poll')

    expect(updateBlockerNotice.blocker).toBe('readOnlyVolume')
    expect(unwritableToasts()).toHaveLength(0)
  })
})
