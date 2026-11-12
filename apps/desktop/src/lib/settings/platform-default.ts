/**
 * A setting default that differs per OS, written as all three arms, each in that OS's own form
 * (the keymap's shape: `macos`, `windows`, `linux`, none the leftover). `getDefaultValue` picks
 * the running platform's arm on every call, so a test that switches platforms reads the right
 * one, and nothing reads the platform at module load.
 *
 * ❗ Read a default through `getDefaultValue`, ❌ never `definition.default` directly: a raw read
 * gets this wrapper, not a value.
 */
import type { Platform } from '#lib/platform/index.js'

const PER_PLATFORM = Symbol('perPlatform')

export interface PlatformDefault<T> {
  readonly [PER_PLATFORM]: Readonly<Record<Platform, T>>
}

/** A default with one value per OS. */
export function platformDefault<T>(values: Readonly<Record<Platform, T>>): PlatformDefault<T> {
  return { [PER_PLATFORM]: values }
}

/** `value` itself, or `platform`'s arm when `value` is a `platformDefault`. */
export function resolveDefault(value: unknown, platform: Platform): unknown {
  if (typeof value === 'object' && value !== null && PER_PLATFORM in value) {
    return (value as PlatformDefault<unknown>)[PER_PLATFORM][platform]
  }
  return value
}
