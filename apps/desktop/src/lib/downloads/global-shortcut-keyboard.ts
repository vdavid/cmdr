/**
 * The keyboard `GlobalShortcutAnimation` draws, per OS, and which of its keys a binding presses.
 *
 * One geometry for every OS (four rows of the left and middle of a keyboard, cropped by the
 * SVG's viewBox), so the animation's timing and framing are shared; only the printed labels and
 * the bottom row's modifier order differ. macOS prints Apple's key caps (lowercase words low on
 * the cap, the modifier glyph above). Windows and Linux print a PC laptop's (title-case words high
 * on the cap; Ctrl, Fn, the Windows key, Alt), with the Windows key named the way Cmdr names it on
 * each OS (`Win`, `Super`; `docs/style-guide.md` § "Platform vocabulary").
 */
import type { Platform } from '#lib/platform/index.js'
import { parseBinding, type AcceleratorModifier } from './global-shortcut-binding'

/** One drawn key: its place in the SVG's user space and what its cap prints. */
export interface DrawnKey {
  readonly x: number
  readonly y: number
  readonly width: number
  /** A character key's character, printed large and centered (`J`, `;`). */
  readonly char?: string
  /** A named key's word (`command`, `Ctrl`). */
  readonly word?: string
  /** Where the word starts, in the cap's own space. */
  readonly wordX?: number
  /** macOS's modifier glyph (`⌘`), printed small above the word. */
  readonly symbol?: string
  readonly symbolX?: number
  /** The raised bar on F and J. */
  readonly homing?: boolean
  /** What holding this key adds to a chord. */
  readonly modifier?: AcceleratorModifier
}

/** Where a word sits on the cap: low on a Mac key, high on a PC one. */
export const WORD_Y: Record<Platform, number> = { macos: 46, windows: 18, linux: 18 }

const KEY = 54
const charKey = (x: number, y: number, char: string, homing = false): DrawnKey => ({
  x,
  y,
  width: KEY,
  char,
  homing,
})
const letterRow = (y: number, firstX: number, chars: string): DrawnKey[] =>
  Array.from(chars, (char, i) => charKey(firstX + 60 * i, y, char, char === 'F' || char === 'J'))

/** The three character rows, the same on every OS's keyboard. */
const CHARACTER_ROWS: readonly DrawnKey[] = [
  ...letterRow(60, 80, 'QWERTYUIO'),
  ...letterRow(120, 100, 'ASDFGHJKL;'),
  ...letterRow(180, 120, 'ZXCVBNM,.'),
]

/** The named keys at each row's left edge, and the bottom row, per keyboard. */
interface NamedKeys {
  tab: string
  capsLock: string
  shift: string
  bottom: readonly DrawnKey[]
}

const SPACE: DrawnKey = { x: 254, y: 240, width: 280 }

const MAC_KEYS: NamedKeys = {
  tab: 'tab',
  capsLock: 'caps lock',
  shift: 'shift',
  bottom: [
    { x: 0, y: 240, width: KEY, word: 'fn' },
    { x: 60, y: 240, width: KEY, word: 'control', symbol: '⌃', symbolX: 42, modifier: 'Control' },
    { x: 120, y: 240, width: KEY, word: 'option', symbol: '⌥', symbolX: 40, modifier: 'Alt' },
    { x: 180, y: 240, width: 68, word: 'command', symbol: '⌘', symbolX: 52, modifier: 'Super' },
    SPACE,
    // The right-hand pair balances the row; a chord lights the left-hand copy.
    { x: 540, y: 240, width: 68, word: 'command', wordX: 22, symbol: '⌘', symbolX: 6, modifier: 'Super' },
    { x: 614, y: 240, width: KEY, word: 'option', wordX: 18, symbol: '⌥', symbolX: 6, modifier: 'Alt' },
  ],
}

/** A PC laptop's keys (Ctrl, Fn, the Windows key, Alt), with that key printed as `superWord`. */
function pcKeys(superWord: string): NamedKeys {
  return {
    tab: 'Tab',
    capsLock: 'Caps Lock',
    shift: 'Shift',
    bottom: [
      { x: 0, y: 240, width: KEY, word: 'Ctrl', modifier: 'Control' },
      { x: 60, y: 240, width: KEY, word: 'Fn' },
      { x: 120, y: 240, width: KEY, word: superWord, modifier: 'Super' },
      { x: 180, y: 240, width: 68, word: 'Alt', modifier: 'Alt' },
      SPACE,
      { x: 540, y: 240, width: 68, word: 'Alt', modifier: 'Alt' },
      { x: 614, y: 240, width: KEY, word: 'Ctrl', modifier: 'Control' },
    ],
  }
}

const NAMED_KEYS: Record<Platform, NamedKeys> = {
  macos: MAC_KEYS,
  windows: pcKeys('Win'),
  linux: pcKeys('Super'),
}

/** The keyboard drawn on `platform`, in drawing order (rows top to bottom, keys left to right). */
export function keyboardFor(platform: Platform): DrawnKey[] {
  const named = NAMED_KEYS[platform]
  return [
    { x: 0, y: 60, width: 74, word: named.tab },
    ...CHARACTER_ROWS.slice(0, 9),
    { x: 0, y: 120, width: 94, word: named.capsLock },
    ...CHARACTER_ROWS.slice(9, 19),
    { x: 0, y: 180, width: 114, word: named.shift, modifier: 'Shift' },
    ...CHARACTER_ROWS.slice(19),
    ...named.bottom,
  ]
}

/**
 * The indices into `keys` a press of `binding` holds down: the first (left-hand) key for each
 * modifier, plus the character key. `null` when the keyboard can't show the chord (a key it
 * doesn't draw, like `P`, `F5`, or `Space`, or a modifier it has no key for), so the toast drops
 * the animation and keeps its text chip.
 */
export function chordOnKeyboard(binding: string, keys: readonly DrawnKey[]): Set<number> | null {
  const chord = parseBinding(binding)
  if (chord === null) return null
  const pressed = new Set<number>()
  for (const modifier of chord.modifiers) {
    const index = keys.findIndex((key) => key.modifier === modifier)
    if (index === -1) return null
    pressed.add(index)
  }
  const character = chord.key.toUpperCase()
  const index = keys.findIndex((key) => key.char === character)
  if (index === -1) return null
  pressed.add(index)
  return pressed
}
