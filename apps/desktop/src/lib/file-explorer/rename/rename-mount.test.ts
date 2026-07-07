import { describe, it, expect } from 'vitest'
import type { RenameTarget } from './rename-state.svelte'
import { shouldMountRenameEditor } from './rename-mount'

function target(overrides: Partial<RenameTarget> = {}): RenameTarget {
  return {
    path: '/dir/file.txt',
    originalName: 'file.txt',
    parentPath: '/dir',
    isDirectory: false,
    ...overrides,
  }
}

describe('shouldMountRenameEditor (path-based identity)', () => {
  it('mounts on the row whose path matches the target path', () => {
    expect(shouldMountRenameEditor(target({ path: '/dir/a' }), { path: '/dir/a' })).toBe(true)
  })

  it('does not mount on a row with a different path', () => {
    expect(shouldMountRenameEditor(target({ path: '/dir/a' }), { path: '/dir/b' })).toBe(false)
  })

  it('does not mount when there is no target', () => {
    expect(shouldMountRenameEditor(null, { path: '/dir/file.txt' })).toBe(false)
    expect(shouldMountRenameEditor(undefined, { path: '/dir/file.txt' })).toBe(false)
  })

  // A watcher diff that inserts or removes OTHER rows shifts every row's position but
  // never its path. Identity follows the file by path, so a different file that slid
  // into the target's old row does NOT get the editor, and the target keeps it after
  // moving. Pre-fix (index identity) both of these went the wrong way.
  it('does not mount on a different file that slid into the target old row', () => {
    expect(shouldMountRenameEditor(target({ path: '/dir/a' }), { path: '/dir/b' })).toBe(false)
  })

  it('still mounts on the target file after a diff moved it', () => {
    expect(shouldMountRenameEditor(target({ path: '/dir/a' }), { path: '/dir/a' })).toBe(true)
  })
})
