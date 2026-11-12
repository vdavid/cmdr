/**
 * A command's default key on the OS the suite runs on, read from the registry's reviewed keymap
 * snapshot (`src/lib/commands/__snapshots__/default-keymap.<os>.txt`, which `default-keymap.test.ts`
 * keeps equal to the registry). A spec that presses a command's key asks here, so it follows the
 * keymap on every OS and never guesses the modifier with `isMac ? meta : ctrl`.
 *
 * The answer is the DEFAULT key: the suite runs on a fresh data dir with no rebinds.
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const SNAPSHOTS = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../src/lib/commands/__snapshots__')

type SuiteOs = 'macos' | 'windows' | 'linux'

function suiteOs(): SuiteOs {
  if (process.platform === 'darwin') return 'macos'
  return process.platform === 'win32' ? 'windows' : 'linux'
}

/** Each OS's modifier tokens, as the snapshot spells them, with Playwright's name for each. */
const MODIFIER_TOKENS: Record<SuiteOs, readonly [token: string, playwright: string][]> = {
  macos: [
    ['⌘', 'Meta'],
    ['⌃', 'Control'],
    ['⌥', 'Alt'],
    ['⇧', 'Shift'],
  ],
  windows: [
    ['Ctrl+', 'Control'],
    ['Alt+', 'Alt'],
    ['Shift+', 'Shift'],
    ['Super+', 'Meta'],
  ],
  linux: [
    ['Ctrl+', 'Control'],
    ['Alt+', 'Alt'],
    ['Shift+', 'Shift'],
    ['Super+', 'Meta'],
  ],
}

/** The registry's key names that Playwright names differently. */
const KEY_NAMES: Record<string, string> = { '↑': 'ArrowUp', '↓': 'ArrowDown', '←': 'ArrowLeft', '→': 'ArrowRight' }

/** A canonical combo (`Ctrl+Shift+Q`, `⌘⌥L`, `Shift+F5`) in Playwright's spelling (`Control+Shift+q`). */
export function toPlaywrightCombo(combo: string, os: SuiteOs): string {
  const modifiers: string[] = []
  let rest = combo
  for (;;) {
    // The key itself may look like a token's tail (`Ctrl++`), so a token never eats the last character.
    const match = MODIFIER_TOKENS[os].find(([token]) => rest.startsWith(token) && rest.length > token.length)
    if (match === undefined) break
    modifiers.push(match[1])
    rest = rest.slice(match[0].length)
  }
  const key = KEY_NAMES[rest] ?? (rest.length === 1 ? rest.toLowerCase() : rest)
  return [...modifiers, key].join('+')
}

/**
 * The same key as a `KeyboardEventInit` (`{ key: 'q', ctrlKey: true, shiftKey: true, … }`), for a
 * spec that dispatches its own keydown inside the webview.
 */
export function defaultKeyInit(commandId: string, index = 0): KeyboardEventInit {
  const parts = defaultKey(commandId, index).split('+')
  // `Control++`: the split leaves two empty parts, and the key was a `+`.
  const key = parts.at(-1) === '' ? '+' : (parts.at(-1) ?? '')
  const held = new Set(parts.slice(0, -1))
  return {
    key,
    ctrlKey: held.has('Control'),
    altKey: held.has('Alt'),
    shiftKey: held.has('Shift'),
    metaKey: held.has('Meta'),
  }
}

/** The `index`th default key of `commandId` on this OS, in Playwright's spelling. Throws when it has none. */
export function defaultKey(commandId: string, index = 0): string {
  const os = suiteOs()
  const snapshot = fs.readFileSync(path.join(SNAPSHOTS, `default-keymap.${os}.txt`), 'utf8')
  const line = snapshot.split('\n').find((l) => l.startsWith(`${commandId}\t`))
  if (line === undefined) throw new Error(`${commandId} isn't in the ${os} keymap`)
  const combos = line
    .slice(commandId.length + 1)
    .split(', ')
    .filter((c) => c !== '')
  const combo = combos.at(index)
  if (combo === undefined) throw new Error(`${commandId} has no default key #${String(index)} on ${os}`)
  return toPlaywrightCombo(combo, os)
}
