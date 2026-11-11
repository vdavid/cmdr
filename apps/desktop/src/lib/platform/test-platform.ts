/**
 * Test helpers: run code as if on another OS, or on one that offers other capabilities. Each sets
 * the same global the backend's init script does, so everything that reads it follows, key
 * formatting included.
 */
import {
  CAPABILITIES_GLOBAL,
  NO_CAPABILITIES,
  PLATFORM_GLOBAL,
  type Capability,
  type Platform,
  type PlatformCapabilities,
} from './index.js'

/** Sets one global and returns the undo, which restores what was there (or its absence). */
function setGlobal(name: string, value: unknown): () => void {
  const host = globalThis as Record<string, unknown>
  const had = Object.hasOwn(host, name)
  const previous = host[name]
  host[name] = value
  return () => {
    if (had) host[name] = previous
    else Reflect.deleteProperty(host, name)
  }
}

/**
 * Switches the platform and returns the undo. Called bare, the switch holds until the next one
 * (each test file starts on the fallback). `beforeEach(() => withPlatform('macos'))` covers each
 * test (Vitest runs a returned function as the teardown); `onTestFinished(withPlatform(…))` covers
 * one. For a module that reads the platform at load time, switch before the imports with
 * `vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))`: hoisted code can't import this
 * helper, so it writes the global itself.
 *
 * It moves the platform only, never the capabilities: a test of a feature says which capability
 * it needs with {@link withCapabilities}.
 */
export function withPlatform(platform: Platform): () => void {
  return setGlobal(PLATFORM_GLOBAL, platform)
}

/** A capability set offering exactly `granted`, for code that takes one as data. */
export function capabilitiesWith(...granted: Capability[]): PlatformCapabilities {
  const capabilities = { ...NO_CAPABILITIES }
  for (const capability of granted) capabilities[capability] = true
  return capabilities
}

/**
 * Makes this OS offer exactly `granted` (nothing else) and returns the undo; the same forms as
 * {@link withPlatform}. `withCapabilities()` with no arguments offers nothing, as the fallback does.
 * Before the imports: `vi.hoisted(() => vi.stubGlobal('__CMDR_CAPABILITIES__', { quickLook: true }))`
 * (an absent field reads `false`).
 */
export function withCapabilities(...granted: Capability[]): () => void {
  return setGlobal(CAPABILITIES_GLOBAL, capabilitiesWith(...granted))
}

/**
 * Makes this OS offer every capability and returns the undo: the world where every surface
 * renders, for a test that walks all of them (the settings tree, the command registry). Not
 * "macOS": a test about one feature names its capability with {@link withCapabilities}.
 */
export function withEveryCapability(): () => void {
  return withCapabilities(...(Object.keys(NO_CAPABILITIES) as Capability[]))
}
