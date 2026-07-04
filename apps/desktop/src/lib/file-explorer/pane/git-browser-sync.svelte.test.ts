/**
 * Tests for `git-browser-sync.svelte.ts`, the file pane's git chip + status-column
 * subscription lifecycle. They pin:
 * - a path on a git repo subscribes and exposes the live RepoInfo,
 * - moving to a different repo unsubscribes the old root before subscribing the new,
 * - both toggles off (or a network / MTP volume, or a no-backend-listing volume)
 *   skips the lookup and drops any active subscription,
 * - a non-git path leaves `gitRepoInfo` null and drops any active subscription,
 * - `subscribeToRepo` throwing falls back to the plain lookup info,
 * - `cleanup()` drops BOTH setting listeners and the active repo subscription
 *   (the leak the pre-extraction FilePane left behind).
 *
 * Uses Svelte runes (`$effect.root` + `$state`), so the filename carries the
 * `.svelte.` infix: the factory creates its `$effect` in a reactive root, and the
 * tests back the deps getters with `$state` so a mutation + `flushSync` drives the
 * effect exactly as the live component would. `syncGitState` is async, so results
 * are observed via `vi.waitFor`.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest'
import { flushSync } from 'svelte'
import type { RepoInfo } from '../git/git-store.svelte'

const { gitStore, settingsListeners, settingsValues, isMtpVolumeIdSpy } = vi.hoisted<{
  gitStore: { lookupRepoInfo: Mock; subscribeToRepo: Mock; unsubscribeFromRepo: Mock }
  settingsListeners: Record<string, (id: string, v: boolean) => void>
  settingsValues: Record<string, boolean>
  isMtpVolumeIdSpy: Mock
}>(() => ({
  gitStore: {
    lookupRepoInfo: vi.fn(),
    subscribeToRepo: vi.fn(),
    unsubscribeFromRepo: vi.fn().mockResolvedValue(undefined),
  },
  settingsListeners: {},
  settingsValues: {
    'fileExplorer.git.showRepoChip': true,
    'fileExplorer.git.showStatusColumn': true,
  },
  isMtpVolumeIdSpy: vi.fn((id: string) => id.startsWith('mtp-')),
}))

vi.mock('../git/git-store.svelte', () => ({
  lookupRepoInfo: gitStore.lookupRepoInfo,
  subscribeToRepo: gitStore.subscribeToRepo,
  unsubscribeFromRepo: gitStore.unsubscribeFromRepo,
}))
vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => settingsValues[id],
  onSpecificSettingChange: (id: string, cb: (id: string, v: boolean) => void) => {
    settingsListeners[id] = cb
    return () => delete settingsListeners[id]
  },
}))
vi.mock('$lib/mtp', () => ({ isMtpVolumeId: isMtpVolumeIdSpy }))

import { createGitBrowserSync, type GitBrowserSyncDeps } from './git-browser-sync.svelte'

function repo(over: Partial<RepoInfo>): RepoInfo {
  return { repoRoot: '/repo', branch: 'main', ...over } as unknown as RepoInfo
}

describe('createGitBrowserSync', () => {
  let dispose: (() => void) | undefined

  function create(opts: { path?: string; volumeId?: string; hasBackendListing?: boolean } = {}) {
    let currentPath = $state(opts.path ?? '/repo/src')
    let volumeId = $state(opts.volumeId ?? 'root')
    let hasBackendListing = $state(opts.hasBackendListing ?? true)
    const deps: GitBrowserSyncDeps = {
      getCurrentPath: () => currentPath,
      getVolumeId: () => volumeId,
      getHasBackendListing: () => hasBackendListing,
    }
    let sub!: ReturnType<typeof createGitBrowserSync>
    dispose = $effect.root(() => {
      sub = createGitBrowserSync(deps)
    })
    flushSync()
    return {
      sub,
      setPath: (v: string) => {
        currentPath = v
        flushSync()
      },
      setVolumeId: (v: string) => {
        volumeId = v
        flushSync()
      },
      setHasBackendListing: (v: boolean) => {
        hasBackendListing = v
        flushSync()
      },
    }
  }

  beforeEach(() => {
    vi.clearAllMocks()
    settingsValues['fileExplorer.git.showRepoChip'] = true
    settingsValues['fileExplorer.git.showStatusColumn'] = true
    for (const k of Object.keys(settingsListeners)) delete settingsListeners[k]
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('subscribes and exposes the live RepoInfo for a git path', async () => {
    const live = repo({ repoRoot: '/repo', branch: 'feature' })
    gitStore.lookupRepoInfo.mockResolvedValue(repo({ repoRoot: '/repo', branch: 'main' }))
    gitStore.subscribeToRepo.mockResolvedValue(live)

    const { sub } = create({ path: '/repo/src' })

    await vi.waitFor(() => {
      expect(sub.gitRepoInfo).toEqual(live)
    })
    expect(gitStore.subscribeToRepo).toHaveBeenCalledWith('/repo')
    expect(sub.showRepoChip).toBe(true)
    expect(sub.showGitStatusColumn).toBe(true)
  })

  it('unsubscribes the old repo root before subscribing a new one', async () => {
    gitStore.lookupRepoInfo.mockImplementation((p: string) =>
      Promise.resolve(repo({ repoRoot: p.startsWith('/other') ? '/other' : '/repo' })),
    )
    gitStore.subscribeToRepo.mockImplementation((root: string) => Promise.resolve(repo({ repoRoot: root })))

    const h = create({ path: '/repo/src' })
    await vi.waitFor(() => {
      expect(h.sub.gitRepoInfo?.repoRoot).toBe('/repo')
    })

    h.setPath('/other/pkg')
    await vi.waitFor(() => {
      expect(h.sub.gitRepoInfo?.repoRoot).toBe('/other')
    })
    expect(gitStore.unsubscribeFromRepo).toHaveBeenCalledWith('/repo')
    expect(gitStore.subscribeToRepo).toHaveBeenCalledWith('/other')
  })

  it('drops the subscription and clears info when both toggles are off', async () => {
    settingsValues['fileExplorer.git.showRepoChip'] = false
    settingsValues['fileExplorer.git.showStatusColumn'] = false
    gitStore.lookupRepoInfo.mockResolvedValue(repo({ repoRoot: '/repo' }))

    const { sub } = create({ path: '/repo/src' })
    await vi.waitFor(() => {
      expect(gitStore.lookupRepoInfo).not.toHaveBeenCalled()
    })
    expect(sub.gitRepoInfo).toBeNull()
  })

  it('re-runs and drops the active subscription when both toggles flip off', async () => {
    gitStore.lookupRepoInfo.mockResolvedValue(repo({ repoRoot: '/repo' }))
    gitStore.subscribeToRepo.mockResolvedValue(repo({ repoRoot: '/repo' }))

    const { sub } = create({ path: '/repo/src' })
    await vi.waitFor(() => {
      expect(sub.gitRepoInfo?.repoRoot).toBe('/repo')
    })

    settingsListeners['fileExplorer.git.showRepoChip']('fileExplorer.git.showRepoChip', false)
    settingsListeners['fileExplorer.git.showStatusColumn']('fileExplorer.git.showStatusColumn', false)
    flushSync()

    await vi.waitFor(() => {
      expect(sub.gitRepoInfo).toBeNull()
    })
    expect(gitStore.unsubscribeFromRepo).toHaveBeenCalledWith('/repo')
  })

  it('skips the lookup on an MTP volume (git cannot run over the MTP transport)', async () => {
    const { sub } = create({ path: '/DCIM', volumeId: 'mtp-123:456' })
    await vi.waitFor(() => {
      expect(sub.showRepoChip).toBe(true)
    })
    expect(gitStore.lookupRepoInfo).not.toHaveBeenCalled()
    expect(sub.gitRepoInfo).toBeNull()
  })

  it('skips the lookup on a volume without a backend listing', async () => {
    create({ path: 'smb://host', volumeId: 'network', hasBackendListing: false })
    await vi.waitFor(() => {
      expect(isMtpVolumeIdSpy).toHaveBeenCalled()
    })
    expect(gitStore.lookupRepoInfo).not.toHaveBeenCalled()
  })

  it('leaves gitRepoInfo null on a non-git path and drops any active subscription', async () => {
    gitStore.lookupRepoInfo.mockResolvedValueOnce(repo({ repoRoot: '/repo' }))
    gitStore.subscribeToRepo.mockResolvedValue(repo({ repoRoot: '/repo' }))
    const h = create({ path: '/repo/src' })
    await vi.waitFor(() => {
      expect(h.sub.gitRepoInfo?.repoRoot).toBe('/repo')
    })

    gitStore.lookupRepoInfo.mockResolvedValue(null)
    h.setPath('/tmp/not-a-repo')
    await vi.waitFor(() => {
      expect(h.sub.gitRepoInfo).toBeNull()
    })
    expect(gitStore.unsubscribeFromRepo).toHaveBeenCalledWith('/repo')
  })

  it('falls back to the plain lookup info when subscribeToRepo throws', async () => {
    const info = repo({ repoRoot: '/repo', branch: 'main' })
    gitStore.lookupRepoInfo.mockResolvedValue(info)
    gitStore.subscribeToRepo.mockRejectedValue(new Error('watcher busy'))

    const { sub } = create({ path: '/repo/src' })
    await vi.waitFor(() => {
      expect(sub.gitRepoInfo).toEqual(info)
    })
  })

  it('cleanup drops both setting listeners and the active repo subscription', async () => {
    gitStore.lookupRepoInfo.mockResolvedValue(repo({ repoRoot: '/repo' }))
    gitStore.subscribeToRepo.mockResolvedValue(repo({ repoRoot: '/repo' }))
    const { sub } = create({ path: '/repo/src' })
    await vi.waitFor(() => {
      expect(sub.gitRepoInfo?.repoRoot).toBe('/repo')
    })

    expect(Object.keys(settingsListeners)).toHaveLength(2)
    sub.cleanup()
    expect(settingsListeners['fileExplorer.git.showRepoChip']).toBeUndefined()
    expect(settingsListeners['fileExplorer.git.showStatusColumn']).toBeUndefined()
    expect(gitStore.unsubscribeFromRepo).toHaveBeenCalledWith('/repo')
  })
})
