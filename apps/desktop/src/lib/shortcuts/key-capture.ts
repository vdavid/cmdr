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

/** Maps event.code to a display character for physical keys (used when event.key is "Dead") */
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
 */
export function toCanonicalShortcut(shortcut: string): string {
  if (!shortcut) return shortcut
  return mapKeyName(shortcut, displayToCanonicalKeyNames)
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
 * macOS: ⌘⇧P, ⌘Backspace. Windows/Linux: Ctrl+Shift+P.
 */
export function formatKeyCombo(event: KeyboardEvent): string {
  const parts: string[] = []

  if (isMacOS()) {
    if (event.metaKey) parts.push('⌘')
    if (event.ctrlKey) parts.push('⌃')
    if (event.altKey) parts.push('⌥')
    if (event.shiftKey) parts.push('⇧')
  } else {
    if (event.ctrlKey) parts.push('Ctrl')
    if (event.altKey) parts.push('Alt')
    if (event.shiftKey) parts.push('Shift')
    if (event.metaKey) parts.push('Super')
  }

  // Don't include modifier keys themselves as the main key
  if (!isModifierKey(event.key)) {
    const key = normalizeKeyName(event.key, event.code)
    parts.push(key)
  }

  return isMacOS() ? parts.join('') : parts.join('+')
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
