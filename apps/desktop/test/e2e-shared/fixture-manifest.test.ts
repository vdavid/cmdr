/**
 * Unit tests for the shared-fixture drift guard (`fixture-manifest.ts`).
 *
 * The guard is what the E2E suite's global `afterEach` runs to name the spec
 * that dirtied `left/` + `right/`, so these tests pin the two properties that
 * make it worth having: it catches every mutation shape the specs make
 * (add / remove / rename / overwrite / archive edit / truncate), and its
 * repair touches ONLY the entries that drifted (an inode churn on an
 * untouched `sample.zip` is what broke the archive specs the last time a
 * blunt restore was tried).
 */

import { describe, it, expect, afterEach } from 'vitest'
import fs from 'fs'
import path from 'path'
import { createFixtures, cleanupFixtures } from './fixtures.js'
import { diffFixtureTree, describeFixtureTreeDiff, restoreFixtureTree } from './fixture-manifest.js'

const createdRoots: string[] = []

function makeRoot(id: string): string {
  const root = createFixtures(`test-manifest-${id}`)
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

describe('diffFixtureTree', () => {
  it('reports no drift on a freshly created tree', () => {
    expect(diffFixtureTree(makeRoot('clean'))).toBeNull()
  })

  it('catches a file copied into right/', () => {
    const root = makeRoot('added')
    fs.writeFileSync(path.join(root, 'right/file-a.txt'), 'A'.repeat(1024))

    const diff = diffFixtureTree(root)
    expect(diff?.added).toEqual(['right/file-a.txt'])
  })

  it('catches a deleted file', () => {
    const root = makeRoot('removed')
    fs.rmSync(path.join(root, 'left/file-b.txt'))

    const diff = diffFixtureTree(root)
    expect(diff?.removed).toEqual(['left/file-b.txt'])
  })

  it('catches a rename as one removal plus one addition', () => {
    const root = makeRoot('renamed')
    fs.renameSync(path.join(root, 'left/file-a.txt'), path.join(root, 'left/renamed-file.txt'))

    const diff = diffFixtureTree(root)
    expect(diff?.removed).toEqual(['left/file-a.txt'])
    expect(diff?.added).toEqual(['left/renamed-file.txt'])
  })

  it('catches an overwrite that keeps the size (the conflict specs write same-length text)', () => {
    const root = makeRoot('overwritten')
    fs.writeFileSync(path.join(root, 'left/file-a.txt'), 'B'.repeat(1024))

    const diff = diffFixtureTree(root)
    expect(diff?.changed).toEqual(['left/file-a.txt'])
  })

  it('catches an edit to sample.zip (the archive specs mutate the zip in place)', () => {
    const root = makeRoot('archive')
    fs.appendFileSync(path.join(root, 'left/sample.zip'), 'trailing')

    const diff = diffFixtureTree(root)
    expect(diff?.changed).toEqual(['left/sample.zip'])
  })

  it('catches a shortened bulk .dat by size', () => {
    const root = makeRoot('bulk')
    // Replace, never truncate: the bulk files are HARDLINKS into the shared
    // `/tmp/cmdr-e2e-fixtures-cache/`, so an in-place write corrupts the cache
    // for every other E2E run on this machine.
    const bulk = path.join(root, 'left/bulk/medium-01.dat')
    fs.rmSync(bulk)
    fs.writeFileSync(bulk, 'short')

    const diff = diffFixtureTree(root)
    expect(diff?.changed).toEqual(['left/bulk/medium-01.dat'])
  })

  it('catches a file replaced by a directory of the same name', () => {
    const root = makeRoot('kind')
    fs.rmSync(path.join(root, 'left/file-a.txt'))
    fs.mkdirSync(path.join(root, 'left/file-a.txt'))

    const diff = diffFixtureTree(root)
    expect(diff?.changed).toEqual(['left/file-a.txt'])
  })

  it('catches a leftover symlink', () => {
    const root = makeRoot('symlink')
    fs.symlinkSync(path.join(root, 'left/file-a.txt'), path.join(root, 'left/my-link'))

    const diff = diffFixtureTree(root)
    expect(diff?.added).toEqual(['left/my-link'])
  })

  it('catches a missing directory', () => {
    const root = makeRoot('missing-dir')
    fs.rmSync(path.join(root, 'left/sub-dir'), { recursive: true })

    const diff = diffFixtureTree(root)
    expect(diff?.removed).toContain('left/sub-dir')
    expect(diff?.removed).toContain('left/sub-dir/nested-file.txt')
  })

  it('ignores spec-owned fixture dirs beside left/ and right/', () => {
    const root = makeRoot('sibling')
    fs.mkdirSync(path.join(root, 'brief-cursor-fixtures'))
    fs.writeFileSync(path.join(root, 'brief-cursor-fixtures/a-00.txt'), 'A')

    expect(diffFixtureTree(root)).toBeNull()
  })
})

describe('describeFixtureTreeDiff', () => {
  it('names each drifted path under its own heading', () => {
    const root = makeRoot('describe')
    fs.writeFileSync(path.join(root, 'right/copied.txt'), 'x')
    fs.rmSync(path.join(root, 'left/file-b.txt'))
    fs.writeFileSync(path.join(root, 'left/file-a.txt'), 'B'.repeat(1024))

    const diff = diffFixtureTree(root)
    expect(diff).not.toBeNull()
    const text = describeFixtureTreeDiff(diff ?? { added: [], removed: [], changed: [] })
    expect(text).toContain('added: right/copied.txt')
    expect(text).toContain('removed: left/file-b.txt')
    expect(text).toContain('changed: left/file-a.txt')
  })
})

describe('restoreFixtureTree', () => {
  it('clears every drift shape in one pass', () => {
    const root = makeRoot('restore-all')
    fs.writeFileSync(path.join(root, 'right/copied.txt'), 'x')
    fs.mkdirSync(path.join(root, 'right/copied-dir/inner'), { recursive: true })
    fs.rmSync(path.join(root, 'left/file-b.txt'))
    fs.rmSync(path.join(root, 'left/sub-dir'), { recursive: true })
    fs.writeFileSync(path.join(root, 'left/file-a.txt'), 'B'.repeat(1024))
    fs.appendFileSync(path.join(root, 'left/sample.zip'), 'trailing')
    fs.rmSync(path.join(root, 'left/bulk/medium-01.dat'))

    restoreFixtureTree(root)

    expect(diffFixtureTree(root)).toBeNull()
  })

  it('leaves untouched entries on their original inode', () => {
    const root = makeRoot('restore-inodes')
    const zip = path.join(root, 'left/sample.zip')
    const right = path.join(root, 'right')
    const zipInodeBefore = fs.statSync(zip).ino
    const rightInodeBefore = fs.statSync(right).ino

    fs.writeFileSync(path.join(root, 'left/file-a.txt'), 'B'.repeat(1024))
    restoreFixtureTree(root)

    expect(fs.statSync(zip).ino).toBe(zipInodeBefore)
    expect(fs.statSync(right).ino).toBe(rightInodeBefore)
  })

  it('is a no-op on a clean tree (no inode churn at all)', () => {
    const root = makeRoot('restore-clean')
    const before = fs.readdirSync(path.join(root, 'left')).map((n) => fs.lstatSync(path.join(root, 'left', n)).ino)

    restoreFixtureTree(root)

    const after = fs.readdirSync(path.join(root, 'left')).map((n) => fs.lstatSync(path.join(root, 'left', n)).ino)
    expect(after).toEqual(before)
  })

  it('removes a dangling symlink left behind by a conflict spec', () => {
    const root = makeRoot('restore-symlink')
    fs.symlinkSync(path.join(root, 'left/gone.txt'), path.join(root, 'left/dangling'))

    restoreFixtureTree(root)

    expect(diffFixtureTree(root)).toBeNull()
  })
})
