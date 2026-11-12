import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { logLevelFor } from './transfer-error-log-level'
import { transferErrorFixtures } from '#lib/dialog-gallery/fixtures/transfer-error.js'
import type { WriteOperationError } from '#lib/file-explorer/types.js'

const permissionDenied: WriteOperationError = {
  type: 'permission_denied',
  path: '/private/var/db/thing',
  message: 'Operation not permitted',
  errno: 1,
  refusal: 'unclassified',
  refusedFolder: null,
  side: null,
}
const writeError: WriteOperationError = { type: 'write_error', path: '/dst/a', message: 'disk said no' }

describe('logLevelFor', () => {
  // ERR-4K2HT: a refused delete in a system folder logged at error and auto-sent a report.
  it('logs an expected refusal at warn, so it never auto-sends a report', () => {
    expect(logLevelFor(permissionDenied)).toBe('warn')
    expect(logLevelFor({ type: 'insufficient_space', required: 2, available: 1, volumeName: 'Disk' })).toBe('warn')
    expect(logLevelFor({ type: 'delete_pending', path: '/p' })).toBe('warn')
  })

  // ERR-XFP5T (cmdr-reports#111): a file open elsewhere on a share refused three copies,
  // and their error lines auto-sent a report.
  it('logs a file in use at warn, on its own and as a move’s cause', () => {
    const inUse: WriteOperationError = { type: 'file_in_use', path: '/Volumes/share/report.pdf' }
    expect(logLevelFor(inUse)).toBe('warn')
    expect(logLevelFor({ type: 'source_not_removed', path: '/a', landedAt: '/b', cause: inUse })).toBe('warn')
  })

  it('keeps the serious kinds at error', () => {
    expect(logLevelFor(writeError)).toBe('error')
    expect(logLevelFor({ type: 'read_error', path: '/p', message: 'm' })).toBe('error')
    expect(logLevelFor({ type: 'io_error', path: '/p', message: 'm' })).toBe('error')
  })

  it('takes a wrapper’s level from its cause', () => {
    expect(logLevelFor({ type: 'new_files_kept', cause: writeError, kept: [] })).toBe('error')
    expect(logLevelFor({ type: 'new_files_kept', cause: permissionDenied, kept: [] })).toBe('warn')
    expect(logLevelFor({ type: 'originals_kept_aside', cause: writeError, recovered: [] })).toBe('error')
    expect(logLevelFor({ type: 'source_not_removed', path: '/a', landedAt: '/b', cause: permissionDenied })).toBe(
      'warn',
    )
  })
})

/**
 * The ONE rule, shared with the backend's `WriteOperationError::is_expected_refusal`
 * (`crates/cmdr-operations/src/error_classification.rs`) through this file: a sample of
 * every variant and the level it earns. The backend logs a failed copy at error only when
 * this side would, so an expected refusal never auto-sends a report from either
 * (cmdr-reports#99, #111).
 */
describe('the log levels shared with the backend', () => {
  const shared = JSON.parse(
    readFileSync(resolve(process.cwd(), '../../crates/cmdr-operations/src/write_error_log_levels.json'), 'utf8'),
  ) as { cases: { level: 'warn' | 'error'; error: WriteOperationError }[] }

  it.each(shared.cases.map((c) => [c.error.type, c.level, c] as const))('logs %s at %s', (_type, level, c) => {
    expect(logLevelFor(c.error)).toBe(level)
  })

  // The gallery keeps one fixture per variant, keyed by its tag (a `Record` over the
  // union), so a variant with no shared sample fails here before the two sides can drift.
  it('holds a sample of every variant', () => {
    const sampled = new Set(shared.cases.map((c) => c.error.type))
    const variants = Object.entries(transferErrorFixtures)
      .filter(([id, fixture]) => fixture?.error.type === id)
      .map(([id]) => id)
    expect(variants.length).toBeGreaterThan(50)
    expect(variants.filter((v) => !sampled.has(v as WriteOperationError['type']))).toEqual([])
  })
})
