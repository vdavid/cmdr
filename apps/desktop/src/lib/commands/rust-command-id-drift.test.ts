/**
 * Rust↔frontend command-id drift guard.
 *
 * The `CommandId` union can't reach across the Tauri IPC boundary: Rust emits a
 * typed `ExecuteCommand { command_id }` whose `commandId` is a bare string, and
 * `LicenseSection.svelte` cross-window emits the same via `emitExecuteCommand`.
 * A stale id silently hits the dispatcher's switch `default` and no-ops —
 * TypeScript can't catch it. This test pins every emitted command id to
 * `COMMAND_IDS`, so renaming a registry id without updating Rust (or vice versa)
 * fails here.
 *
 * Mechanism: parse the two Rust/Svelte source files for their command-id string
 * literals rather than maintaining a hand-copied list (which would itself drift).
 * `menu/mod.rs`'s `menu_id_to_command` is the source of truth for menu-emitted
 * ids; `LicenseSection.svelte` is the only cross-window `execute-command` emit.
 *
 * Cross-pointers: `src-tauri/src/menu/mod.rs` § `menu_id_to_command` and
 * `LicenseSection.svelte` § the `emitTo('main', 'execute-command', …)` call both
 * carry a comment pointing back here.
 */
import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { isCommandId } from './command-ids'

const here = path.dirname(fileURLToPath(import.meta.url))
// here = apps/desktop/src/lib/commands → repo desktop root is three up.
const desktopRoot = path.resolve(here, '../../..')

/** Command ids `menu_id_to_command` maps menu items to (the `Some(("id", …))` literals). */
function menuEmittedCommandIds(): string[] {
  const source = readFileSync(path.join(desktopRoot, 'src-tauri/src/menu/mod.rs'), 'utf8')
  // Isolate the `menu_id_to_command` body so we don't also scan the reverse map
  // or the unit tests (which list the same ids and would mask a drift).
  const fnStart = source.indexOf('pub fn menu_id_to_command(')
  expect(fnStart, 'menu_id_to_command not found in menu/mod.rs').toBeGreaterThan(-1)
  const fnEnd = source.indexOf('pub fn command_id_to_menu_id(', fnStart)
  expect(fnEnd, 'command_id_to_menu_id not found after menu_id_to_command').toBeGreaterThan(fnStart)
  const body = source.slice(fnStart, fnEnd)

  // Match `Some(("app.about", CommandScope::App))` → capture `app.about`.
  const ids = new Set<string>()
  const re = /Some\(\("([^"]+)"\s*,\s*CommandScope::/g
  let match: RegExpExecArray | null
  while ((match = re.exec(body)) !== null) {
    ids.add(match[1])
  }
  return [...ids]
}

/** Command ids cross-window-emitted from settings windows via `execute-command`. */
function crossWindowEmittedCommandIds(): string[] {
  const source = readFileSync(path.join(desktopRoot, 'src/lib/settings/sections/LicenseSection.svelte'), 'utf8')
  const ids = new Set<string>()
  // Match `emitExecuteCommand('app.licenseKey')` (the typed cross-window relay
  // wrapper over `events.executeCommand.emit`).
  const re = /emitExecuteCommand\('([^']+)'\)/g
  let match: RegExpExecArray | null
  while ((match = re.exec(source)) !== null) {
    ids.add(match[1])
  }
  return [...ids]
}

describe('Rust↔FE command-id drift', () => {
  it('every menu-emitted command id is a registry CommandId', () => {
    const menuIds = menuEmittedCommandIds()
    // Guard against the regex silently matching nothing (which would make the
    // test pass vacuously if the file shape changed).
    expect(menuIds.length).toBeGreaterThan(20)

    const unknown = menuIds.filter((id) => !isCommandId(id))
    expect(unknown, 'menu-emitted ids not present in COMMAND_IDS').toEqual([])
  })

  it('every cross-window-emitted command id is a registry CommandId', () => {
    const emittedIds = crossWindowEmittedCommandIds()
    expect(emittedIds).toContain('app.licenseKey')

    const unknown = emittedIds.filter((id) => !isCommandId(id))
    expect(unknown, 'cross-window-emitted ids not present in COMMAND_IDS').toEqual([])
  })
})
