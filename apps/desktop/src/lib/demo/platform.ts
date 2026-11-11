/**
 * The demo's stand-in for the backend's platform init script (`#lib/platform`). A page has no
 * backend to say which OS it runs on, so the demo answers for the visitor's own computer: the
 * shortcuts must match the keyboard under their hands, ⌘ on a Mac and Ctrl elsewhere. See
 * `DETAILS.md` § Platform.
 */
import { PLATFORM_GLOBAL, type Platform } from '#lib/platform/index.js'

/** The visitor's OS, from the one signal a page has. Exported for its test. */
export function visitorPlatform(userAgent: string): Platform {
  if (/Macintosh|Mac OS X/.test(userAgent)) return 'macos'
  if (/Windows/.test(userAgent)) return 'windows'
  return 'linux'
}

/** Defines the platform in `frame` before any app module reads it, read-only as on desktop. */
export function installPlatform(frame: Window): void {
  Object.defineProperty(frame, PLATFORM_GLOBAL, { value: visitorPlatform(frame.navigator.userAgent) })
}
