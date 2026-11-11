import { describe, expect, it, onTestFinished } from 'vitest'
import {
  applyPlatformAttributes,
  CAPABILITIES_GLOBAL,
  FALLBACK_PLATFORM,
  getPlatform,
  hasCapability,
  isMacOS,
  NO_CAPABILITIES,
  PLATFORM_GLOBAL,
  platformCapabilities,
  type Capability,
  type Platform,
} from './index.js'
import { capabilitiesWith, withCapabilities, withPlatform } from './test-platform.js'

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

describe('platformCapabilities', () => {
  const every = Object.keys(NO_CAPABILITIES) as Capability[]

  it('offers nothing where nothing was injected', () => {
    expect(Object.hasOwn(host, CAPABILITIES_GLOBAL)).toBe(false)
    expect(platformCapabilities()).toEqual(NO_CAPABILITIES)
    for (const capability of every) expect(hasCapability(capability), capability).toBe(false)
  })

  it('reads the answers the backend injected', () => {
    onTestFinished(() => {
      Reflect.deleteProperty(host, CAPABILITIES_GLOBAL)
    })
    host[CAPABILITIES_GLOBAL] = Object.freeze({ ...NO_CAPABILITIES, quickLook: true, overlayTitleBar: true })
    expect(hasCapability('quickLook')).toBe(true)
    expect(hasCapability('overlayTitleBar')).toBe(true)
    expect(hasCapability('fullDiskAccess')).toBe(false)
  })

  it('reads a missing or non-boolean answer as not offered', () => {
    onTestFinished(() => {
      Reflect.deleteProperty(host, CAPABILITIES_GLOBAL)
    })
    host[CAPABILITIES_GLOBAL] = { quickLook: 'yes', getInfo: 1, colorTags: true }
    expect(platformCapabilities()).toEqual({ ...NO_CAPABILITIES, colorTags: true })
    for (const junk of [null, 'quickLook', 1, true]) {
      host[CAPABILITIES_GLOBAL] = junk
      expect(platformCapabilities()).toEqual(NO_CAPABILITIES)
    }
  })

  it('never follows the platform', () => {
    onTestFinished(withPlatform('macos'))
    expect(hasCapability('quickLook')).toBe(false)
  })

  it('hands out a copy, so a caller cannot change the answer', () => {
    const mine = platformCapabilities()
    mine.quickLook = true
    expect(hasCapability('quickLook')).toBe(false)
  })
})

describe('withCapabilities', () => {
  it('offers exactly what it names, and undoes it', () => {
    const undo = withCapabilities('fullDiskAccess', 'dockPin')
    expect(platformCapabilities()).toEqual(capabilitiesWith('fullDiskAccess', 'dockPin'))
    expect(hasCapability('quickLook')).toBe(false)
    const undoInner = withCapabilities()
    expect(hasCapability('fullDiskAccess')).toBe(false)
    undoInner()
    expect(hasCapability('fullDiskAccess')).toBe(true)
    undo()
    expect(Object.hasOwn(host, CAPABILITIES_GLOBAL)).toBe(false)
  })
})

describe('applyPlatformAttributes', () => {
  it('marks the root with the platform, for CSS', () => {
    onTestFinished(withPlatform('macos'))
    const root = document.createElement('html')
    applyPlatformAttributes(root)
    expect(root.getAttribute('data-platform')).toBe('macos')
  })

  it('marks an overlay title bar only where the OS offers one', () => {
    const root = document.createElement('html')
    applyPlatformAttributes(root)
    expect(root.hasAttribute('data-overlay-title-bar')).toBe(false)
    onTestFinished(withCapabilities('overlayTitleBar'))
    applyPlatformAttributes(root)
    expect(root.hasAttribute('data-overlay-title-bar')).toBe(true)
  })
})
