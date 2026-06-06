/**
 * Tests for the pure transfer-dialog derivation helpers (path validation and
 * free-space formatting). No reactivity, no IPC — these are the testable
 * branches lifted out of `TransferDialog.svelte`.
 */
import { describe, it, expect } from 'vitest'
import { getPathValidationError, formatSpaceInfo } from './transfer-dialog-logic'
import type { VolumeSpaceInfo } from '$lib/tauri-commands'

describe('getPathValidationError', () => {
  it('returns null when the destination is unrelated to the sources', () => {
    expect(getPathValidationError(['/a/photos'], '/b/dest', 'copy')).toBeNull()
  })

  it('rejects copying a folder into itself', () => {
    expect(getPathValidationError(['/a/photos'], '/a/photos', 'copy')).toBe(
      `Can't copy "photos" into its own subfolder`,
    )
  })

  it('rejects copying a folder into its own subfolder', () => {
    expect(getPathValidationError(['/a/photos'], '/a/photos/sub', 'copy')).toBe(
      `Can't copy "photos" into its own subfolder`,
    )
  })

  it('uses the move verb for a move operation', () => {
    expect(getPathValidationError(['/a/photos'], '/a/photos', 'move')).toBe(
      `Can't move "photos" into its own subfolder`,
    )
  })

  it('rejects a destination that is the source own parent (already in this location)', () => {
    expect(getPathValidationError(['/a/photos'], '/a', 'copy')).toBe(`"photos" is already in this location`)
  })

  it('normalizes trailing slashes on both sides before comparing', () => {
    expect(getPathValidationError(['/a/photos/'], '/a/photos', 'copy')).toBe(
      `Can't copy "photos" into its own subfolder`,
    )
    expect(getPathValidationError(['/a/photos'], '/a/', 'copy')).toBe(`"photos" is already in this location`)
  })

  it('flags any matching source when several are given', () => {
    expect(getPathValidationError(['/a/notes.txt', '/a/photos'], '/a/photos/sub', 'copy')).toBe(
      `Can't copy "photos" into its own subfolder`,
    )
  })

  it('does not falsely match a sibling with a shared name prefix', () => {
    // "/a/photos2" must not count as inside "/a/photos".
    expect(getPathValidationError(['/a/photos'], '/a/photos2', 'copy')).toBeNull()
  })

  it('prioritizes the subfolder error over the already-in-location error', () => {
    // The destination equals the source AND (vacuously) is its own parent path
    // only in the subfolder branch; assert the subfolder branch wins for an
    // exact match.
    expect(getPathValidationError(['/a/photos'], '/a/photos', 'copy')).toContain('into its own subfolder')
  })
})

describe('formatSpaceInfo', () => {
  const fmt = (n: number): string => `${String(n)} B`

  it('returns an empty string when no space info is available', () => {
    expect(formatSpaceInfo(null, fmt)).toBe('')
  })

  it('formats free-of-total using the injected formatter', () => {
    const space: VolumeSpaceInfo = { availableBytes: 500, totalBytes: 1000 }
    expect(formatSpaceInfo(space, fmt)).toBe('500 B free of 1000 B')
  })
})
