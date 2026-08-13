/**
 * Tests for the tracked-artifact guard in `capture-runtime.ts`.
 *
 * The guard exists because a capture run rewrites files git TRACKS while it goes,
 * and a run that dies halfway used to leave that rewrite behind: a capture that
 * lost four surfaces rewrote `capture-report.json` without them, and the
 * `message-screenshots-fresh` check then validated couplings against a report no
 * complete run ever produced. The launch primitives beside it need a real machine;
 * this part is pure filesystem work, so it gets real tests.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createTrackedArtifactGuard } from './capture-runtime.ts'

let dir: string

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'cmdr-tracked-artifact-guard-'))
})

afterEach(() => {
  rmSync(dir, { recursive: true, force: true })
})

function write(name: string, content: string): void {
  writeFileSync(join(dir, name), content)
}

function read(name: string): string | null {
  const path = join(dir, name)
  return existsSync(path) ? readFileSync(path, 'utf8') : null
}

describe('createTrackedArtifactGuard', () => {
  it('rolls a run’s rewrite back when the run never earned it', () => {
    write('report.json', 'the complete run')
    const guard = createTrackedArtifactGuard(dir, ['report.json'])
    guard.snapshot()

    write('report.json', 'a partial run, four surfaces short')
    guard.restoreUnlessEarned()

    expect(read('report.json')).toBe('the complete run')
  })

  it('keeps the rewrite once the run earns it', () => {
    write('report.json', 'the previous run')
    const guard = createTrackedArtifactGuard(dir, ['report.json'])
    guard.snapshot()

    write('report.json', 'a complete, green run')
    guard.earn()
    guard.restoreUnlessEarned()

    expect(read('report.json')).toBe('a complete, green run')
  })

  it('removes a file the failed run created, rather than leaving an orphan', () => {
    const guard = createTrackedArtifactGuard(dir, ['report.json'])
    guard.snapshot()

    write('report.json', 'written by a run that then died')
    guard.restoreUnlessEarned()

    expect(read('report.json')).toBeNull()
  })

  it('reports only what it actually put back, so a no-op run stays quiet', () => {
    write('report.json', 'unchanged')
    write('skipped.json', 'changed')
    const guard = createTrackedArtifactGuard(dir, ['report.json', 'skipped.json'])
    guard.snapshot()

    write('skipped.json', 'changed by the failed run')

    expect(guard.restoreUnlessEarned()).toEqual([join(dir, 'skipped.json')])
  })

  it('does nothing without a snapshot, so an interrupted run cannot invent one', () => {
    write('report.json', 'never snapshotted')
    const guard = createTrackedArtifactGuard(dir, ['report.json'])

    expect(guard.restoreUnlessEarned()).toEqual([])
    expect(read('report.json')).toBe('never snapshotted')
  })

  it('restores once, so a second exit handler cannot undo a later write', () => {
    write('report.json', 'the complete run')
    const guard = createTrackedArtifactGuard(dir, ['report.json'])
    guard.snapshot()

    write('report.json', 'a partial run')
    guard.restoreUnlessEarned()
    write('report.json', 'someone else, afterwards')

    expect(guard.restoreUnlessEarned()).toEqual([])
    expect(read('report.json')).toBe('someone else, afterwards')
  })

  it('survives a directory that has gone away, since a failed restore is only a dirty file', () => {
    write('report.json', 'the complete run')
    const guard = createTrackedArtifactGuard(dir, ['report.json'])
    guard.snapshot()
    rmSync(dir, { recursive: true, force: true })

    expect(() => guard.restoreUnlessEarned()).not.toThrow()

    mkdirSync(dir, { recursive: true })
  })
})
