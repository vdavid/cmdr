import { describe, it, expect } from 'vitest'
import {
  sanitizeWorktreeSlug,
  resolveInstanceId,
  computeAppDataDir,
  bundleIdentifier,
  productName,
  extractWorktreeFlag,
  buildInstanceConfig,
  deriveInstance,
} from './instance-id.js'

describe('sanitizeWorktreeSlug', () => {
  it('lowercases ASCII as-is', () => {
    expect(sanitizeWorktreeSlug('foo')).toBe('foo')
    expect(sanitizeWorktreeSlug('FOO')).toBe('foo')
  })

  it('replaces slashes and other punctuation with dashes', () => {
    expect(sanitizeWorktreeSlug('Feature/Onboarding-Revamp')).toBe('feature-onboarding-revamp')
    expect(sanitizeWorktreeSlug('foo_bar.baz')).toBe('foo-bar-baz')
  })

  it('collapses runs of dashes and trims edges', () => {
    expect(sanitizeWorktreeSlug('--foo--bar--')).toBe('foo-bar')
    expect(sanitizeWorktreeSlug('a___b')).toBe('a-b')
  })

  it('strips non-ASCII characters', () => {
    expect(sanitizeWorktreeSlug('café')).toBe('caf')
  })

  it('truncates to 32 chars and re-trims trailing dashes', () => {
    const long = 'a'.repeat(40)
    expect(sanitizeWorktreeSlug(long)).toBe('a'.repeat(32))
    // 33 chars where char 33 would be a dash created by truncation
    const trickier = 'a'.repeat(31) + 'b!c'
    // 'aaaa...a' (31 a's) + 'b' + '-' + 'c' = 34 chars; sliced to 32 = 'aaaa...a' + 'b' + '-'
    // The slice ends on '-', which gets re-trimmed.
    const result = sanitizeWorktreeSlug(trickier)
    expect(result).not.toBeNull()
    expect(result?.endsWith('-')).toBe(false)
  })

  it('rejects empty or whitespace-only input by returning null', () => {
    expect(sanitizeWorktreeSlug('')).toBeNull()
    expect(sanitizeWorktreeSlug('   ')).toBeNull()
    expect(sanitizeWorktreeSlug('---')).toBeNull()
    expect(sanitizeWorktreeSlug('!@#$')).toBeNull()
  })

  it('handles non-string input safely', () => {
    expect(sanitizeWorktreeSlug(undefined)).toBeNull()
    expect(sanitizeWorktreeSlug(null)).toBeNull()
    expect(sanitizeWorktreeSlug(42)).toBeNull()
  })
})

describe('resolveInstanceId', () => {
  it('returns null for prod (no env, no flag, not dev)', () => {
    expect(resolveInstanceId({ isDev: false, envInstanceId: undefined, worktreeSlug: null })).toBeNull()
  })

  it('returns "dev" for plain pnpm dev', () => {
    expect(resolveInstanceId({ isDev: true, envInstanceId: undefined, worktreeSlug: null })).toBe('dev')
  })

  it('returns dev-<slug> when --worktree is set in dev mode', () => {
    expect(resolveInstanceId({ isDev: true, envInstanceId: undefined, worktreeSlug: 'foo' })).toBe('dev-foo')
    expect(resolveInstanceId({ isDev: true, envInstanceId: undefined, worktreeSlug: 'Feature/X' })).toBe(
      'dev-feature-x',
    )
  })

  it('honors an externally-set env var (E2E checker path)', () => {
    expect(resolveInstanceId({ isDev: true, envInstanceId: 'e2e-nonmtp1-12345', worktreeSlug: 'foo' })).toBe(
      'e2e-nonmtp1-12345',
    )
    expect(resolveInstanceId({ isDev: false, envInstanceId: 'e2e-mtp-99999', worktreeSlug: null })).toBe(
      'e2e-mtp-99999',
    )
  })

  it('throws on unsanitizable --worktree value', () => {
    expect(() => resolveInstanceId({ isDev: true, envInstanceId: undefined, worktreeSlug: '!!!' })).toThrow(
      /worktree must be/,
    )
  })

  it('ignores --worktree outside dev mode (returns null)', () => {
    expect(resolveInstanceId({ isDev: false, envInstanceId: undefined, worktreeSlug: 'foo' })).toBeNull()
  })
})

describe('extractWorktreeFlag', () => {
  it('pulls --worktree foo before --', () => {
    const { slug, rest } = extractWorktreeFlag(['dev', '--worktree', 'foo'])
    expect(slug).toBe('foo')
    expect(rest).toEqual(['dev'])
  })

  it('pulls --worktree=foo before --', () => {
    const { slug, rest } = extractWorktreeFlag(['dev', '--worktree=foo'])
    expect(slug).toBe('foo')
    expect(rest).toEqual(['dev'])
  })

  it('passes --features bar after the -- separator untouched', () => {
    const { slug, rest } = extractWorktreeFlag(['dev', '--worktree', 'foo', '--', '--features', 'virtual-mtp'])
    expect(slug).toBe('foo')
    expect(rest).toEqual(['dev', '--', '--features', 'virtual-mtp'])
  })

  it('does not consume a --worktree that lives AFTER the -- separator', () => {
    const { slug, rest } = extractWorktreeFlag(['dev', '--', '--worktree', 'foo'])
    expect(slug).toBeNull()
    expect(rest).toEqual(['dev', '--', '--worktree', 'foo'])
  })

  it('returns null when no --worktree is present', () => {
    const { slug, rest } = extractWorktreeFlag(['dev'])
    expect(slug).toBeNull()
    expect(rest).toEqual(['dev'])
  })

  it('handles a trailing --worktree with no value', () => {
    const { slug, rest } = extractWorktreeFlag(['dev', '--worktree'])
    expect(slug).toBeNull()
    expect(rest).toEqual(['dev'])
  })
})

describe('computeAppDataDir', () => {
  it('returns the macOS Application Support path on darwin', () => {
    expect(
      computeAppDataDir({
        identifier: 'com.veszelovszki.cmdr-dev',
        platform: 'darwin',
        home: '/Users/me',
        xdgDataHome: undefined,
      }),
    ).toBe('/Users/me/Library/Application Support/com.veszelovszki.cmdr-dev')
  })

  it('uses XDG_DATA_HOME on Linux when set', () => {
    expect(
      computeAppDataDir({
        identifier: 'com.veszelovszki.cmdr-dev-foo',
        platform: 'linux',
        home: '/home/me',
        xdgDataHome: '/custom/data',
      }),
    ).toBe('/custom/data/com.veszelovszki.cmdr-dev-foo')
  })

  it('falls back to ~/.local/share on Linux when XDG_DATA_HOME is unset', () => {
    expect(
      computeAppDataDir({
        identifier: 'com.veszelovszki.cmdr-dev',
        platform: 'linux',
        home: '/home/me',
        xdgDataHome: undefined,
      }),
    ).toBe('/home/me/.local/share/com.veszelovszki.cmdr-dev')
  })

  it('treats an empty XDG_DATA_HOME as unset', () => {
    expect(
      computeAppDataDir({
        identifier: 'com.veszelovszki.cmdr-dev',
        platform: 'linux',
        home: '/home/me',
        xdgDataHome: '',
      }),
    ).toBe('/home/me/.local/share/com.veszelovszki.cmdr-dev')
  })
})

describe('bundleIdentifier + productName', () => {
  it('returns prod values when instance is null', () => {
    expect(bundleIdentifier(null)).toBe('com.veszelovszki.cmdr')
    expect(productName(null)).toBe('Cmdr')
  })

  it('suffixes both with the instance ID', () => {
    expect(bundleIdentifier('dev')).toBe('com.veszelovszki.cmdr-dev')
    expect(productName('dev')).toBe('Cmdr (dev)')
    expect(bundleIdentifier('dev-foo')).toBe('com.veszelovszki.cmdr-dev-foo')
    expect(productName('dev-foo')).toBe('Cmdr (dev-foo)')
    expect(bundleIdentifier('e2e-mtp-12345')).toBe('com.veszelovszki.cmdr-e2e-mtp-12345')
  })
})

describe('buildInstanceConfig', () => {
  it('returns null for prod', () => {
    expect(buildInstanceConfig(null)).toBeNull()
  })

  it('emits a config with identifier, productName, withGlobalTauri, and a dead updater URL', () => {
    const cfg = buildInstanceConfig('dev-foo')
    expect(cfg).not.toBeNull()
    if (cfg === null) return // narrow for TS
    expect(cfg.identifier).toBe('com.veszelovszki.cmdr-dev-foo')
    expect(cfg.productName).toBe('Cmdr (dev-foo)')
    expect(cfg.app.withGlobalTauri).toBe(true)
    expect(cfg.plugins.updater.endpoints).toEqual(['https://localhost.invalid/no-updater'])
  })
})

describe('deriveInstance', () => {
  it('composes identifier, dataDir, and config in one call (macOS dev)', () => {
    const out = deriveInstance({
      instanceId: 'dev',
      platform: 'darwin',
      home: '/Users/me',
      xdgDataHome: undefined,
    })
    expect(out.identifier).toBe('com.veszelovszki.cmdr-dev')
    expect(out.dataDir).toBe('/Users/me/Library/Application Support/com.veszelovszki.cmdr-dev')
    expect(out.config?.productName).toBe('Cmdr (dev)')
  })

  it('returns prod identifier with no config when instance is null', () => {
    const out = deriveInstance({
      instanceId: null,
      platform: 'darwin',
      home: '/Users/me',
      xdgDataHome: undefined,
    })
    expect(out.identifier).toBe('com.veszelovszki.cmdr')
    expect(out.dataDir).toBe('/Users/me/Library/Application Support/com.veszelovszki.cmdr')
    expect(out.config).toBeNull()
  })
})
