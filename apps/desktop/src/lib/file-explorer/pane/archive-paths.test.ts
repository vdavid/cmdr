/**
 * Tests for the archive-path vocabulary: the suffix tables and the two boundary
 * checks (`pathCrossesArchiveBoundary` / `pathInsideArchive`), plus the two name
 * helpers built on them. All pure string checks, no mocks needed.
 */

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, it, expect } from 'vitest'
import {
  pathCrossesArchiveBoundary,
  pathInsideArchive,
  archiveNameFromPath,
  SUPPORTED_ARCHIVE_SUFFIXES,
  WRITABLE_ARCHIVE_SUFFIXES,
} from './archive-paths'

describe('pathCrossesArchiveBoundary — the WIDE, navigate-into check', () => {
  it('is true at the archive root and anywhere inside it', () => {
    expect(pathCrossesArchiveBoundary('/a/foo.zip')).toBe(true) // the archive root itself
    expect(pathCrossesArchiveBoundary('/a/foo.zip/inner')).toBe(true)
    expect(pathCrossesArchiveBoundary('/a/foo.zip/inner/deep/file.txt')).toBe(true)
  })

  it('is false for a plain folder that merely CONTAINS an archive', () => {
    // The pane is at `/a`, listing `foo.zip` as a row — not inside it.
    expect(pathCrossesArchiveBoundary('/a')).toBe(false)
    expect(pathCrossesArchiveBoundary('/a/b/c')).toBe(false)
  })

  it('matches the case-insensitive extension, and any component (nested leftmost)', () => {
    expect(pathCrossesArchiveBoundary('/a/foo.ZIP/inner')).toBe(true)
    expect(pathCrossesArchiveBoundary('/a/archive.name.zip')).toBe(true)
    // Leftmost archive component makes the whole path cross; the inner b.zip
    // is a plain entry the FE can't distinguish, but the answer (true) is right.
    expect(pathCrossesArchiveBoundary('/a.zip/b.zip/x')).toBe(true)
  })

  it('is NOT decidable-true for a component whose extension is not an archive', () => {
    // `foo.zip.txt`: final extension is txt, so the STRING doesn't cross a boundary.
    expect(pathCrossesArchiveBoundary('/a/foo.zip.txt')).toBe(false)
    // A leading-dot dotfile has no stem, so `.zip` is not an extension.
    expect(pathCrossesArchiveBoundary('/a/.zip')).toBe(false)
    // No dot at all.
    expect(pathCrossesArchiveBoundary('/a/zip/file')).toBe(false)
  })

  it('a real directory literally named foo.zip is NOT decidable here (backend corrects)', () => {
    // Extension-only: the FE reads this as inside-an-archive. The backend
    // stat+magic check corrects a real directory to plain navigation; the FE only
    // uses this for read-only gating, where the false positive is safe.
    expect(pathCrossesArchiveBoundary('/a/foo.zip/anything')).toBe(true)
  })
})

/**
 * The narrow half of the split, and the reason the split exists: a site that
 * operates ON a path (preview it, move it, rename it) must treat the `.zip` file
 * ITSELF as an ordinary file, exactly as the backend's `path_is_inside_archive`
 * does. Only a path with a non-empty INNER part is unreachable through `std::fs`.
 */
describe('pathInsideArchive — the NARROW, operate-on check', () => {
  it('is false for the archive file itself, true only for something inside it', () => {
    // The distinction the whole split turns on.
    expect(pathInsideArchive('/a/foo.zip')).toBe(false)
    expect(pathInsideArchive('/a/foo.zip/inner')).toBe(true)
    expect(pathInsideArchive('/a/foo.zip/inner/deep/file.txt')).toBe(true)
  })

  it('is false for ordinary paths, archive-free or not', () => {
    expect(pathInsideArchive('/a')).toBe(false)
    expect(pathInsideArchive('/a/b/c')).toBe(false)
    expect(pathInsideArchive('/a/notes.txt')).toBe(false)
  })

  it('reads a trailing slash as still being AT the archive root', () => {
    // `/a/foo.zip/` names the archive, not an entry in it: the inner path is empty.
    // Splitting on '/' leaves a trailing empty segment, which must not read as one.
    expect(pathInsideArchive('/a/foo.zip/')).toBe(false)
  })

  it('uses the leftmost boundary, matching the wide check and the backend', () => {
    // The outer `.zip` is the boundary, so `b.zip` is an entry inside it.
    expect(pathInsideArchive('/a.zip/b.zip')).toBe(true)
    expect(pathInsideArchive('/a/foo.ZIP/inner')).toBe(true)
  })

  it('agrees with the wide check everywhere except at the archive root', () => {
    // Pins the exact shape of the difference, so neither can drift into the other.
    for (const path of ['/a', '/a/b/c', '/a/foo.zip/inner', '/a/foo.zip.txt']) {
      expect(pathInsideArchive(path), path).toBe(pathCrossesArchiveBoundary(path))
    }
    expect(pathCrossesArchiveBoundary('/a/foo.zip')).toBe(true)
    expect(pathInsideArchive('/a/foo.zip')).toBe(false)
  })
})

/**
 * The two suffix tables are one decision written twice, in two languages, and
 * nothing but this test makes them agree. Drift is silent and asymmetric: a
 * suffix the backend browses but the FE doesn't know is a pane whose write
 * affordances stay ON inside a read-only container, which is the direction that
 * costs a user their data.
 *
 * Reading the Rust source is the same trick `archive-enter-policy.test.ts` uses
 * to pin a claim about a file it can't import.
 */
describe('archive suffix table ↔ the backend`s `format_for_name`', () => {
  it('knows exactly the suffixes the Rust table does', () => {
    const rust = readFileSync(resolve(process.cwd(), '../../crates/cmdr-fs/src/archive_format.rs'), 'utf8')
    // The `SUFFIXES` const's entries: `(".tar.gz", ArchiveFormat::…)`.
    const table = rust.slice(rust.indexOf('const SUFFIXES'), rust.indexOf('];', rust.indexOf('const SUFFIXES')))
    const backendSuffixes = [...table.matchAll(/\("(\.[^"]+)"/g)].map((m) => m[1])
    expect(backendSuffixes.length, 'failed to parse the Rust SUFFIXES table').toBeGreaterThan(10)

    for (const suffix of backendSuffixes) {
      expect(SUPPORTED_ARCHIVE_SUFFIXES, `backend browses ${suffix}, the FE does not`).toContain(suffix)
    }
    expect([...SUPPORTED_ARCHIVE_SUFFIXES].sort()).toEqual([...backendSuffixes].sort())
  })

  it('treats every document container as browsable but NOT writable', () => {
    // The asymmetry that makes browsing a `.docx` safe to offer at all.
    for (const suffix of ['.docx', '.xlsx', '.pptx', '.jar', '.apk']) {
      expect(SUPPORTED_ARCHIVE_SUFFIXES, suffix).toContain(suffix)
      expect(WRITABLE_ARCHIVE_SUFFIXES, suffix).not.toContain(suffix)
    }
    expect(WRITABLE_ARCHIVE_SUFFIXES).toEqual(['.zip'])
  })
})

describe('pathInsideArchive — tar family and 7z', () => {
  it('recognizes the compressed-tar suffixes and 7z', () => {
    for (const name of [
      'a.tar',
      'a.tar.gz',
      'a.tgz',
      'a.tar.xz',
      'a.txz',
      'a.tar.bz2',
      'a.tbz2',
      'a.tar.zst',
      'a.7z',
    ]) {
      expect(pathInsideArchive(`/dir/${name}/inner`), name).toBe(true)
    }
  })

  it('does NOT treat a bare compressed file (not a tar) as an archive', () => {
    expect(pathInsideArchive('/dir/photo.gz')).toBe(false)
    expect(pathInsideArchive('/dir/data.zst/x')).toBe(false)
  })
})

describe('archiveNameFromPath — the archive display name for a prompt', () => {
  it('returns the archive segment for the archive root and any inner path', () => {
    expect(archiveNameFromPath('/a/photos.zip')).toBe('photos.zip')
    expect(archiveNameFromPath('/a/photos.zip/inner/x.jpg')).toBe('photos.zip')
  })

  it('picks the LEFTMOST archive segment (outer archive governs)', () => {
    expect(archiveNameFromPath('/x/foo.tar/bar.zip/y')).toBe('foo.tar')
  })

  it('falls back to the basename when no segment is an archive', () => {
    expect(archiveNameFromPath('/a/b/c.txt')).toBe('c.txt')
  })
})
