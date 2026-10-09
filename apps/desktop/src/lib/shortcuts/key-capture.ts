/**
 * Key capture and formatting.
 *
 * ONE canonical vocabulary, one display layer:
 *
 * - **Canonical** (`formatKeyCombo`, `normalizeKeyName`) is what the command
 *   registry declares, what `shortcuts.json` persists, what the dispatch map is
 *   keyed by, what conflict detection compares, and what Rust's
 *   `frontend_shortcut_to_accelerator` parses. Platform-neutral WORD forms:
 *   `Enter`, `Backspace`, `Escape`, `PageUp`.
 * - **Display** (`toDisplayShortcut`) turns that into what a Mac user expects to
 *   read (`↩`, `⌫`, `⎋`). Rendering only, never stored or compared.
 *
 * Keeping the symbols out of the canonical form is load-bearing: a combo spelled
 * `↩` can never be looked up from a keypress, can never clash with `Enter` in
 * conflict detection, and turns into a broken native accelerator.
 */

/** Check if running on macOS */
export function isMacOS(): boolean {
  if (typeof navigator === 'undefined') return false
  // eslint-disable-next-line cmdr/no-error-string-match -- canonical isMacOS() implementation; no platform API available
  return navigator.userAgent.toLowerCase().includes('mac')
}

/** `event.key` → its canonical name. Arrows are symbols on every platform. */
const canonicalKeyNames: Record<string, string> = {
  Backspace: 'Backspace',
  Delete: 'Delete',
  // macOS reports a PC keyboard's Insert key as the old Apple Help key (`key` AND
  // `code` both `Help`, verified on macOS 27 with a Genius PC keyboard, Safari key
  // log, 2026-10-05). Apple stopped shipping Help keys around 2007, and theirs sat
  // where Insert does, so the name is safe to take over.
  Help: 'Insert',
  Insert: 'Insert',
  Enter: 'Enter',
  Return: 'Enter',
  Escape: 'Escape',
  Tab: 'Tab',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  ' ': 'Space',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
  Home: 'Home',
  End: 'End',
}

/** Canonical name → the glyph macOS users read on their keycaps. */
const macDisplayKeyNames: Record<string, string> = {
  Backspace: '⌫',
  Delete: '⌦',
  Enter: '↩',
  Escape: '⎋',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
}

/** Canonical name → the abbreviation Windows and Linux keyboards use. */
const nonMacDisplayKeyNames: Record<string, string> = {
  Escape: 'Esc',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
}

/**
 * Display (or legacy-stored) name → the canonical name. Feeds the load-time
 * healing pass in `shortcuts-store`, so a `shortcuts.json` written before the
 * vocabulary was unified still resolves.
 */
const displayToCanonicalKeyNames: Record<string, string> = {
  '⌫': 'Backspace',
  '⌦': 'Delete',
  '↩': 'Enter',
  '⎋': 'Escape',
  Return: 'Enter',
  Esc: 'Escape',
  PgUp: 'PageUp',
  PgDn: 'PageDown',
}

/**
 * The character each US key types with Shift. Heals a stored Shift-only key
 * position (`⇧8`) to the character it named (`*`) — see `toCanonicalShortcut`.
 */
const usShiftedCharacter: Record<string, string> = {
  '1': '!',
  '2': '@',
  '3': '#',
  '4': '$',
  '5': '%',
  '6': '^',
  '7': '&',
  '8': '*',
  '9': '(',
  '0': ')',
  '-': '_',
  '=': '+',
  '[': '{',
  ']': '}',
  '\\': '|',
  ';': ':',
  "'": '"',
  '`': '~',
  ',': '<',
  '.': '>',
  '/': '?',
}

/**
 * `event.code` → the character that physical key types unmodified. Read twice: by
 * `normalizeKeyName` when macOS reports `Dead`, and by `physicalKeyCombo` when a
 * modifier made the layout type something else entirely.
 */
const codeToKey: Record<string, string> = {
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Backquote: '`',
  Comma: ',',
  Period: '.',
  Slash: '/',
}

/**
 * Normalize an `event.key` to its canonical name (see the module header).
 * Single characters are uppercased, special keys are mapped to word forms.
 */
export function normalizeKeyName(key: string, code?: string): string {
  // On macOS, Option+key often produces "Dead"; fall back to the physical key via event.code
  if (key === 'Dead' && code) {
    const match = /^Key([A-Z])$/.exec(code) ?? /^Digit(\d)$/.exec(code)
    if (match) return match[1]
    if (code in codeToKey) return codeToKey[code]
    return code // last resort: raw code name
  }

  // Single printable characters are uppercased
  if (key.length === 1 && key !== ' ') {
    return key.toUpperCase()
  }

  return canonicalKeyNames[key] ?? key
}

/**
 * Swap the key name at the end of a combo using `map`, leaving the modifier
 * prefix untouched. Matching on the suffix keeps this safe for both the macOS
 * symbol form (`⌘Backspace`) and the `Ctrl+Backspace` form.
 */
function mapKeyName(shortcut: string, map: Record<string, string>): string {
  for (const [from, to] of Object.entries(map)) {
    if (shortcut.endsWith(from)) return shortcut.slice(0, -from.length) + to
  }
  return shortcut
}

/**
 * Render a canonical combo the way this platform's users read it: `⌘Backspace`
 * shows as `⌘⌫` on macOS, `Escape` as `Esc` elsewhere. Display only — never
 * store, compare, or dispatch the result. Idempotent, so wrapping an
 * already-displayed value is harmless.
 */
export function toDisplayShortcut(shortcut: string): string {
  if (!shortcut) return shortcut
  return mapKeyName(shortcut, isMacOS() ? macDisplayKeyNames : nonMacDisplayKeyNames)
}

/**
 * The canonical spelling of a combo that may carry a display or legacy key name
 * (`⌘⌫` → `⌘Backspace`). Used to heal persisted shortcuts on load. Idempotent.
 *
 * Also heals a Shift-only key POSITION (`⇧8`, `Shift+=`), which no keypress
 * produces since typed symbols are named by their character (`formatKeyCombo`).
 * Those combos were spelled by the US keycap, so they heal to the US character.
 */
export function toCanonicalShortcut(shortcut: string): string {
  if (!shortcut) return shortcut
  const shiftOnly = /^(?:⇧|Shift\+)(.)$/u.exec(shortcut)?.[1]
  if (shiftOnly !== undefined && shiftOnly in usShiftedCharacter) return usShiftedCharacter[shiftOnly]
  return mapKeyName(shortcut, displayToCanonicalKeyNames)
}

/**
 * True for a one-character key with no case: a symbol, punctuation, or a digit.
 * Shift is how a layout TYPES such a character, not a modifier on it, which is
 * why `formatKeyCombo` names it by the character alone. Letters (`H` / `⇧H`)
 * and Space (`⇧Space` is Quick Look) keep their Shift.
 */
function isTypedSymbol(key: string): boolean {
  return Array.from(key).length === 1 && key !== ' ' && key.toLowerCase() === key.toUpperCase()
}

/** True when ⌘ / ⌃ (Ctrl / Super off macOS) is held: the combo is a command, never typing. */
function hasCommandModifier(event: KeyboardEvent): boolean {
  return event.metaKey || event.ctrlKey
}

/**
 * Check if a key is a modifier (should not be captured alone).
 */
export function isModifierKey(key: string): boolean {
  return ['Meta', 'Control', 'Alt', 'Shift', 'OS'].includes(key)
}

/**
 * Format a keyboard event into its canonical combo string. This is the single
 * writer of the shortcut vocabulary: dispatch looks up what it returns, and a
 * rebind persists what it returns.
 *
 * Modifiers come out in a fixed order — ⌘⌃⌥⇧ on macOS, Ctrl+Alt+Shift+Super
 * elsewhere — so a registry default written any other way (Apple's ⌥⌘ display
 * order, say) can never match a keypress. `shortcut-vocabulary.test.ts` pins it.
 *
 * A symbol typed with Shift alone is named by the character, without the ⇧
 * (`isTypedSymbol`): US ⇧8, Swedish ⇧', and the numpad all give `*`, and
 * Hungarian ⇧3 gives `+`. That's what makes a `*` binding mean "the `*` key"
 * on every layout. With ⌘ / ⌃ / ⌥ held, Shift stays: those combos are commands,
 * named by key position (`physicalKeyCombo`).
 *
 * macOS: ⌘⇧P, ⌘Backspace. Windows/Linux: Ctrl+Shift+P.
 */
export function formatKeyCombo(event: KeyboardEvent): string {
  const shiftTypedTheKey = event.shiftKey && !hasCommandModifier(event) && !event.altKey && isTypedSymbol(event.key)
  const parts = modifierTokens({ ...eventModifiers(event), shiftKey: event.shiftKey && !shiftTypedTheKey })

  // Don't include modifier keys themselves as the main key
  if (!isModifierKey(event.key)) {
    const key = normalizeKeyName(event.key, event.code)
    parts.push(key)
  }

  return isMacOS() ? parts.join('') : parts.join('+')
}

/** The held modifiers as combo tokens, in the fixed order: ⌘⌃⌥⇧ on macOS, Ctrl+Alt+Shift+Super elsewhere. */
function modifierTokens(modifiers: Pick<KeyboardEvent, 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>): string[] {
  const { metaKey, ctrlKey, altKey, shiftKey } = modifiers
  const ordered: [boolean, string][] = isMacOS()
    ? [
        [metaKey, '⌘'],
        [ctrlKey, '⌃'],
        [altKey, '⌥'],
        [shiftKey, '⇧'],
      ]
    : [
        [ctrlKey, 'Ctrl'],
        [altKey, 'Alt'],
        [shiftKey, 'Shift'],
        [metaKey, 'Super'],
      ]
  return ordered.filter(([held]) => held).map(([, token]) => token)
}

/**
 * The character a physical key types with nothing held, for the codes we can name:
 * the digit row and the punctuation in `codeToKey`. `undefined` for everything else
 * (letters, numpad, F-keys), where `event.key` is already the right identity.
 */
function physicalKeyCharacter(code: string): string | undefined {
  const digit = /^Digit(\d)$/.exec(code)?.[1]
  if (digit !== undefined) return digit
  return code in codeToKey ? codeToKey[code] : undefined
}

/** The subset of a `KeyboardEvent` `formatKeyCombo` reads for its modifier prefix. */
function eventModifiers(event: KeyboardEvent): Pick<KeyboardEvent, 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'> {
  return { metaKey: event.metaKey, ctrlKey: event.ctrlKey, altKey: event.altKey, shiftKey: event.shiftKey }
}

/**
 * The combo this keypress would format as if the layout had typed the key's own
 * character, or `null` when `event.key` already IS that character.
 *
 * Option, and Shift alongside ⌘ / ⌃, change what a key types, and what it types
 * varies by layout: `⌥⇧=` is `±` on US, and `⌘⇧.` reports `>`. So
 * `formatKeyCombo` can never yield those combos from a real keypress, and a
 * default (or a rebind) spelled that way would be dead on the keyboard. Matching
 * the physical key is what makes them bindable at all, and layout-independent.
 *
 * Deliberately narrow: only the digit row and the punctuation `codeToKey` names,
 * and only while a command combo's modifiers retyped the key. Shift ALONE is out:
 * there the typed character is the identity (`formatKeyCombo`), so `⇧8` is `*` and
 * never the key position. Letters count only with ⌘ or ⌃ held as well: ⌘⌥R reports
 * `®`, never typing, so the key position names it, as the `Dead` branch of
 * `normalizeKeyName` does for ⌘⌥E. A bare ⌥ + letter is typing (`å`), so it stays.
 */
export function physicalKeyCombo(event: KeyboardEvent): string | null {
  if (!event.altKey && !(event.shiftKey && hasCommandModifier(event))) return null
  const letter = hasCommandModifier(event) ? /^Key([A-Z])$/.exec(event.code)?.[1].toLowerCase() : undefined
  const physical = letter ?? physicalKeyCharacter(event.code)
  if (physical === undefined || physical === event.key) return null
  return formatKeyCombo({ ...eventModifiers(event), key: physical, code: event.code } as KeyboardEvent)
}

/**
 * The bare character an Option keypress typed, or `null`. On a Mac, Option is
 * the PC's AltGr: plenty of layouts type `*`, `+`, or `-` with it. So a binding
 * on that character works for that user too, the way it does for a Shift-typed
 * one. Only without ⌘ / ⌃ (those make a command, whatever ⌥ typed), and only
 * for symbols: ⌥ + a letter is a real ⌥ combo, never typing a different letter.
 *
 * A FALLBACK, tried after the exact combo and the physical key
 * (`keyComboCandidates`): a user who bound the ⌥ combo itself keeps it.
 */
export function typedCharacterCombo(event: KeyboardEvent): string | null {
  if (!event.altKey || hasCommandModifier(event) || !isTypedSymbol(event.key)) return null
  return normalizeKeyName(event.key, event.code)
}

/**
 * Every combo this keypress may mean, most specific first: the exact combo, then
 * the physical key (`physicalKeyCombo`), then the bare typed character
 * (`typedCharacterCombo`). No repeats. ❗ The ONE answer to "which bindings can
 * this keypress trigger?": the document dispatcher and every local handler
 * (`eventMatchesCommand`) resolve through it, so a binding can't work in one place
 * and be dead in another.
 */
export function keyComboCandidates(event: KeyboardEvent): string[] {
  const candidates = [formatKeyCombo(event), physicalKeyCombo(event), typedCharacterCombo(event)]
  return candidates.filter((combo, i): combo is string => combo !== null && candidates.indexOf(combo) === i)
}

/**
 * The combo the Settings capture field (and its key-filter box) records for a
 * keypress: the physical key where a modifier retyped it (a rebind persists `⌥⇧=`,
 * not the `⌥⇧±` macOS reports), the canonical combo otherwise. Always the first or
 * second of `keyComboCandidates`, so whatever gets recorded also dispatches.
 */
export function capturedKeyCombo(event: KeyboardEvent): string {
  return physicalKeyCombo(event) ?? formatKeyCombo(event)
}

/** Modifier symbols used in macOS shortcut format */
const macModifierToLinux: Record<string, string> = {
  '⌘': 'Ctrl',
  '⌥': 'Alt',
  '⇧': 'Shift',
  '⌃': 'Ctrl',
}

const macModifierSymbols = new Set(Object.keys(macModifierToLinux))

/**
 * Convert a macOS-format shortcut string to the current platform's format.
 * On macOS, returns as-is. On Linux, converts symbols to names with `+` separator.
 * Special case: when both ⌃ and ⌘ are present, one maps to Ctrl and the other to Shift
 * (since both would otherwise become Ctrl).
 */
export function toPlatformShortcut(shortcut: string): string {
  if (isMacOS()) return shortcut

  // Check if the shortcut contains any macOS modifier symbols
  const chars = Array.from(shortcut)
  const hasModifierSymbols = chars.some((ch) => macModifierSymbols.has(ch))
  if (!hasModifierSymbols) return shortcut

  // Parse the macOS symbol string character by character
  const modifiers: string[] = []
  let key = ''
  let hasCmdSymbol = false
  let hasCtrlSymbol = false

  for (const ch of chars) {
    if (macModifierSymbols.has(ch)) {
      if (ch === '⌘') hasCmdSymbol = true
      if (ch === '⌃') hasCtrlSymbol = true
      modifiers.push(ch)
    } else {
      key += ch
    }
  }

  // Build the Linux modifier list, handling the ⌃+⌘ collision
  const linuxModifiers: string[] = []
  const hasCollision = hasCmdSymbol && hasCtrlSymbol

  for (const mod of modifiers) {
    if (hasCollision && mod === '⌃') {
      // When both ⌃ and ⌘ are present, ⌃ maps to Shift instead of Ctrl
      linuxModifiers.push('Shift')
    } else {
      linuxModifiers.push(macModifierToLinux[mod])
    }
  }

  // Deduplicate modifiers while preserving order
  const seen = new Set<string>()
  const uniqueModifiers = linuxModifiers.filter((m) => {
    if (seen.has(m)) return false
    seen.add(m)
    return true
  })

  return [...uniqueModifiers, key].join('+')
}

/**
 * Check if a keyboard event matches a stored shortcut string.
 */
export function matchesShortcut(event: KeyboardEvent, shortcut: string): boolean {
  return formatKeyCombo(event) === shortcut
}

/**
 * Check if a key combo is complete (has a non-modifier key).
 */
export function isCompleteCombo(event: KeyboardEvent): boolean {
  return !isModifierKey(event.key)
}

/** Modifier tokens that signal command intent (Shift alone doesn't — it types capitals and reverse-tabs). */
const commandModifierTokens = ['⌘', '⌃', '⌥', 'Ctrl', 'Alt', 'Super']

/** Both platform spellings of the Shift modifier: `⇧F6` on macOS, `Shift+F6` elsewhere. */
const shiftModifierTokens = ['⇧', 'Shift+']

/**
 * True when the combo carries the Shift modifier, in either platform spelling. A
 * modifier can never be a combo's own key (`formatKeyCombo` drops modifier-only
 * combos), so a plain token test is exact.
 *
 * The F-key bar's Shift row asks this to pick the binding that belongs in the row
 * the user is looking at: `file.rename` carries both `F2` and `⇧F6`, and the Shift
 * row must show the second one.
 */
export function comboHasShift(shortcut: string): boolean {
  return shiftModifierTokens.some((token) => shortcut.includes(token))
}

/**
 * True when the combo is something a user types in a text field rather than a
 * command: no command modifier (⌘/⌃/⌥ or Ctrl/Alt/Super — Shift alone still
 * counts as typing), and not an F-key or Escape (which never produce text).
 * The centralized dispatch uses this to let typing win in focused text inputs:
 * a bare-key Tier 1 binding (Tab → switch pane) must not fire mid-typing.
 */
export function isTypingKeyCombo(shortcut: string): boolean {
  if (commandModifierTokens.some((token) => shortcut.includes(token))) return false
  // Strip the shift prefix (both platform forms) to inspect the base key.
  const base = shortcut.replace(/^⇧/, '').replace(/^Shift\+/, '')
  if (/^F\d+$/.test(base)) return false
  if (base === 'Escape') return false
  return true
}
