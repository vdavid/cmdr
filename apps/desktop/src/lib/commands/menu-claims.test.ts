import { describe, it, expect, vi, afterEach } from 'vitest'
import { claimMenuCommand, runMenuClaim } from './menu-claims'

describe('menu claims', () => {
  const releases: (() => void)[] = []
  afterEach(() => {
    for (const release of releases.splice(0)) release()
  })

  it('runs the claim while it holds, and stops once released', () => {
    const run = vi.fn()
    const release = claimMenuCommand('file.rename', run)
    expect(runMenuClaim('file.rename')).toBe(true)
    expect(run).toHaveBeenCalledOnce()

    release()
    expect(runMenuClaim('file.rename')).toBe(false)
    expect(run).toHaveBeenCalledOnce()
  })

  it('leaves an unclaimed command alone', () => {
    releases.push(claimMenuCommand('file.rename', vi.fn()))
    expect(runMenuClaim('file.copy')).toBe(false)
  })

  it('the newest claim wins, and releasing a superseded one keeps it', () => {
    const older = vi.fn()
    const newer = vi.fn()
    const releaseOlder = claimMenuCommand('file.rename', older)
    releases.push(claimMenuCommand('file.rename', newer))

    releaseOlder()
    expect(runMenuClaim('file.rename')).toBe(true)
    expect(newer).toHaveBeenCalledOnce()
    expect(older).not.toHaveBeenCalled()
  })
})
