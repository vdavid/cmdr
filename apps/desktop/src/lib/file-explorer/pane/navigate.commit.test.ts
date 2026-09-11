/**
 * `navigate(intent, deps)` headless seam tests: WHEN a commit lands.
 *
 * The per-arm optimistic-commit ordering (P4: in-place is NOT optimistic and
 * commits only at listing-complete, a volume switch IS and commits
 * synchronously), the background-correction token/generation bookkeeping that
 * rides a volume switch, `commitPathFromListing`'s stale-listing drop policy
 * (L6), and the same-token self-re-entry rule for a parent-nav / walk-up
 * completion.
 *
 * Exercises `navigate()` and `commitPathFromListing()` DIRECTLY against
 * injected fakes (no mount needed); harness in `navigate.test-fixtures.ts`.
 * Assertions are on OBSERVABLE OUTCOMES (committed tab state, history depth,
 * persisted events), never internal function identities.
 */
import { describe, it, expect, beforeEach } from 'vitest'
import { navigate, commitPathFromListing } from './navigate'
import { flush, makeHarness, type Harness } from './navigate.test-fixtures'
import type { DetermineNavigationPathArgs } from '../navigation/path-navigation'

let h: Harness
beforeEach(() => {
  h = makeHarness()
})

describe('in-place path nav (P4 — NOT optimistic, commits at listing-complete)', () => {
  it('drives the FilePane primitive and returns its promise as `settled`; does NOT commit on call', () => {
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: '/Users/me/sub' } }, source: 'user' },
      h.deps,
    )

    expect(result.status).toBe('started')
    // The FilePane primitive was driven; the path has NOT advanced yet (in-place
    // commit lands later via commitPathFromListing).
    expect(h.paneState.left.paneRef?.navigateToPath).toHaveBeenCalledWith('/Users/me/sub', undefined)
    expect(h.tab('left').path).toBe('/Users/me')
  })

  it('forwards `selectName` to the FilePane primitive', () => {
    navigate(
      {
        pane: 'left',
        to: { goTo: { volumeId: 'root', path: '/Users/me/sub' } },
        source: 'user',
        selectName: 'file.txt',
      },
      h.deps,
    )
    expect(h.paneState.left.paneRef?.navigateToPath).toHaveBeenCalledWith('/Users/me/sub', 'file.txt')
  })

  it('commitPathFromListing commits the path + pushes one history entry + records last-used', () => {
    // Drive the in-place nav, then land its completion.
    navigate({ pane: 'left', to: { goTo: { volumeId: 'root', path: '/Users/me/sub' } }, source: 'user' }, h.deps)
    const depthBefore = h.tab('left').history.stack.length

    const committed = commitPathFromListing(h.deps, 'left', '/Users/me/sub')

    expect(committed).toBe(true)
    expect(h.tab('left').path).toBe('/Users/me/sub')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    expect(h.tab('left').history.stack.at(-1)?.path).toBe('/Users/me/sub')
    expect(h.lastUsedRecords).toContainEqual({ volumeId: 'root', path: '/Users/me/sub' })
  })
})

describe('volume switch (P4 — truly optimistic, synchronous commit)', () => {
  it('commits volumeId + path + history SYNCHRONOUSLY, before any listing', () => {
    const depthBefore = h.tab('left').history.stack.length
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)

    // Committed immediately (no await, no listing event).
    expect(h.tab('left').volumeId).toBe('ext')
    expect(h.tab('left').path).toBe('/Volumes/Ext')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
    expect(h.tab('left').history.stack.at(-1)).toMatchObject({ volumeId: 'ext', path: '/Volumes/Ext' })
  })

  it("records the OLD path as the old volume's last-used before the swap", () => {
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    // The pre-save of the OLD path under the OLD volume (DPE:615).
    expect(h.lastUsedRecords[0]).toEqual({ volumeId: 'root', path: '/Users/me' })
  })

  it("shifts focus to the navigated pane for a 'user' source", () => {
    navigate({ pane: 'right', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    expect(h.setFocusedPane).toHaveBeenCalledWith('right')
  })

  it("does NOT shift focus for a 'mirror' source (L1 restoreFocus semantics)", () => {
    navigate(
      { pane: 'right', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'mirror' },
      h.deps,
    )
    expect(h.setFocusedPane).not.toHaveBeenCalled()
  })

  it("does NOT shift focus at commit for an 'mcp' source (the listener shifts synchronously instead)", () => {
    // The MCP nav listener calls setFocusedPane when the nav is ACCEPTED, so a
    // commit-time shift here would be a second, late-landing shift racing the
    // agent's next action. Pre-fix this fired and intermittently ate the very
    // next keystroke (the E2E enterEntry flake).
    navigate({ pane: 'right', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'mcp' }, h.deps)
    expect(h.setFocusedPane).not.toHaveBeenCalled()
  })

  it('uses the volume mount path for the background correction lookup', () => {
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    expect(h.determineNavigationPath).toHaveBeenCalledWith({
      volumeId: 'ext',
      volumePath: '/Volumes/Ext',
      targetPath: '/Volumes/Ext',
      otherPane: expect.anything() as DetermineNavigationPathArgs['otherPane'],
    })
  })

  it("hands the background correction the volume's landing, so a place with nothing remembered opens on its start folder", () => {
    const root = 'sftp://ada@nas.local:22/srv/data'
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'sftp-nas', path: root } }, source: 'user' }, h.deps)
    expect(h.determineNavigationPath).toHaveBeenCalledWith(
      expect.objectContaining({ volumeId: 'sftp-nas', landingPath: 'sftp://ada@nas.local:22/srv/data/photos' }),
    )
  })
})

describe('background correction (global correctionGen, the old volumeChangeGeneration)', () => {
  it('applies a better path when the correction is still the latest', async () => {
    h.determineNavigationPath.mockResolvedValue('/Volumes/Ext/photos')
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    await flush()

    expect(h.tab('left').path).toBe('/Volumes/Ext/photos')
    expect(h.tab('left').history.stack.at(-1)?.path).toBe('/Volumes/Ext/photos')
  })

  it('DROPS a stale correction superseded by a newer volume change on the SAME pane', async () => {
    // First switch: its correction resolves to a "better" path but slowly.
    let resolveFirst: (p: string) => void = () => {}
    const slowCorrection = new Promise<string>((r) => {
      resolveFirst = r
    })
    h.determineNavigationPath.mockReturnValueOnce(slowCorrection)
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)

    // A newer navigate() bumps the global correctionGen before the first resolves.
    h.determineNavigationPath.mockResolvedValueOnce('/')
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'root', path: '/' } }, source: 'user' }, h.deps)
    await flush()
    expect(h.tab('left').volumeId).toBe('root')
    expect(h.tab('left').path).toBe('/')

    // Now the STALE first correction resolves — it must be dropped (gen superseded).
    resolveFirst('/Volumes/Ext/should-be-dropped')
    await flush()
    expect(h.tab('left').path).toBe('/') // unchanged — stale correction dropped
    expect(h.tab('left').volumeId).toBe('root')
  })

  it('DROPS a left-pane correction superseded by a volume change on the RIGHT pane (GLOBAL gen)', async () => {
    // The correctionGen is GLOBAL (the old `volumeChangeGeneration` was a single
    // counter shared by both panes), not per-pane: a volume change on the RIGHT
    // pane must drop a still-pending correction on the LEFT pane. Without this, a
    // simultaneous two-pane reset (E2E `ensureAppReady`'s double `mcp-volume-select`)
    // runs both corrections and re-enters the listing cycle on both panes — a freeze.
    let resolveLeft: (p: string) => void = () => {}
    const slowLeft = new Promise<string>((r) => {
      resolveLeft = r
    })
    h.determineNavigationPath.mockReturnValueOnce(slowLeft)
    navigate({ pane: 'left', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    const leftPathAfterSwitch = h.tab('left').path

    // The RIGHT pane switches volumes — this bumps the GLOBAL correctionGen.
    h.determineNavigationPath.mockResolvedValueOnce('/Volumes/Ext')
    navigate({ pane: 'right', to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } }, source: 'user' }, h.deps)
    await flush()

    // The left correction resolves late — dropped because the right switch bumped
    // the shared gen past it.
    resolveLeft('/Volumes/Ext/should-be-dropped')
    await flush()
    expect(h.tab('left').path).toBe(leftPathAfterSwitch) // unchanged — left correction dropped
  })
})

describe('commitPathFromListing — stale-listing drop policy (L6, token + foreign-path)', () => {
  it('real-volume branch: drops a listing whose path is not on the pane volume', () => {
    h = makeHarness({ left: { path: '/Volumes/Ext', volumeId: 'ext' } })
    const committed = commitPathFromListing(h.deps, 'left', '/Users/me/deep')
    expect(committed).toBe(false)
    expect(h.tab('left').path).toBe('/Volumes/Ext')
    expect(h.lastUsedRecords).toEqual([])
  })

  it('network branch: drops a non-smb path', () => {
    h = makeHarness({ left: { path: 'smb://', volumeId: 'network' } })
    expect(commitPathFromListing(h.deps, 'left', '/Users/me/deep')).toBe(false)
    expect(h.tab('left').path).toBe('smb://')
  })

  it('search-results branch: drops a non-search-results path', () => {
    h = makeHarness({ left: { path: 'search-results://sr-1', volumeId: 'search-results' } })
    expect(commitPathFromListing(h.deps, 'left', '/Library/x')).toBe(false)
    expect(h.tab('left').path).toBe('search-results://sr-1')
  })

  it('commits a non-stale path that IS on the current volume', () => {
    const depthBefore = h.tab('left').history.stack.length
    expect(commitPathFromListing(h.deps, 'left', '/Users/me/deep')).toBe(true)
    expect(h.tab('left').path).toBe('/Users/me/deep')
    expect(h.tab('left').history.stack.length).toBe(depthBefore + 1)
  })
})

describe('same-token self-re-entry (parent-nav / walk-up completion)', () => {
  it('a parent-nav completion re-entering via commitPathFromListing is NOT dropped', () => {
    // Start a parent-nav (mints a token, drives the primitive).
    navigate({ pane: 'left', to: { history: 'parent' }, source: 'user' }, h.deps)
    const tokenAfterParent = h.deps.tokens.get('left')

    // The primitive's onPathChange fires for the resolved parent — same logical
    // navigation, no new navigate() minted a token, so it commits (not dropped).
    const committed = commitPathFromListing(h.deps, 'left', '/Users')
    expect(committed).toBe(true)
    expect(h.tab('left').path).toBe('/Users')
    // The token was NOT bumped by the self-re-entry.
    expect(h.deps.tokens.get('left')).toBe(tokenAfterParent)
  })
})
