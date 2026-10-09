/**
 * Test helper: a button's label as a person reads it, without the letter-key
 * chip a decision prompt puts inside it (`DecisionKeyHint.svelte`, which is
 * `aria-hidden`). `textContent` alone would read "SkipS".
 */
export function buttonLabel(button: Element): string {
  const copy = button.cloneNode(true) as Element
  for (const hidden of copy.querySelectorAll('[aria-hidden="true"]')) hidden.remove()
  return copy.textContent.trim()
}
