/**
 * Key captions for the marketing clips: every key press a take makes, labelled the way
 * KeyCastr shows it (`⌘F`, `↓`, `Enter`, `⇧⌘N`), so the website can overlay what was
 * pressed. A caption lands on the take that is rolling (`captionKey`), and the encoder
 * writes them on the clip's clock (`planKeyCaptions` in `scripts/marketing-clips-encode.ts`).
 *
 * The helpers here caption AND act, so a scene can't press a key the overlay misses:
 * `pressCaptioned` for a key, `runCaptioned` for a command the scene dispatches in place
 * of its shortcut, `typeCaptioned` for typed text (one caption per string).
 */

import type { TauriPage } from '@srsholmes/tauri-playwright'
import { dispatchMenuCommand, pressKey } from './helpers.js'
import { captionKey } from './marketing-clips-take.js'

/** Modifiers in Apple's order, as the menus and KeyCastr print them. */
const MODIFIERS: readonly [string, string][] = [
  ['Control', '⌃'],
  ['Alt', '⌥'],
  ['Shift', '⇧'],
  ['Meta', '⌘'],
]

/** Keys whose label isn't the key itself. Letters print uppercase, everything else as is. */
const KEY_NAMES: Record<string, string> = {
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Escape: 'Esc',
  ' ': 'Space',
  Backspace: '⌫',
  Delete: '⌦',
  PageUp: 'Page Up',
  PageDown: 'Page Down',
}

/**
 * A key combo in `pressKey`'s spelling (`Meta+Shift+n`, `ArrowDown`, `Enter`) as a caption
 * (`⇧⌘N`, `↓`, `Enter`). Modifiers sort into Apple's order whatever order they came in.
 */
export function keyLabel(combo: string): string {
  // A trailing `+` is the plus key itself (`Meta++`).
  const parts = combo.endsWith('++') ? [...combo.slice(0, -2).split('+'), '+'] : combo.split('+')
  const key = parts[parts.length - 1]
  const held = new Set(parts.slice(0, -1))
  const mods = MODIFIERS.filter(([name]) => held.has(name))
    .map(([, glyph]) => glyph)
    .join('')
  const name = KEY_NAMES[key] ?? (key.length === 1 ? key.toUpperCase() : key)
  return `${mods}${name}`
}

/**
 * The shortcut each command a clip dispatches stands in for: the registry's first,
 * default binding (`commands/sources/*.ts`), checked against it by
 * `marketing-clips-keys.test.ts`. A command a scene dispatches only off camera (zoom,
 * deselect) needs no entry.
 */
export const COMMAND_CAPTIONS = {
  'file.copy': 'F5',
  'file.move': 'F6',
  'file.multiRename': '⌃M',
  'search.open': '⌘F',
  'selection.selectAll': '⌘A',
} as const

export type CaptionedCommand = keyof typeof COMMAND_CAPTIONS

/** Presses `combo` (`pressKey`'s spelling) and captions it. */
export async function pressCaptioned(page: TauriPage, combo: string, code?: string): Promise<void> {
  captionKey(keyLabel(combo))
  await pressKey(page, combo, code)
}

/** Dispatches `id` the way its shortcut would, and captions the shortcut. */
export async function runCaptioned(page: TauriPage, id: CaptionedCommand): Promise<void> {
  captionKey(COMMAND_CAPTIONS[id])
  await dispatchMenuCommand(page, id)
}

/**
 * Captions `text` as one label, then types it with `type` (the scene's own typing
 * helper, which paces the characters).
 */
export async function typeCaptioned(text: string, type: (text: string) => Promise<void>): Promise<void> {
  captionKey(text)
  await type(text)
}
