/**
 * "Is the user typing right now?" — one predicate, so every layer that has to
 * hand a key combo to the text field instead of the file pane agrees on the
 * answer: the global keydown resolver, the dispatch core's capability guard, and
 * the clipboard handlers' input-vs-file branch.
 */

/**
 * True when focus sits in a text-editing element: an `<input>`, a `<textarea>`,
 * or anything inside a `contenteditable` host.
 *
 * Deliberately NOT narrowed to text-typed inputs. A checkbox or radio never
 * receives ⌘C / ⌘V / ⌘← in a way that matters, and `type` is a moving target
 * (`search`, `url`, `email`, …), so an allowlist would silently drop a field.
 */
export function isTextInputFocused(): boolean {
  const active = document.activeElement
  if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) return true
  return !!active?.closest('[contenteditable]')
}
