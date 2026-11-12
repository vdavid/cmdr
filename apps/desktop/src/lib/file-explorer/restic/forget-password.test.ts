/**
 * "Forget backup password": which repo it means, what it tells the backend,
 * where it leaves the panes, and what it says.
 */

import { describe, it, expect, vi, beforeEach, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('#lib/tauri-commands/index.js')>()
  return { ...actual, forgetSavedContainerPassword: vi.fn() }
})
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn() }))

import { forgetSavedContainerPassword, UnlockFailure } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import type { NavigateIntent } from '../pane/navigate'
import { forgetResticPassword, type ForgetPasswordExplorer } from './forget-password'

const NAS = 'smb-nas'
const REPO = '/Volumes/naspi/backups/m3-macbook-restic'
const VIEW = `${REPO}/Browse snapshots`

function explorerAt(
  left: string,
  right = '/Users/david',
  underCursor: string | null = null,
): ForgetPasswordExplorer & { navigated: NavigateIntent[] } {
  const navigated: NavigateIntent[] = []
  const paths = { left, right }
  return {
    navigated,
    getFocusedPane: () => 'left',
    getPaneLocation: (pane) => ({ volumeId: NAS, path: paths[pane] }),
    getFileAndPathUnderCursor: () =>
      Promise.resolve(
        underCursor === null ? null : { path: underCursor, filename: underCursor.split('/').at(-1) ?? '' },
      ),
    navigate: (intent) => {
      navigated.push(intent)
      return { status: 'started', settled: Promise.resolve() }
    },
  }
}

function lastToast(): { message: unknown; level: unknown } {
  const call = vi.mocked(addToast).mock.lastCall
  return { message: call?.[0], level: (call?.[1] as { level?: string } | undefined)?.level }
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(forgetSavedContainerPassword).mockResolvedValue('everywhere')
})

describe('forgetResticPassword', () => {
  it('forgets the repo the focused pane is in, and walks every pane in it back out to the repo folder', async () => {
    const explorer = explorerAt(`${VIEW}/latest/Users`, `${VIEW}/2026-10-19 05.30.08 (1b5b9b0d)`)

    await forgetResticPassword(explorer)

    expect(forgetSavedContainerPassword).toHaveBeenCalledWith(NAS, VIEW)
    expect(explorer.navigated).toEqual([
      {
        pane: 'left',
        to: { goTo: { volumeId: NAS, path: REPO } },
        source: 'user',
        selectName: 'Browse snapshots',
      },
      {
        pane: 'right',
        to: { goTo: { volumeId: NAS, path: REPO } },
        source: 'fallback',
        selectName: 'Browse snapshots',
      },
    ])
    expect(lastToast().level).toBe('success')
    expect(lastToast().message).toContain('m3-macbook-restic')
  })

  it('forgets the repo whose Browse snapshots row is under the cursor, and leaves the pane where it is', async () => {
    const explorer = explorerAt(REPO, '/Users/david', VIEW)

    await forgetResticPassword(explorer)

    expect(forgetSavedContainerPassword).toHaveBeenCalledWith(NAS, VIEW)
    expect(explorer.navigated).toEqual([])
    expect(lastToast().level).toBe('success')
  })

  it('says so when it isn’t near a backup, and calls nothing', async () => {
    const explorer = explorerAt('/Users/david', '/tmp', '/Users/david/notes.txt')

    await forgetResticPassword(explorer)

    expect(forgetSavedContainerPassword).not.toHaveBeenCalled()
    expect(explorer.navigated).toEqual([])
    expect(lastToast().level).toBe('info')
  })

  it.each([
    ['macos', 'Keychain Access'],
    ['windows', 'Credential Manager'],
    ['linux', 'your keyring app'],
  ] as const)(
    'warns when the store kept its copy, though the backup is locked for now (%s)',
    async (platform, store) => {
      onTestFinished(withPlatform(platform))
      vi.mocked(forgetSavedContainerPassword).mockResolvedValueOnce('storeKeptIt')
      const explorer = explorerAt(`${VIEW}/latest`)

      await forgetResticPassword(explorer)

      expect(explorer.navigated).toHaveLength(1)
      expect(lastToast().level).toBe('warn')
      // It names where to go to remove the password for good.
      expect(lastToast().message).toContain(store)
    },
  )

  it('leaves the panes alone and says the password stays when the backup couldn’t be reached', async () => {
    vi.mocked(forgetSavedContainerPassword).mockRejectedValueOnce(new UnlockFailure({ type: 'notLocked' }))
    const explorer = explorerAt(`${VIEW}/latest`)

    await forgetResticPassword(explorer)

    expect(explorer.navigated).toEqual([])
    expect(lastToast().level).toBe('warn')
    expect(lastToast().message).toContain('m3-macbook-restic')
  })
})
