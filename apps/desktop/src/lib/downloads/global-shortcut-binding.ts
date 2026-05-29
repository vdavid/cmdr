/**
 * Adapter between the user-facing macOS-symbol shortcut form (the storage
 * format and what the rest of Cmdr uses, like `'⌃⌥⌘J'`) and the accelerator
 * string `tauri-plugin-global-shortcut` understands (`'Control+Alt+Super+J'`).
 *
 * We store the symbol form so settings.json reads naturally, the command
 * registry stays consistent, and the warn-toast copy can drop the binding in
 * verbatim. The plugin (Rust + JS halves) only speaks the named-modifier
 * accelerator dialect, so we translate at the IPC boundary.
 *
 * Asymmetric mapping: macOS-symbol → accelerator is lossless (we own the set
 * of allowed combos). The reverse direction is unused; we never decode an
 * accelerator from the plugin.
 */
export const DEFAULT_GLOBAL_REVEAL_BINDING = '⌃⌥⌘J' // ⌃⌥⌘J

const SYMBOL_TO_MODIFIER: ReadonlyArray<readonly [string, string]> = [
  ['⌃', 'Control'], // ⌃
  ['⌥', 'Alt'], // ⌥
  ['⇧', 'Shift'], // ⇧
  ['⌘', 'Super'], // ⌘ — global-hotkey accepts Command/Cmd/Super for the Cmd key, NOT "Meta"
]

/**
 * Translate a macOS-symbol binding like `'⌃⌥⌘J'` into the accelerator format
 * the plugin expects (`'Control+Alt+Super+J'`). Returns `null` for empty or
 * malformed input — callers treat that as "unbound" (don't register).
 *
 * The key (everything after the trailing modifier symbol) is uppercased so
 * `j` and `J` both produce `Super+J`. The plugin's parser is case-insensitive
 * for the key but case-sensitive for the modifier names; we always emit the
 * canonical capitalization.
 */
export function toAccelerator(binding: string): string | null {
  if (!binding) return null

  const modifiers: string[] = []
  let i = 0
  while (i < binding.length) {
    const ch = binding[i]
    const found = SYMBOL_TO_MODIFIER.find(([sym]) => sym === ch)
    if (!found) break
    if (!modifiers.includes(found[1])) modifiers.push(found[1])
    i++
  }

  const key = binding.slice(i).trim()
  if (!key) return null
  if (modifiers.length === 0) return null // global shortcuts always need at least one modifier

  return [...modifiers, key.toUpperCase()].join('+')
}
