/**
 * Tier-3 tests for `AdbSection.svelte` (File systems › Android (ADB)).
 *
 * ❗ The one that matters is the Re-check budget: `recheckAdbInstall` is the only
 * path allowed to retry `adb start-server`, so it may run once per click and
 * never on mount. A regression there spawns processes behind the user's back,
 * and nothing else in the app would notice.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import AdbSection from './AdbSection.svelte'

/** What `get_adb_install_status` and `recheck_adb_install` answer. */
interface InstallStatus {
  binaryPath: string | null
  tracking: boolean
}

const getAdbInstallStatus = vi.fn<() => Promise<InstallStatus>>()
const recheckAdbInstall = vi.fn<() => Promise<InstallStatus>>()

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'fileOperations.adbEnabled') return true
    if (key === 'fileOperations.adbBinaryPath') return ''
    return undefined
  }),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  getAdbInstallStatus: () => getAdbInstallStatus(),
  recheckAdbInstall: () => recheckAdbInstall(),
}))

const openDialog = vi.fn<() => Promise<string | null>>()

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: () => openDialog(),
}))

const missing: InstallStatus = { binaryPath: null, tracking: false }
const found: InstallStatus = { binaryPath: '/opt/homebrew/bin/adb', tracking: true }

async function mountSection(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AdbSection, { target, props: { searchQuery: '' } })
  await tick()
  await tick()
  return target
}

/** The Re-check button: the only one in the status row. */
function recheckButton(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector('.status-body button')
}

describe('the status row', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getAdbInstallStatus.mockResolvedValue(missing)
    recheckAdbInstall.mockResolvedValue(found)
  })

  it('reads what is already known on mount, and never looks again by itself', async () => {
    const target = await mountSection()

    expect(getAdbInstallStatus).toHaveBeenCalledTimes(1)
    expect(recheckAdbInstall).not.toHaveBeenCalled()
    target.remove()
  })

  it('says where adb lives once it is found', async () => {
    getAdbInstallStatus.mockResolvedValue(found)
    const target = await mountSection()

    expect(target.querySelector('.status-value')?.textContent).toContain('/opt/homebrew/bin/adb')
    target.remove()
  })

  it('offers the install command only while adb is missing', async () => {
    const target = await mountSection()
    expect(target.querySelector('.install')?.textContent).toContain('brew install android-platform-tools')
    target.remove()

    getAdbInstallStatus.mockResolvedValue(found)
    const withAdb = await mountSection()
    expect(withAdb.querySelector('.install')).toBeNull()
    withAdb.remove()
  })

  it('says whether phones are being watched for', async () => {
    const target = await mountSection()
    expect(target.querySelector('.status-note')?.textContent).toContain('Not watching')
    target.remove()
  })
})

describe('Re-check', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getAdbInstallStatus.mockResolvedValue(missing)
    recheckAdbInstall.mockResolvedValue(found)
  })

  /**
   * Picking a binary is a person saying "look here now", and the path only takes
   * effect once the tracker restarts under it — so the status has to be asked
   * again, ❌ never read stale.
   */
  it('re-checks after a Browse pick, so the status is about the new binary', async () => {
    openDialog.mockResolvedValue('/opt/android/platform-tools/adb')
    const target = await mountSection()

    const browse = Array.from(target.querySelectorAll('.path-field button'))[0] as HTMLButtonElement | undefined
    browse?.click()
    await tick()
    await tick()
    await tick()

    expect(recheckAdbInstall).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it('runs one call per click and shows what came back', async () => {
    const target = await mountSection()

    recheckButton(target)?.click()
    await tick()
    await tick()

    expect(recheckAdbInstall).toHaveBeenCalledTimes(1)
    expect(target.querySelector('.status-value')?.textContent).toContain('/opt/homebrew/bin/adb')
    target.remove()
  })

  /** A double-click is one person's one intent, not two `adb start-server` tries. */
  it('ignores a second click while the first is still running', async () => {
    let release: ((status: InstallStatus) => void) | undefined
    recheckAdbInstall.mockImplementation(
      () =>
        new Promise<InstallStatus>((resolve) => {
          release = resolve
        }),
    )
    const target = await mountSection()

    recheckButton(target)?.click()
    await tick()
    recheckButton(target)?.click()
    await tick()

    expect(recheckAdbInstall).toHaveBeenCalledTimes(1)
    release?.(found)
    target.remove()
  })
})
