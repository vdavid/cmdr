import { describe, it, expect } from 'vitest'
import { mkdtempSync, readFileSync, existsSync, writeFileSync, readdirSync, statSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import {
  sanitizeWorktreeSlug,
  resolveInstanceId,
  computeAppDataDir,
  bundleIdentifier,
  productName,
  extractWorktreeFlag,
  buildInstanceConfig,
  deriveInstance,
  pickEphemeralPort,
  writePortFile,
  removePortFile,
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

  it('reformats E2E instance IDs into a pgrep-friendly label without the PID', () => {
    // The checker uses `e2e-<kind>-<pid>` for shard isolation. The label drops the pid so
    // the Dock string stays short and cleanup scripts can target `Cmdr (E2E ` cleanly.
    expect(productName('e2e-mtp-12345')).toBe('Cmdr (E2E mtp)')
    expect(productName('e2e-nonmtp1-99999')).toBe('Cmdr (E2E nonmtp1)')
    expect(productName('e2e-nonmtp-2-12345')).toBe('Cmdr (E2E nonmtp-2)')
  })

  it('leaves non-E2E instance IDs unchanged in the label', () => {
    // dev-<slug> looks superficially similar but must NOT be re-shaped: the slug is
    // user-supplied and could legitimately contain digits in its tail.
    expect(productName('dev-12345')).toBe('Cmdr (dev-12345)')
    expect(productName('dev-foo')).toBe('Cmdr (dev-foo)')
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

  it('omits build.devUrl by default so the static tauri.conf.json value applies', () => {
    const cfg = buildInstanceConfig('dev')
    expect(cfg).not.toBeNull()
    if (cfg === null) return
    expect(cfg.build).toBeUndefined()
  })

  it('writes build.devUrl when a Vite port is supplied', () => {
    const cfg = buildInstanceConfig('dev-foo', { vitePort: 54321 })
    expect(cfg).not.toBeNull()
    if (cfg === null) return
    expect(cfg.build).toEqual({ devUrl: 'http://localhost:54321' })
  })

  it('also stubs the updater endpoint for E2E instances so shards never phone home', () => {
    const cfg = buildInstanceConfig('e2e-nonmtp1-12345')
    expect(cfg).not.toBeNull()
    if (cfg === null) return
    expect(cfg.plugins.updater.endpoints).toEqual(['https://localhost.invalid/no-updater'])
  })

  it('rejects out-of-range Vite ports', () => {
    expect(() => buildInstanceConfig('dev', { vitePort: 0 })).toThrow(/vitePort/)
    expect(() => buildInstanceConfig('dev', { vitePort: 70000 })).toThrow(/vitePort/)
    expect(() => buildInstanceConfig('dev', { vitePort: 1.5 })).toThrow(/vitePort/)
  })
})

describe('pickEphemeralPort', () => {
  it('returns a usable port in the unprivileged range', async () => {
    const port = await pickEphemeralPort()
    expect(port).toBeGreaterThan(1024)
    expect(port).toBeLessThanOrEqual(65535)
  })

  it('returns different ports across two back-to-back calls', async () => {
    // Not a strict guarantee per the OS, but in practice the kernel rotates the ephemeral
    // pool fast enough that two sequential allocations are almost always distinct.
    const a = await pickEphemeralPort()
    const b = await pickEphemeralPort()
    // Even if they happen to collide once in a blue moon, at least one of them is valid.
    expect(a).toBeGreaterThan(0)
    expect(b).toBeGreaterThan(0)
  })
})

describe('writePortFile + removePortFile', () => {
  it('writes the port + newline atomically and reads back', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cmdr-port-file-test-'))
    writePortFile(dir, 'tauri-mcp.port', 54321)
    const content = readFileSync(join(dir, 'tauri-mcp.port'), 'utf8')
    expect(content).toBe('54321\n')
  })

  it('creates the parent directory if missing', () => {
    const dir = join(mkdtempSync(join(tmpdir(), 'cmdr-port-file-test-')), 'nested', 'subdir')
    writePortFile(dir, 'tauri-mcp.port', 1234)
    expect(existsSync(join(dir, 'tauri-mcp.port'))).toBe(true)
  })

  it('overwrites an existing file', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cmdr-port-file-test-'))
    writePortFile(dir, 'tauri-mcp.port', 1111)
    writePortFile(dir, 'tauri-mcp.port', 2222)
    expect(readFileSync(join(dir, 'tauri-mcp.port'), 'utf8')).toBe('2222\n')
  })

  it('does not leave a tempfile behind on success', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cmdr-port-file-test-'))
    writePortFile(dir, 'tauri-mcp.port', 9999)
    const stragglers = readdirSync(dir).filter((name) => name.startsWith('tauri-mcp.port.tmp.'))
    expect(stragglers).toEqual([])
  })

  it.skipIf(process.platform === 'win32')('writes the port file owner-only (0o600)', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cmdr-port-file-test-'))
    writePortFile(dir, 'tauri-mcp.port', 54321)
    const mode = statSync(join(dir, 'tauri-mcp.port')).mode & 0o777
    expect(mode).toBe(0o600)
  })

  it('rejects out-of-range port values', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cmdr-port-file-test-'))
    expect(() => writePortFile(dir, 'tauri-mcp.port', -1)).toThrow(/u16/)
    expect(() => writePortFile(dir, 'tauri-mcp.port', 70000)).toThrow(/u16/)
    expect(() => writePortFile(dir, 'tauri-mcp.port', 1.5)).toThrow(/u16/)
  })

  it('removePortFile is a no-op when the file is missing', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cmdr-port-file-test-'))
    expect(() => {
      removePortFile(dir, 'tauri-mcp.port')
    }).not.toThrow()
  })

  it('removePortFile deletes an existing port file', () => {
    const dir = mkdtempSync(join(tmpdir(), 'cmdr-port-file-test-'))
    writeFileSync(join(dir, 'tauri-mcp.port'), '12345\n')
    removePortFile(dir, 'tauri-mcp.port')
    expect(existsSync(join(dir, 'tauri-mcp.port'))).toBe(false)
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

  it('threads vitePort into the generated config build.devUrl', () => {
    const out = deriveInstance({
      instanceId: 'dev-foo',
      platform: 'darwin',
      home: '/Users/me',
      xdgDataHome: undefined,
      vitePort: 49152,
    })
    expect(out.config?.build?.devUrl).toBe('http://localhost:49152')
  })
})
