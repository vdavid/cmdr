import { describe, expect, it } from 'vitest'
import {
  CAPABILITIES_GLOBAL,
  getPlatform,
  NO_CAPABILITIES,
  PLATFORM_GLOBAL,
  platformCapabilities,
  type Capability,
} from '#lib/platform/index.js'
import { installPlatform, visitorCapabilities, visitorPlatform } from './platform.js'

describe('visitorPlatform', () => {
  it.each([
    [
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15',
      'macos',
    ],
    [
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36',
      'windows',
    ],
    ['Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0', 'linux'],
    ['', 'linux'],
  ])('answers %s with %s', (userAgent, platform) => {
    expect(visitorPlatform(userAgent)).toBe(platform)
  })
})

describe('installPlatform', () => {
  it('answers the app for the visitor, read-only as on desktop', () => {
    const frame = { navigator: { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)' } } as unknown as Window
    installPlatform(frame)
    expect((frame as unknown as Record<string, unknown>)[PLATFORM_GLOBAL]).toBe('macos')
    expect(Object.getOwnPropertyDescriptor(frame, PLATFORM_GLOBAL)?.writable).toBe(false)
  })

  it('is what the app reads once installed in its own frame', () => {
    installPlatform(window)
    expect(getPlatform()).toBe(visitorPlatform(navigator.userAgent))
    expect(platformCapabilities()).toEqual(visitorCapabilities(getPlatform()))
  })

  it('hands the capabilities over read-only too', () => {
    const frame = { navigator: { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)' } } as unknown as Window
    installPlatform(frame)
    expect(Object.getOwnPropertyDescriptor(frame, CAPABILITIES_GLOBAL)?.writable).toBe(false)
    expect(Object.isFrozen((frame as unknown as Record<string, unknown>)[CAPABILITIES_GLOBAL])).toBe(true)
  })
})

describe('visitorCapabilities', () => {
  it('shows a Mac visitor the Mac app, overlay title bar and Full Disk Access included', () => {
    const mac = visitorCapabilities('macos')
    for (const capability of Object.keys(NO_CAPABILITIES) as Capability[])
      expect(mac[capability], capability).toBe(true)
  })

  // With `appMenu`, Quit and the Hide trio are the bar's app-menu rows, so no visitor gets a
  // Quit palette row whose IPC has no backend (`command-registry.test.ts` pins that consequence).
  it('shows everyone else no OS-backed surface but the app menu the demo’s macOS bar draws', () => {
    expect(visitorCapabilities('windows')).toEqual({ ...NO_CAPABILITIES, appMenu: true })
    expect(visitorCapabilities('linux')).toEqual({ ...NO_CAPABILITIES, appMenu: true })
  })
})
