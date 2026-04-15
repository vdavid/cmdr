import { describe, it, expect, vi } from 'vitest'
import {
  ensureTrailingSlash,
  resolvePrivateSymlinks,
  hasDescendantUpdate,
  throttledRefresh,
  createIndexEventHandler,
} from './index-events'
import type { FilePaneAPI } from './types'

vi.mock('$lib/shortcuts/key-capture', () => ({
  isMacOS: () => true,
}))

describe('ensureTrailingSlash', () => {
  it('adds slash when missing', () => {
    expect(ensureTrailingSlash('/Users/test')).toBe('/Users/test/')
  })

  it('keeps existing trailing slash', () => {
    expect(ensureTrailingSlash('/Users/test/')).toBe('/Users/test/')
  })

  it('handles root path', () => {
    expect(ensureTrailingSlash('/')).toBe('/')
  })
})

describe('resolvePrivateSymlinks', () => {
  it('resolves /tmp to /private/tmp', () => {
    expect(resolvePrivateSymlinks('/tmp')).toBe('/private/tmp')
  })

  it('resolves /tmp/foo to /private/tmp/foo', () => {
    expect(resolvePrivateSymlinks('/tmp/foo')).toBe('/private/tmp/foo')
  })

  it('resolves /var to /private/var', () => {
    expect(resolvePrivateSymlinks('/var')).toBe('/private/var')
  })

  it('resolves /etc/hosts to /private/etc/hosts', () => {
    expect(resolvePrivateSymlinks('/etc/hosts')).toBe('/private/etc/hosts')
  })

  it('does not resolve /tmpfoo (no slash boundary)', () => {
    expect(resolvePrivateSymlinks('/tmpfoo')).toBe('/tmpfoo')
  })

  it('passes through non-symlink paths unchanged', () => {
    expect(resolvePrivateSymlinks('/Users/test')).toBe('/Users/test')
  })
})

describe('hasDescendantUpdate', () => {
  it('returns true when a path is a descendant of the dir', () => {
    expect(hasDescendantUpdate(['/Users/test/foo/'], '/Users/test/')).toBe(true)
  })

  it('returns false when the path is the dir itself', () => {
    expect(hasDescendantUpdate(['/Users/test/'], '/Users/test/')).toBe(false)
  })

  it('returns false when paths are unrelated', () => {
    expect(hasDescendantUpdate(['/other/path/'], '/Users/test/')).toBe(false)
  })

  it('handles paths without trailing slash', () => {
    expect(hasDescendantUpdate(['/Users/test/foo'], '/Users/test/')).toBe(true)
  })
})

/* eslint-disable @typescript-eslint/unbound-method -- vi.fn() mocks have no this binding */
describe('throttledRefresh', () => {
  it('fires immediately when not throttled', () => {
    const paneRef = { refreshIndexSizes: vi.fn() } as unknown as FilePaneAPI
    const setThrottle = vi.fn()
    throttledRefresh(true, 0, setThrottle, paneRef, 2000)
    expect(paneRef.refreshIndexSizes).toHaveBeenCalled()
    expect(setThrottle).toHaveBeenCalled()
  })

  it('skips when shouldRefresh is false', () => {
    const paneRef = { refreshIndexSizes: vi.fn() } as unknown as FilePaneAPI
    throttledRefresh(false, 0, vi.fn(), paneRef, 2000)
    expect(paneRef.refreshIndexSizes).not.toHaveBeenCalled()
  })

  it('skips when within cooldown period', () => {
    const paneRef = { refreshIndexSizes: vi.fn() } as unknown as FilePaneAPI
    const futureTime = Date.now() + 10_000
    throttledRefresh(true, futureTime, vi.fn(), paneRef, 2000)
    expect(paneRef.refreshIndexSizes).not.toHaveBeenCalled()
  })

  it('handles undefined paneRef gracefully', () => {
    expect(() => {
      throttledRefresh(true, 0, vi.fn(), undefined, 2000)
    }).not.toThrow()
  })
})

describe('createIndexEventHandler', () => {
  it('refreshes the correct pane when a descendant path is updated', () => {
    const leftRefresh = vi.fn()
    const rightRefresh = vi.fn()
    const handler = createIndexEventHandler({
      getLeftPath: () => '/Users/test/left',
      getRightPath: () => '/Users/test/right',
      getPaneRef: (pane) =>
        ({
          refreshIndexSizes: pane === 'left' ? leftRefresh : rightRefresh,
        }) as unknown as FilePaneAPI,
    })

    handler(['/private/Users/test/left/subdir/'])
    // Won't match because /Users/test/left gets resolvePrivateSymlinks applied (no-op since not /tmp/var/etc)
    // and the event path is /private/... which doesn't start with /Users/test/left/
    expect(leftRefresh).not.toHaveBeenCalled()

    // Match: path is a descendant of right pane path
    handler(['/Users/test/right/subdir/'])
    expect(rightRefresh).toHaveBeenCalled()
  })

  it('respects throttle between calls', () => {
    const refresh = vi.fn()
    const handler = createIndexEventHandler({
      getLeftPath: () => '/Users/test',
      getRightPath: () => '/other',
      getPaneRef: () => ({ refreshIndexSizes: refresh }) as unknown as FilePaneAPI,
    })

    handler(['/Users/test/child/'])
    expect(refresh).toHaveBeenCalledTimes(1)

    // Second call within cooldown should be throttled
    handler(['/Users/test/child2/'])
    expect(refresh).toHaveBeenCalledTimes(1)
  })
})
