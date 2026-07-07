import { describe, it, expect } from 'vitest'
import type { RenameTarget } from './rename-state.svelte'
import { shouldMountRenameEditor } from './rename-mount'

function target(overrides: Partial<RenameTarget> = {}): RenameTarget {
  return {
    path: '/dir/file.txt',
    originalName: 'file.txt',
    parentPath: '/dir',
    index: 3,
    isDirectory: false,
    ...overrides,
  }
}

describe('shouldMountRenameEditor (index-based, current behavior)', () => {
  it('mounts on the row whose index matches the target index', () => {
    expect(shouldMountRenameEditor(target({ index: 3 }), { index: 3, path: '/dir/file.txt' })).toBe(true)
  })

  it('does not mount on a row with a different index', () => {
    expect(shouldMountRenameEditor(target({ index: 3 }), { index: 4, path: '/dir/file.txt' })).toBe(false)
  })

  it('does not mount when there is no target', () => {
    expect(shouldMountRenameEditor(null, { index: 3, path: '/dir/file.txt' })).toBe(false)
    expect(shouldMountRenameEditor(undefined, { index: 3, path: '/dir/file.txt' })).toBe(false)
  })
})
