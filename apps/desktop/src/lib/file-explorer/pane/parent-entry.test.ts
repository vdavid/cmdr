/**
 * Tests for `parent-entry.ts`, the synthetic `..` row a pane shows above its
 * listing. It has to look enough like a real `FileEntry` for the list rows, the
 * selection index math, and the entries snapshot to treat it uniformly, while
 * pointing at the PARENT of the current directory.
 */
import { describe, it, expect } from 'vitest'
import { toCanonical } from '$lib/path/canonical'
import { createParentEntry } from './parent-entry'

const canonical = (path: string) => toCanonical(path, '/Users/test')

describe('createParentEntry', () => {
  it('points at the parent of the current directory', () => {
    const entry = createParentEntry(canonical('/Users/test/Documents/notes'))
    expect(entry?.path).toBe('/Users/test/Documents')
  })

  it('is named `..` and reads as a directory', () => {
    const entry = createParentEntry(canonical('/Users/test/Documents'))
    expect(entry?.name).toBe('..')
    expect(entry?.isDirectory).toBe(true)
    expect(entry?.isSymlink).toBe(false)
    expect(entry?.iconId).toBe('dir')
  })

  it('counts as fully loaded, so no row ever waits on extended metadata for it', () => {
    const entry = createParentEntry(canonical('/Users/test'))
    expect(entry?.extendedMetadataLoaded).toBe(true)
  })

  it('has no parent at the filesystem root', () => {
    expect(createParentEntry(canonical('/'))).toBeNull()
  })

  it('resolves the parent one level below the root', () => {
    expect(createParentEntry(canonical('/Volumes'))?.path).toBe('/')
  })
})
