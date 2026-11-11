/**
 * The demo's stand-in for the backend's platform init script (`#lib/platform`). A page has no
 * backend to say which OS it runs on, so the demo answers for the visitor's own computer: the
 * shortcuts must match the keyboard under their hands, ⌘ on a Mac and Ctrl elsewhere. The
 * capabilities follow that answer. See `DETAILS.md` § Platform.
 */
import {
  CAPABILITIES_GLOBAL,
  NO_CAPABILITIES,
  PLATFORM_GLOBAL,
  type Platform,
  type PlatformCapabilities,
} from '#lib/platform/index.js'

/** The visitor's OS, from the one signal a page has. Exported for its test. */
export function visitorPlatform(userAgent: string): Platform {
  if (/Macintosh|Mac OS X/.test(userAgent)) return 'macos'
  if (/Windows/.test(userAgent)) return 'windows'
  return 'linux'
}

/**
 * What the demo shows a Mac visitor: the Mac app's surfaces, as the desktop's macOS answer lists
 * them, so the demo looks like the app it advertises. Its fake backend answers the probes behind
 * them (Full Disk Access granted, `answers.ts`) or leaves them unanswered. Written out rather than
 * derived, so a new capability is a type error here until someone decides what the demo shows.
 */
const MAC_VISITOR_CAPABILITIES: Readonly<PlatformCapabilities> = Object.freeze({
  quickLook: true,
  getInfo: true,
  openTerminalHere: true,
  textEditorChoice: true,
  cloudFileActions: true,
  colorTags: true,
  servicesMenu: true,
  fullDiskAccess: true,
  finderExtension: true,
  revealHandler: true,
  dockPin: true,
  cliCommand: true,
  overlayTitleBar: true,
  imageIndexing: true,
})

/** The demo's capabilities for a visitor on `platform`. Exported for its test. */
export function visitorCapabilities(platform: Platform): Readonly<PlatformCapabilities> {
  return platform === 'macos' ? MAC_VISITOR_CAPABILITIES : NO_CAPABILITIES
}

/** Defines the platform and its capabilities in `frame` before any app module reads them, read-only as on desktop. */
export function installPlatform(frame: Window): void {
  const platform = visitorPlatform(frame.navigator.userAgent)
  Object.defineProperty(frame, PLATFORM_GLOBAL, { value: platform })
  Object.defineProperty(frame, CAPABILITIES_GLOBAL, { value: visitorCapabilities(platform) })
}
