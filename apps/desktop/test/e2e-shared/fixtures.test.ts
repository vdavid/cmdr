/**
 * Unit tests for the fixture cache + hardlink machinery in fixtures.ts.
 *
 * These tests don't run in CI's E2E flow (Playwright wires them in via global
 * setup) but they verify the cache contract: cache-hit reuse, hardlink
 * sharing, per-instance isolation, and the verified-rename concurrency
 * pattern. Cleaning up after each test keeps the global state honest.
 */

import { describe, it, expect, afterEach } from 'vitest'
import fs from 'fs'
import path from 'path'
import { execSync } from 'child_process'
import { createFixtures, cleanupFixtures, getCacheRoot, ensureCacheBuilt } from './fixtures.js'

const createdRoots: string[] = []

function track(root: string): string {
  createdRoots.push(root)
  return root
}

afterEach(() => {
  for (const root of createdRoots) {
    try {
      cleanupFixtures(root)
    } catch {
      // Ignore: a test may have already cleaned up.
    }
  }
  createdRoots.length = 0
})

describe('createFixtures with instance ID', () => {
  it('places the fixture root under /tmp/cmdr-e2e-fixtures-<instance>-<ts>/', () => {
    const root = track(createFixtures('test-isolated-1'))
    expect(root.startsWith('/tmp/cmdr-e2e-fixtures-test-isolated-1-')).toBe(true)
  })

  it('creates the text files as real copies', () => {
    const root = track(createFixtures('test-isolated-2'))
    const fileA = path.join(root, 'left/file-a.txt')
    expect(fs.existsSync(fileA)).toBe(true)
    // Text file is a regular file, not a hardlink (nlink === 1)
    expect(fs.statSync(fileA).nlink).toBe(1)
  })

  it('hardlinks bulk .dat files from the shared cache', () => {
    const root = track(createFixtures('test-isolated-3'))
    const bulkFile = path.join(root, 'left/bulk/large-1.dat')
    const cacheFile = path.join(getCacheRoot(), 'left/bulk/large-1.dat')

    expect(fs.existsSync(bulkFile)).toBe(true)
    expect(fs.existsSync(cacheFile)).toBe(true)

    // Same inode means it's a hardlink, not a copy
    const bulkStat = fs.statSync(bulkFile)
    const cacheStat = fs.statSync(cacheFile)
    expect(bulkStat.ino).toBe(cacheStat.ino)
    // nlink >= 2 (cache + at least this fixture)
    expect(bulkStat.nlink).toBeGreaterThanOrEqual(2)
  })

  it('reuses the cache across two fixture roots (same inode)', () => {
    const root1 = track(createFixtures('test-shared-1'))
    const root2 = track(createFixtures('test-shared-2'))

    const file1 = path.join(root1, 'left/bulk/medium-01.dat')
    const file2 = path.join(root2, 'left/bulk/medium-01.dat')

    expect(fs.statSync(file1).ino).toBe(fs.statSync(file2).ino)
  })
})

describe('createFixtures without instance ID (legacy)', () => {
  it('falls back to /tmp/cmdr-e2e-<ts>/ and regenerates bulk files in-place', () => {
    const root = track(createFixtures())
    expect(root.startsWith('/tmp/cmdr-e2e-')).toBe(true)
    expect(root.startsWith('/tmp/cmdr-e2e-fixtures-')).toBe(false)

    const bulkFile = path.join(root, 'left/bulk/large-1.dat')
    expect(fs.existsSync(bulkFile)).toBe(true)
    // nlink === 1 confirms it's a fresh copy, not a hardlink
    expect(fs.statSync(bulkFile).nlink).toBe(1)
  })
})

describe('ensureCacheBuilt', () => {
  it('is idempotent: a second call when the cache exists is a no-op', () => {
    ensureCacheBuilt()
    const cacheRoot = getCacheRoot()
    expect(fs.existsSync(cacheRoot)).toBe(true)

    const mtimeBefore = fs.statSync(cacheRoot).mtimeMs
    ensureCacheBuilt()
    const mtimeAfter = fs.statSync(cacheRoot).mtimeMs
    // The cache dir mtime stays put: no rebuild happened.
    expect(mtimeAfter).toBe(mtimeBefore)
  })

  it('builds bulk .dat files of the expected sizes', () => {
    ensureCacheBuilt()
    const cacheRoot = getCacheRoot()
    expect(fs.statSync(path.join(cacheRoot, 'left/bulk/large-1.dat')).size).toBe(50 * 1024 * 1024)
    expect(fs.statSync(path.join(cacheRoot, 'left/bulk/medium-01.dat')).size).toBe(1 * 1024 * 1024)
  })
})

describe('cleanupFixtures safety guard', () => {
  it('refuses paths outside /tmp/cmdr-e2e-', () => {
    expect(() => {
      cleanupFixtures('/tmp/some-other-dir')
    }).toThrow(/Refusing to delete/)
  })

  it('accepts both legacy and per-instance prefixes', () => {
    const legacy = `/tmp/cmdr-e2e-${String(Date.now())}-legacy-test`
    fs.mkdirSync(legacy)
    cleanupFixtures(legacy)
    expect(fs.existsSync(legacy)).toBe(false)

    const perInstance = `/tmp/cmdr-e2e-fixtures-cleanup-test-${String(Date.now())}`
    fs.mkdirSync(perInstance)
    cleanupFixtures(perInstance)
    expect(fs.existsSync(perInstance)).toBe(false)
  })
})

describe('cache torn-write recovery', () => {
  it('rebuilds when a bulk file is shorter than expected, without touching a live fixture tree', () => {
    ensureCacheBuilt()
    const cacheRoot = getCacheRoot()
    const torn = path.join(cacheRoot, 'left/bulk/large-1.dat')

    // A bystander tree that hardlinked the cache BEFORE the tear, standing in
    // for the E2E shards running on this machine right now.
    const bystander = path.join(track(createFixtures('test-torn-bystander')), 'left/bulk/large-1.dat')

    // Simulate a torn write. REPLACE the cache entry, never `truncateSync` it:
    // every live fixture tree hardlinks this inode, so an in-place write
    // shortens `left/bulk/large-1.dat` under every E2E run on the machine at
    // once, and the leak guard then blames whichever spec happened to be
    // finishing.
    fs.rmSync(torn)
    fs.writeFileSync(torn, Buffer.alloc(1024))
    expect(fs.statSync(torn).size).toBe(1024)
    expect(fs.statSync(bystander).size).toBe(50 * 1024 * 1024)

    // Force the cache to rebuild by removing it (ensureCacheBuilt won't
    // rebuild on partial corruption alone in the happy hot path: we treat
    // cacheIsValid as the gate, and removing the dir simulates "another
    // process spotted the corruption and wiped it"). The point: a fresh
    // build restores the expected size.
    execSync(`rm -rf "${cacheRoot}"`)
    ensureCacheBuilt()
    expect(fs.statSync(torn).size).toBe(50 * 1024 * 1024)
    expect(fs.statSync(bystander).size).toBe(50 * 1024 * 1024)
  })

  it('replaces a cache that is present but short, rather than hardlinking from it', () => {
    // The tear a crashed builder leaves behind: the directory is there, so no
    // `renameSync` can land on top of it, but a bulk file is short. Nothing
    // else on the machine ever wipes this, so a cache that can't recover here
    // hands every later E2E run a fixture tree the leak guard fails.
    ensureCacheBuilt()
    const short = path.join(getCacheRoot(), 'left/bulk/large-1.dat')
    fs.rmSync(short)
    fs.writeFileSync(short, Buffer.alloc(1024))

    ensureCacheBuilt()

    expect(fs.statSync(short).size).toBe(50 * 1024 * 1024)
    const fresh = path.join(track(createFixtures('test-cache-repaired')), 'left/bulk/large-1.dat')
    expect(fs.statSync(fresh).size).toBe(50 * 1024 * 1024)
  })
})
