import { describe, expect, it } from 'vitest'

import { NO_WALKED_GROUND, isPathAffectedByWalk } from './walked-ground'

describe('isPathAffectedByWalk', () => {
  it('flags a row inside the branch being walked', () => {
    const ground = ['/Users/someone/Downloads']

    expect(isPathAffectedByWalk(ground, '/Users/someone/Downloads/big')).toBe(true)
    expect(isPathAffectedByWalk(ground, '/Users/someone/Downloads/big/nested')).toBe(true)
  })

  it('flags the branch root itself', () => {
    const ground = ['/Users/someone/Downloads']

    expect(isPathAffectedByWalk(ground, '/Users/someone/Downloads')).toBe(true)
  })

  it('flags a row ABOVE the branch, because the roll-up repairs the ancestor chain', () => {
    // Walking `~/Downloads/big` moves the size shown for `~/Downloads` and for
    // `~` as well, so the test has to run both ways or every ancestor shows a
    // settled-looking size that is about to change.
    const ground = ['/Users/someone/Downloads/big']

    expect(isPathAffectedByWalk(ground, '/Users/someone/Downloads')).toBe(true)
    expect(isPathAffectedByWalk(ground, '/Users/someone')).toBe(true)
    expect(isPathAffectedByWalk(ground, '/Users')).toBe(true)
    expect(isPathAffectedByWalk(ground, '/')).toBe(true)
  })

  it('leaves a sibling alone', () => {
    const ground = ['/Users/someone/Downloads']

    expect(isPathAffectedByWalk(ground, '/Users/someone/Documents')).toBe(false)
    expect(isPathAffectedByWalk(ground, '/opt')).toBe(false)
    expect(isPathAffectedByWalk(ground, '/Users/someone/Documents/notes')).toBe(false)
  })

  it('compares whole path segments, so a same-prefix neighbour is not inside', () => {
    const ground = ['/Users/someone/Downloads']

    expect(isPathAffectedByWalk(ground, '/Users/someone/Downloads2')).toBe(false)
    expect(isPathAffectedByWalk(ground, '/Users/someone/Downloads-old/x')).toBe(false)
  })

  it('reads a walk of the volume root as covering everything on it', () => {
    const ground = ['/']

    expect(isPathAffectedByWalk(ground, '/')).toBe(true)
    expect(isPathAffectedByWalk(ground, '/opt')).toBe(true)
    expect(isPathAffectedByWalk(ground, '/Users/someone/Documents')).toBe(true)
  })

  it('flags every row while a run walks the volume whole', () => {
    // A full rebuild and every network scan announce the VOLUME ROOT as their
    // one walked root, so the same predicate covers them: no sentinel, no second
    // kind of run to branch on.
    expect(isPathAffectedByWalk(['/'], '/anywhere/at/all')).toBe(true)
    expect(isPathAffectedByWalk(['/Volumes/Backup'], '/Volumes/Backup/photos/2026')).toBe(true)
    expect(isPathAffectedByWalk(['/Volumes/Backup'], '/Users/someone')).toBe(false)
  })

  it('flags nothing when no walk is running', () => {
    expect(isPathAffectedByWalk(NO_WALKED_GROUND, '/Users/someone/Downloads')).toBe(false)
    expect(isPathAffectedByWalk([], '/Users/someone/Downloads')).toBe(false)
  })

  it('checks every announced branch, not only the first', () => {
    const ground = ['/opt/one', '/opt/two']

    expect(isPathAffectedByWalk(ground, '/opt/two/deep')).toBe(true)
  })

  it('ignores a trailing slash on either side', () => {
    expect(isPathAffectedByWalk(['/Users/someone/Downloads/'], '/Users/someone/Downloads/big')).toBe(true)
    expect(isPathAffectedByWalk(['/Users/someone/Downloads'], '/Users/someone/Downloads/')).toBe(true)
  })
})
