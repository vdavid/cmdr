import { describe, it, expect } from 'vitest'
import { isVirtualGitPath } from './path-detection'

describe('isVirtualGitPath', () => {
  it('treats the `.git` directory itself as real', () => {
    expect(isVirtualGitPath('/Users/me/repo/.git')).toBe(false)
  })

  it('treats raw `.git` internals as real', () => {
    // `.git/HEAD` and `.git/refs/...` are real on-disk files. Only the
    // six category names route to the virtual portal.
    expect(isVirtualGitPath('/Users/me/repo/.git/HEAD')).toBe(false)
    expect(isVirtualGitPath('/Users/me/repo/.git/refs/heads/main')).toBe(false)
    expect(isVirtualGitPath('/Users/me/repo/.git/objects/pack')).toBe(false)
    expect(isVirtualGitPath('/Users/me/repo/.git/config')).toBe(false)
  })

  it('detects each of the six virtual categories', () => {
    const root = '/Users/me/repo/.git'
    expect(isVirtualGitPath(`${root}/branches`)).toBe(true)
    expect(isVirtualGitPath(`${root}/tags`)).toBe(true)
    expect(isVirtualGitPath(`${root}/commits`)).toBe(true)
    expect(isVirtualGitPath(`${root}/stash`)).toBe(true)
    expect(isVirtualGitPath(`${root}/worktrees`)).toBe(true)
    expect(isVirtualGitPath(`${root}/submodules`)).toBe(true)
  })

  it('detects subpaths inside a virtual category', () => {
    expect(isVirtualGitPath('/Users/me/repo/.git/branches/main')).toBe(true)
    expect(isVirtualGitPath('/Users/me/repo/.git/branches/main/src/foo.rs')).toBe(true)
    expect(isVirtualGitPath('/Users/me/repo/.git/branches/feature/foo')).toBe(true)
    expect(isVirtualGitPath('/Users/me/repo/.git/tags/v1.0.0')).toBe(true)
    expect(isVirtualGitPath('/Users/me/repo/.git/commits/abc123/Cargo.toml')).toBe(true)
  })

  it('treats `.git/raw/` as real, because the backend has no such category', () => {
    // There were seven categories once. `raw/` is gone, so a directory a
    // user actually made there is an ordinary folder, and the pane's
    // "does this still exist?" poll has to keep running for it.
    expect(isVirtualGitPath('/Users/me/repo/.git/raw')).toBe(false)
    expect(isVirtualGitPath('/Users/me/repo/.git/raw/HEAD')).toBe(false)
  })

  it('ignores paths that look similar but are not under `.git`', () => {
    expect(isVirtualGitPath('/Users/me/projects/branches/main')).toBe(false)
    expect(isVirtualGitPath('/Users/me/repo/git/branches/main')).toBe(false)
    expect(isVirtualGitPath('/Users/me/repo/.gitignore')).toBe(false)
  })

  it('returns false for normal paths', () => {
    expect(isVirtualGitPath('/Users/me/foo/bar')).toBe(false)
    expect(isVirtualGitPath('/')).toBe(false)
    expect(isVirtualGitPath('')).toBe(false)
  })

  it('does not match when the segment after `.git/` only starts with a category name', () => {
    // Exact category match required: `.git/branches-of-foo` is not virtual.
    expect(isVirtualGitPath('/Users/me/repo/.git/branches-of-foo')).toBe(false)
    expect(isVirtualGitPath('/Users/me/repo/.git/raw_logs')).toBe(false)
  })
})
