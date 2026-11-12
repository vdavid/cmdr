/**
 * Adapter between the stored binding (the canonical combo `formatKeyCombo` writes on this OS:
 * `'⌃⌥⌘J'` on macOS, `'Ctrl+Alt+Super+J'` on Windows and Linux) and the accelerator string
 * `tauri-plugin-global-shortcut` understands (`'Control+Alt+Super+J'`).
 *
 * We store the canonical form so settings.json reads naturally, the recorder's output goes in
 * as it is, and the copy shows the binding through `toDisplayShortcut` like every other key. The
 * plugin (Rust + JS halves) only speaks the named-modifier accelerator dialect, so we translate
 * at the IPC boundary. The Rust twin is `downloads::runtime::binding_to_accelerator`.
 *
 * Asymmetric mapping: combo → accelerator is lossless (we own the set of allowed combos). The
 * reverse direction is unused; we never decode an accelerator from the plugin.
 */

/**
 * The one chord `GlobalShortcutAnimation` draws, with the Mac's own key caps. The toast plays it
 * only when the binding is exactly this, so a remapped combo, or another OS's spelling of it,
 * never animates keys the user doesn't press.
 */
export const ANIMATED_GLOBAL_GO_TO_LATEST_BINDING = '⌃⌥⌘J'

/** Each spelling's modifier token → its accelerator name. */
const SYMBOL_TO_MODIFIER: ReadonlyArray<readonly [string, string]> = [
  ['⌃', 'Control'],
  ['⌥', 'Alt'],
  ['⇧', 'Shift'],
  ['⌘', 'Super'], // global-hotkey accepts Command/Cmd/Super for the Cmd key, NOT "Meta"
]
const WORD_TO_MODIFIER: ReadonlyArray<readonly [string, string]> = [
  ['Ctrl+', 'Control'],
  ['Alt+', 'Alt'],
  ['Shift+', 'Shift'],
  ['Super+', 'Super'],
]
/** A modifier left where the key belongs (`'⌘⇧'`, `'Ctrl+Alt'`): no key, so no binding. */
const MODIFIER_ONLY_KEYS = new Set([
  ...SYMBOL_TO_MODIFIER.map(([symbol]) => symbol),
  ...WORD_TO_MODIFIER.map(([word]) => word.slice(0, -1)),
])

/**
 * Translate a stored binding (`'⌃⌥⌘J'` or `'Ctrl+Alt+Super+J'`) into the accelerator format the
 * plugin expects (`'Control+Alt+Super+J'`). Returns `null` for empty or malformed input; callers
 * treat that as "unbound" (don't register).
 *
 * The key (everything after the last modifier) is uppercased so `j` and `J` both produce
 * `Super+J`. The plugin's parser is case-insensitive for the key but case-sensitive for the
 * modifier names; we always emit the canonical capitalization.
 */
export function toAccelerator(binding: string): string | null {
  if (!binding) return null

  const modifiers: string[] = []
  let rest = binding
  for (;;) {
    // A token never eats the last character, so a key that looks like a token's tail stays the key.
    const found = [...SYMBOL_TO_MODIFIER, ...WORD_TO_MODIFIER].find(
      ([token]) => rest.startsWith(token) && rest.length > token.length,
    )
    if (!found) break
    if (!modifiers.includes(found[1])) modifiers.push(found[1])
    rest = rest.slice(found[0].length)
  }

  const key = rest.trim()
  if (!key || MODIFIER_ONLY_KEYS.has(key)) return null
  if (modifiers.length === 0) return null // global shortcuts always need at least one modifier

  return [...modifiers, key.toUpperCase()].join('+')
}
