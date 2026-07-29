/**
 * "Is this a text field?" — two predicates over one shared element test, so every
 * layer that has to hand an event to the text field instead of the file pane
 * agrees on the answer: the global keydown resolver, the dispatch core's
 * capability guard, the clipboard handlers' input-vs-file branch, the explorer's
 * type-to-jump guard, and the global right-click resolver.
 *
 * Pick by what the event knows:
 *
 * - **Keyboard** → `isTextInputFocused()`: a keypress goes wherever focus is.
 * - **Mouse** → `isTextInputTarget(event.target)`: a right-click can land on a
 *   field that isn't focused yet, so reading focus would miss it.
 */

/**
 * True for an `<input>`, `<textarea>`, `<select>`, or anything editable through
 * `contenteditable` (inherited, so a span inside an editable host counts).
 *
 * Deliberately NOT narrowed to text-typed inputs. A checkbox or radio never
 * receives ⌘C / ⌘V / ⌘← in a way that matters, and `type` is a moving target
 * (`search`, `url`, `email`, …), so an allowlist would silently drop a field.
 */
function isTextEditingElement(element: Element | null): boolean {
  if (
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement ||
    element instanceof HTMLSelectElement
  )
    return true
  return element instanceof HTMLElement && element.isContentEditable
}

/** True when FOCUS sits in a text-editing element. For keyboard events. */
export function isTextInputFocused(): boolean {
  return isTextEditingElement(document.activeElement)
}

/** True when an event's TARGET is a text-editing element. For mouse events. */
export function isTextInputTarget(target: EventTarget | null): boolean {
  return target instanceof Element && isTextEditingElement(target)
}
