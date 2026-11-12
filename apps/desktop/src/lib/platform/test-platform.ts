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
 * What each OS CAN offer: every field its Rust arm (`src-tauri/src/platform/macos.rs`,
 * `windows.rs`, `linux/mod.rs`) doesn't answer with a literal `false`. A runtime answer
 * (`cfg!(…)`, `x11_session()`) counts as possible, since some user on that OS gets it.
 * `capabilities-possible-on.test.ts` pins these lists to the arms, so an arm edit fails there
 * until the list follows.
 */
export const CAPABILITIES_POSSIBLE_ON: Readonly<Record<Platform, readonly Capability[]>> = {
  macos: [
    'quickLook',
    'getInfo',
    'openTerminalHere',
    'textEditorChoice',
    'cloudFileActions',
    'colorTags',
    'servicesMenu',
    'fullDiskAccess',
    'finderExtension',
    'revealHandler',
    'dockPin',
    'cliCommand',
    'overlayTitleBar',
    'imageIndexing',
    'localNetworkPermission',
    'localAi',
    'safeSaveFiles',
    'appBundles',
    'fullScreen',
    'homebrewCask',
    'globalShortcuts',
  ],
  windows: ['globalShortcuts'],
  // `imageIndexing` follows the `playwright-e2e` feature, `globalShortcuts` an X11 session.
  linux: ['imageIndexing', 'globalShortcuts'],
}

/**
 * Makes this OS offer exactly what `platform` can offer ({@link CAPABILITIES_POSSIBLE_ON}) and
 * returns the undo; the same forms as {@link withCapabilities}. For a test of what a user on that
 * OS gets (the keymap, conflicts, reachability); pair it with `withPlatform(platform)`. ❌ Not
 * {@link withEveryCapability} for a per-OS test: that puts macOS-only commands on Linux.
 */
export function withCapabilitiesPossibleOn(platform: Platform): () => void {
  return withCapabilities(...CAPABILITIES_POSSIBLE_ON[platform])
}

/**
 * Makes this OS offer every capability and returns the undo: the world where every surface
 * renders, for a test that walks all of them (the settings tree, the command registry). Not
 * "macOS": a test about one feature names its capability with {@link withCapabilities}.
 */
export function withEveryCapability(): () => void {
  return withCapabilities(...(Object.keys(NO_CAPABILITIES) as Capability[]))
}
