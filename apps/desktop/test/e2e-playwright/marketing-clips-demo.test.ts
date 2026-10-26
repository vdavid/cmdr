/**
 * Tests for the clips' staged demo tree (`marketing-clips-demo.ts`), above all its one
 * safety rule: a folder without the marker is somebody's data and is never deleted.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

// Before the import: the module reads it once. Kilobytes instead of ~400 MB per staging.
process.env.CMDR_CLIPS_DEMO_SIZE_SCALE = '0.001'
const {
  BOUQUET_PHOTO,
  DEMO,
  MARKER,
  QUEUE_SOURCES,
  VIEWER_DOC,
  ensureDemoTree,
  removeDemoTree,
  resetBackup,
  resetRawImports,
  stageDemoTree,
} = await import('./marketing-clips-demo.js')

describe('the demo tree', () => {
  let parent = ''
  let root = ''

  beforeEach(() => {
    parent = mkdtempSync(join(tmpdir(), 'cmdr-clips-demo-'))
    root = join(parent, 'Studio')
  })

  afterEach(() => {
    rmSync(parent, { recursive: true, force: true })
  })

  it('refuses to delete a folder it did not stage', () => {
    mkdirSync(root)
    writeFileSync(join(root, 'thesis.docx'), 'irreplaceable')
    expect(() => {
      removeDemoTree(root)
    }).toThrow(/may be real data/)
    expect(existsSync(join(root, 'thesis.docx'))).toBe(true)
  })

  it('stages every folder the clips reach for, then keeps a current tree', () => {
    expect(ensureDemoTree(root)).toBe('staged')
    expect(ensureDemoTree(root)).toBe('kept')
    expect(existsSync(join(root, MARKER))).toBe(true)
    for (const dir of Object.values(DEMO)) expect(existsSync(join(root, dir)), dir).toBe(true)
    for (const project of QUEUE_SOURCES) expect(existsSync(join(root, DEMO.projects, project)), project).toBe(true)
    expect(statSync(join(root, DEMO.fieldNotes, BOUQUET_PHOTO.file)).size).toBeGreaterThan(100_000)
    expect(readFileSync(join(root, DEMO.fieldNotes, VIEWER_DOC), 'utf8')).toContain(BOUQUET_PHOTO.author)
  })

  it('restages a tree an older version staged', () => {
    stageDemoTree(root)
    writeFileSync(join(root, MARKER), 'Staged by an older pnpm marketing:clips.\n')
    expect(ensureDemoTree(root)).toBe('staged')
  })

  it('puts renamed files and copied ones back between takes, keeping the folders', () => {
    stageDemoTree(root)
    const raw = join(root, DEMO.rawImports)
    const rawInode = statSync(raw).ino
    const before = readdirSync(raw).sort()
    rmSync(join(raw, before[0]))
    writeFileSync(join(raw, 'Iceland 2026-08-14 001.jpg'), 'renamed')
    writeFileSync(join(root, DEMO.backup, 'IMG_4021.jpg'), 'copied')

    resetRawImports(root)
    resetBackup(root)

    expect(readdirSync(raw).sort()).toEqual(before)
    expect(readdirSync(join(root, DEMO.backup))).toEqual([])
    // The folder itself survives: a deleted-and-recreated folder loses its index sizes.
    expect(statSync(raw).ino).toBe(rawInode)
  })
})
