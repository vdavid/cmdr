/**
 * Tests for the compress-mode suggested-filename helper. Pure, no IPC, no
 * reactivity: the three filename cases the Transfer dialog's compress mode
 * relies on (single source, multi source, volume-root fallback).
 */
import { describe, it, expect } from 'vitest'
import { suggestCompressArchiveName } from './transfer-compress-name'

describe('suggestCompressArchiveName', () => {
  it('names a single folder source after its basename', () => {
    expect(suggestCompressArchiveName(['/a/photos'], '/a')).toBe('photos.zip')
  })

  it('keeps a single file source basename verbatim (extension not stripped)', () => {
    expect(suggestCompressArchiveName(['/a/report.pdf'], '/a')).toBe('report.pdf.zip')
  })

  it('gives a `.zip` source no special treatment (targets a new archive)', () => {
    expect(suggestCompressArchiveName(['/a/data.zip'], '/a')).toBe('data.zip.zip')
  })

  it('names a multi-selection after the source directory basename', () => {
    expect(suggestCompressArchiveName(['/a/photos/one.jpg', '/a/photos/two.jpg'], '/a/photos')).toBe('photos.zip')
  })

  it('falls back to the first selection basename when the source dir is a volume root', () => {
    // sourceFolderPath is the volume root ("/"), whose basename is empty.
    expect(suggestCompressArchiveName(['/one.jpg', '/two.jpg'], '/')).toBe('one.jpg.zip')
  })

  it('ignores trailing slashes on the source directory', () => {
    expect(suggestCompressArchiveName(['/a/photos/one', '/a/photos/two'], '/a/photos/')).toBe('photos.zip')
  })

  it('returns a safe default when the only source is a volume root', () => {
    expect(suggestCompressArchiveName(['/'], '/')).toBe('archive.zip')
  })
})
