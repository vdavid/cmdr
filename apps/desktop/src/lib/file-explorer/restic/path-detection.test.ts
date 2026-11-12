import { describe, it, expect } from 'vitest'
import { isVirtualResticPath, resticRepoRoot, resticViewComponentIndex } from './path-detection'

const REPO = '/Volumes/testshare/backups/test-restic'

describe('isVirtualResticPath', () => {
  it('detects the view folder and everything below it', () => {
    expect(isVirtualResticPath(`${REPO}/Browse snapshots`)).toBe(true)
    expect(isVirtualResticPath(`${REPO}/Browse snapshots/latest`)).toBe(true)
    expect(isVirtualResticPath(`${REPO}/Browse snapshots/2026-10-12 21.00.03 (a1b2c3d4)/Users/me`)).toBe(true)
  })

  it('treats the repo folder and its real files as real', () => {
    expect(isVirtualResticPath(REPO)).toBe(false)
    expect(isVirtualResticPath(`${REPO}/snapshots`)).toBe(false)
    expect(isVirtualResticPath(`${REPO}/config`)).toBe(false)
  })

  it('needs the exact component, so near misses stay real', () => {
    expect(isVirtualResticPath('/r/browse snapshots/latest')).toBe(false)
    expect(isVirtualResticPath('/r/Browse snapshots.txt')).toBe(false)
    expect(isVirtualResticPath('/r/My Browse snapshots')).toBe(false)
  })

  it('returns false for normal paths', () => {
    expect(isVirtualResticPath('/Users/me/foo')).toBe(false)
    expect(isVirtualResticPath('/')).toBe(false)
    expect(isVirtualResticPath('')).toBe(false)
  })
})

describe('resticViewComponentIndex', () => {
  it('is the first view component', () => {
    expect(resticViewComponentIndex('/r/Browse snapshots/latest'.split('/'))).toBe(2)
    expect(resticViewComponentIndex('/r/Browse snapshots/latest/b/Browse snapshots'.split('/'))).toBe(2)
  })

  it('is -1 outside every view', () => {
    expect(resticViewComponentIndex('/r/Browse snapshots.txt'.split('/'))).toBe(-1)
  })
})

describe('resticRepoRoot', () => {
  it('is the folder holding the first view component', () => {
    expect(resticRepoRoot(`${REPO}/Browse snapshots/latest/Users`)).toBe(REPO)
    expect(resticRepoRoot(`${REPO}/Browse snapshots`)).toBe(REPO)
  })

  it('stops at the FIRST view component, like the backend route', () => {
    // A repo backed up inside a snapshot is served by the outer repo.
    expect(resticRepoRoot('/r/Browse snapshots/latest/backup/Browse snapshots/x')).toBe('/r')
  })

  it('answers the root for a repo at the top of a volume', () => {
    expect(resticRepoRoot('/Browse snapshots/latest')).toBe('/')
  })

  it('is null outside the view', () => {
    expect(resticRepoRoot(REPO)).toBeNull()
    expect(resticRepoRoot('/r/Browse snapshots.txt')).toBeNull()
  })
})
