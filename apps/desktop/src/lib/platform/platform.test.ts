import { describe, expect, it, onTestFinished } from 'vitest'
import {
  applyPlatformAttribute,
  FALLBACK_PLATFORM,
  getPlatform,
  isMacOS,
  PLATFORM_GLOBAL,
  type Platform,
} from './index.js'
import { withPlatform } from './test-platform.js'

const host = globalThis as Record<string, unknown>

describe('getPlatform', () => {
  it('reads the value the backend injected', () => {
    for (const platform of ['macos', 'linux', 'windows'] satisfies Platform[]) {
      host[PLATFORM_GLOBAL] = platform
      expect(getPlatform()).toBe(platform)
    }
    Reflect.deleteProperty(host, PLATFORM_GLOBAL)
  })

  it('falls back to a non-mac platform where nothing was injected', () => {
    expect(Object.hasOwn(host, PLATFORM_GLOBAL)).toBe(false)
    expect(getPlatform()).toBe(FALLBACK_PLATFORM)
    expect(isMacOS()).toBe(false)
  })

  it('ignores a value that names no platform', () => {
    onTestFinished(() => {
      Reflect.deleteProperty(host, PLATFORM_GLOBAL)
    })
    for (const junk of ['darwin', 'MacOS', '', 1, null, {}]) {
      host[PLATFORM_GLOBAL] = junk
      expect(getPlatform()).toBe(FALLBACK_PLATFORM)
    }
  })

  it('never reads the user agent', () => {
    onTestFinished(() => {
      Reflect.deleteProperty(host, PLATFORM_GLOBAL)
    })
    const userAgent = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(navigator) as object, 'userAgent')
    Object.defineProperty(navigator, 'userAgent', {
      value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)',
      configurable: true,
    })
    onTestFinished(() => {
      Reflect.deleteProperty(navigator, 'userAgent')
      if (userAgent) Object.defineProperty(Object.getPrototypeOf(navigator) as object, 'userAgent', userAgent)
    })
    expect(isMacOS()).toBe(false)
    host[PLATFORM_GLOBAL] = 'macos'
    expect(isMacOS()).toBe(true)
  })
})

describe('withPlatform', () => {
  it('switches the platform and undoes it', () => {
    const undo = withPlatform('windows')
    expect(getPlatform()).toBe('windows')
    const undoInner = withPlatform('macos')
    expect(isMacOS()).toBe(true)
    undoInner()
    expect(getPlatform()).toBe('windows')
    undo()
    expect(Object.hasOwn(host, PLATFORM_GLOBAL)).toBe(false)
  })
})

describe('applyPlatformAttribute', () => {
  it('marks the root with the platform, for CSS', () => {
    onTestFinished(withPlatform('macos'))
    const root = document.createElement('html')
    applyPlatformAttribute(root)
    expect(root.getAttribute('data-platform')).toBe('macos')
  })
})
