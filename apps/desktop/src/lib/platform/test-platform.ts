/**
 * Test helper: run code as if on another OS. Sets the same global the backend's init script does,
 * so everything that reads the platform follows, key formatting included.
 */
import { PLATFORM_GLOBAL, type Platform } from './index.js'

/**
 * Switches the platform and returns the undo. Called bare, the switch holds until the next one
 * (each test file starts on the fallback). `beforeEach(() => withPlatform('macos'))` covers each
 * test (Vitest runs a returned function as the teardown); `onTestFinished(withPlatform(…))` covers
 * one. For a module that reads the platform at load time, switch before the imports with
 * `vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))`: hoisted code can't import this
 * helper, so it writes the global itself.
 */
export function withPlatform(platform: Platform): () => void {
  const host = globalThis as Record<string, unknown>
  const had = Object.hasOwn(host, PLATFORM_GLOBAL)
  const previous = host[PLATFORM_GLOBAL]
  host[PLATFORM_GLOBAL] = platform
  return () => {
    if (had) host[PLATFORM_GLOBAL] = previous
    else Reflect.deleteProperty(host, PLATFORM_GLOBAL)
  }
}
