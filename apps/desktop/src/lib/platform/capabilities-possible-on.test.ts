/**
 * Pins `CAPABILITIES_POSSIBLE_ON` to the backend's three OS arms, read as text (Vitest can't run
 * Rust). Each arm answers every `PlatformCapabilities` field as a struct literal; a field counts as
 * possible on that OS unless its value is the literal `false`. So flipping an arm, adding a field,
 * or renaming one fails here until the fixture and `NO_CAPABILITIES` follow.
 */
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { NO_CAPABILITIES, type Capability, type Platform } from './index.js'
import { CAPABILITIES_POSSIBLE_ON } from './test-platform.js'

const here = path.dirname(fileURLToPath(import.meta.url))
// here = apps/desktop/src/lib/platform → the desktop root is three up.
const platformDir = path.resolve(here, '../../../src-tauri/src/platform')

const ARM_FILES: Record<Platform, string> = {
  macos: 'macos.rs',
  windows: 'windows.rs',
  linux: 'linux/mod.rs',
}

/** `quick_look` → `quickLook`, the way the bindings name the field. */
function camelCase(snake: string): string {
  return snake.replace(/_([a-z0-9])/g, (_, letter: string) => letter.toUpperCase())
}

/** Each field of the arm's `PlatformCapabilities { … }` literal with its value's source text. */
function armFields(platform: Platform): Map<string, string> {
  const source = readFileSync(path.join(platformDir, ARM_FILES[platform]), 'utf8')
  // The body of `fn capabilities()`'s struct literal, which opens on its own line (the
  // signature's `-> PlatformCapabilities {` doesn't) and closes at the same indent.
  const body = /fn capabilities\(\)[^\n]*\n {4}PlatformCapabilities \{\n([\s\S]*?)\n {4}\}/.exec(source)?.[1]
  expect(body, `no \`fn capabilities()\` struct literal in ${ARM_FILES[platform]}`).toBeDefined()
  const fields = new Map<string, string>()
  for (const line of (body ?? '').split('\n')) {
    const trimmed = line.trim()
    if (trimmed === '' || trimmed.startsWith('//')) continue
    const match = /^([a-z][a-z0-9_]*):\s*(.+?),$/.exec(trimmed)
    expect(match, `unparsed line in ${ARM_FILES[platform]}: ${trimmed}`).not.toBeNull()
    if (match) fields.set(match[1], match[2])
  }
  return fields
}

const capabilityNames = Object.keys(NO_CAPABILITIES).sort()

describe.each(['macos', 'windows', 'linux'] as const)('CAPABILITIES_POSSIBLE_ON.%s', (platform) => {
  it('names exactly the fields the Rust arm answers', () => {
    const rustNames = [...armFields(platform).keys()].map(camelCase).sort()
    expect(rustNames).toEqual(capabilityNames)
  })

  it('grants exactly the fields the Rust arm does not answer with a literal false', () => {
    const possible = [...armFields(platform)]
      .filter(([, value]) => value !== 'false')
      .map(([field]) => camelCase(field) as Capability)
      .sort()
    expect([...CAPABILITIES_POSSIBLE_ON[platform]].sort()).toEqual(possible)
  })
})
