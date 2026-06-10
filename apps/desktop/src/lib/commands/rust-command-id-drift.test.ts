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
import { menuCommands } from '$lib/shortcuts/shortcuts-store'

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

/** Command ids in `command_id_to_menu_id` (the accelerator-sync reverse map). */
function menuAcceleratorCommandIds(): string[] {
  const source = readFileSync(path.join(desktopRoot, 'src-tauri/src/menu/mod.rs'), 'utf8')
  const fnStart = source.indexOf('pub fn command_id_to_menu_id(')
  expect(fnStart, 'command_id_to_menu_id not found in menu/mod.rs').toBeGreaterThan(-1)
  // The function ends at the first closing brace at column 0 after the match arms.
  const fnEnd = source.indexOf('\n}', fnStart)
  const body = source.slice(fnStart, fnEnd)

  const ids = new Set<string>()
  const re = /"([^"]+)" => Some\(/g
  let match: RegExpExecArray | null
  while ((match = re.exec(body)) !== null) {
    ids.add(match[1])
  }
  return [...ids]
}

describe('menuCommands ↔ command_id_to_menu_id drift', () => {
  // `update_menu_accelerator` special-cases the two view-mode CheckMenuItems by
  // command id (they rebuild via the cached-accel path, not the items HashMap),
  // so they belong in `menuCommands` without a reverse-map entry.
  const VIEW_MODE_SPECIALS = ['view.fullMode', 'view.briefMode']

  // Menu items that exist in the reverse map but are NOT registered in
  // `MenuState.items` (no `register_item` call), so an accelerator update would
  // error on the Rust side (`update_menu_item_accelerator` can't find the entry).
  // They must stay OUT of `menuCommands` until registered. Each entry needs a
  // reason; remove it here the day the item is registered.
  const UNREGISTERED_MENU_ITEMS: Record<string, string> = {
    'app.about': 'cmdr app-menu item, never registered in MenuState.items; no default shortcut to sync',
    'app.licenseKey': 'cmdr app-menu item, never registered in MenuState.items; no default shortcut to sync',
    'help.sendErrorReport': 'Help-menu item, never registered in MenuState.items; no default shortcut to sync',
    'tab.togglePin':
      'MenuState.pin_tab holds a dedicated item reference for live label swaps (Pin/Unpin); the remove-recreate accelerator update would orphan it. Register + rework the label path before syncing.',
    'selection.toggle':
      'Select-menu item, never registered in MenuState.items; its key (Space) is not a representable macOS accelerator anyway',
  }

  it('menuCommands covers exactly the syncable menu items (Rust map − unregistered + view modes)', () => {
    const rustIds = menuAcceleratorCommandIds()
    expect(rustIds.length).toBeGreaterThan(40) // parser sanity: the map really was found

    const expected = new Set([...rustIds.filter((id) => !(id in UNREGISTERED_MENU_ITEMS)), ...VIEW_MODE_SPECIALS])
    expect(new Set(menuCommands)).toEqual(expected)
  })

  it('every unregistered exception still exists in the Rust map (no stale excuses)', () => {
    const rustIds = new Set(menuAcceleratorCommandIds())
    for (const id of Object.keys(UNREGISTERED_MENU_ITEMS)) {
      expect(rustIds.has(id), `${id} is excused but no longer in command_id_to_menu_id — drop the exception`).toBe(true)
    }
  })
})
