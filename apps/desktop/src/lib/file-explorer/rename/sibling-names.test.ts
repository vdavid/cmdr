/**
 * The directory's names, read once per chain.
 *
 * The paging read is the expensive part (a 100k-file directory is 200 IPC round
 * trips), so what these tests pin is that a chain pays for it once and that the
 * list stays true to the directory as the chain's own renames land.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/tauri-commands', () => ({ getFileRange: vi.fn() }))

import { getFileRange } from '$lib/tauri-commands'
import { createSiblingNames, type ListingScope } from './sibling-names'

const DIR: ListingScope = { listingId: 'lst-1', includeHidden: false, parentPath: '/dir', totalCount: 3 }

/** Answers a paging read with `names`, one batch per call. */
function listingOf(names: string[]) {
  vi.mocked(getFileRange).mockImplementation((_id, start, count) =>
    Promise.resolve(names.slice(start, start + count).map((name) => ({ name })) as never),
  )
}

beforeEach(() => {
  vi.clearAllMocks()
  listingOf(['a.txt', 'b.txt', 'c.txt'])
})

describe('the directory names a rename validates against', () => {
  it('reads the listing once and serves every later activation from what it read', async () => {
    const siblings = createSiblingNames()

    await siblings.ensure(DIR)
    await siblings.ensure(DIR)
    await siblings.ensure(DIR)

    expect(siblings.names).toEqual(['a.txt', 'b.txt', 'c.txt'])
    expect(getFileRange).toHaveBeenCalledTimes(1)
  })

  it('pages a big listing in batches, and still only once', async () => {
    const many = Array.from({ length: 1200 }, (_, i) => `f${String(i)}.txt`)
    listingOf(many)
    const siblings = createSiblingNames()

    await siblings.ensure({ ...DIR, totalCount: 1200 })
    await siblings.ensure({ ...DIR, totalCount: 1200 })

    expect(siblings.names).toHaveLength(1200)
    expect(getFileRange).toHaveBeenCalledTimes(3)
  })

  it('reads again when the pane moved to another listing', async () => {
    const siblings = createSiblingNames()

    await siblings.ensure(DIR)
    listingOf(['x.txt'])
    await siblings.ensure({ listingId: 'lst-2', includeHidden: false, parentPath: '/other', totalCount: 1 })

    expect(siblings.names).toEqual(['x.txt'])
    expect(getFileRange).toHaveBeenCalledTimes(2)
  })

  it('reads again when hidden files were switched on', async () => {
    const siblings = createSiblingNames()

    await siblings.ensure(DIR)
    listingOf(['.dotfile', 'a.txt', 'b.txt', 'c.txt'])
    await siblings.ensure({ ...DIR, includeHidden: true, totalCount: 4 })

    expect(siblings.names).toContain('.dotfile')
    expect(getFileRange).toHaveBeenCalledTimes(2)
  })

  it('follows a rename that landed: the old name is gone, the new one is there', async () => {
    const siblings = createSiblingNames()
    await siblings.ensure(DIR)

    siblings.applyRename('/dir', 'a.txt', 'renamed.txt')

    expect(siblings.names).toEqual(['b.txt', 'c.txt', 'renamed.txt'])
  })

  it('ignores a rename that landed in a different directory', async () => {
    const siblings = createSiblingNames()
    await siblings.ensure(DIR)

    siblings.applyRename('/elsewhere', 'a.txt', 'renamed.txt')

    expect(siblings.names).toEqual(['a.txt', 'b.txt', 'c.txt'])
  })

  it('keeps a rename that landed while the read was still paging', async () => {
    const siblings = createSiblingNames()

    const read = siblings.ensure(DIR)
    siblings.applyRename('/dir', 'a.txt', 'renamed.txt')
    await read

    // The backend snapshot predates the rename, so replaying it is what keeps
    // the list true to the directory.
    expect(siblings.names).toEqual(['b.txt', 'c.txt', 'renamed.txt'])
  })

  it('drops what it read when the chain ends, so the next one starts fresh', async () => {
    const siblings = createSiblingNames()
    await siblings.ensure(DIR)

    siblings.clear()

    expect(siblings.names).toEqual([])
    await siblings.ensure(DIR)
    expect(getFileRange).toHaveBeenCalledTimes(2)
  })

  it('lets a read that outlived its chain go, rather than answering for the next one', async () => {
    const siblings = createSiblingNames()

    const abandoned = siblings.ensure(DIR)
    siblings.clear()
    listingOf(['x.txt'])
    await siblings.ensure({ ...DIR, totalCount: 1 })
    await abandoned

    expect(siblings.names).toEqual(['x.txt'])
  })

  it('gives no names when the listing cannot be read, instead of guessing', async () => {
    vi.mocked(getFileRange).mockRejectedValue(new Error('listing gone'))
    const siblings = createSiblingNames()

    await siblings.ensure(DIR)

    expect(siblings.names).toEqual([])
  })

  it('asks nothing of an empty listing', async () => {
    const siblings = createSiblingNames()

    await siblings.ensure({ ...DIR, totalCount: 0 })

    expect(siblings.names).toEqual([])
    expect(getFileRange).not.toHaveBeenCalled()
  })

  it('tries again once a listing with nothing to read has rows', async () => {
    const siblings = createSiblingNames()

    await siblings.ensure({ ...DIR, totalCount: 0 })
    await siblings.ensure(DIR)

    // Remembering the empty read would leave the hint blind for the whole chain.
    expect(siblings.names).toEqual(['a.txt', 'b.txt', 'c.txt'])
  })
})
