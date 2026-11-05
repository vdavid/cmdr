import { describe, expect, it } from 'vitest'
import { logLevelFor } from './transfer-error-log-level'
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
