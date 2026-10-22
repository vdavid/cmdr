/**
 * Tests for the clips' staged demo tree (`marketing-clips-demo.ts`), above all its one
 * safety rule: a folder without the marker is somebody's data and is never deleted.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

// Before the import: the module reads it once. Kilobytes instead of ~400 MB per staging.
process.env.CMDR_CLIPS_DEMO_SIZE_SCALE = '0.001'
const { DEMO, MARKER, QUEUE_SOURCES, removeDemoTree, resetBackup, resetRawImports, stageDemoTree } =
  await import('./marketing-clips-demo.js')

const hasMagick = spawnSync('magick', ['-version']).status === 0

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

  it.skipIf(!hasMagick)('stages every folder the clips reach for, and can stage again over itself', () => {
    stageDemoTree(root)
    stageDemoTree(root)
    expect(existsSync(join(root, MARKER))).toBe(true)
    for (const dir of Object.values(DEMO)) expect(existsSync(join(root, dir)), dir).toBe(true)
    for (const project of QUEUE_SOURCES) expect(existsSync(join(root, DEMO.projects, project)), project).toBe(true)
    expect(existsSync(join(root, DEMO.fieldNotes, 'glacier.jpg'))).toBe(true)
  })

  it.skipIf(!hasMagick)('puts renamed files and copied ones back between takes', () => {
    stageDemoTree(root)
    const raw = join(root, DEMO.rawImports)
    const before = readdirSync(raw).sort()
    rmSync(join(raw, before[0]))
    writeFileSync(join(raw, 'Iceland 2026-08-14 001.jpg'), 'renamed')
    writeFileSync(join(root, DEMO.backup, 'IMG_4021.jpg'), 'copied')

    resetRawImports(root)
    resetBackup(root)

    expect(readdirSync(raw).sort()).toEqual(before)
    expect(readdirSync(join(root, DEMO.backup))).toEqual([])
  })
})
