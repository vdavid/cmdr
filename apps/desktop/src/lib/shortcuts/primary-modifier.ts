/**
 * The platform's primary modifier: ⌘ on macOS, Ctrl on Windows and Linux. It's the key that
 * toggles a row into the selection on click, and the one the viewer's copy, select all, and search
 * take.
 *
 * ❌ Not `metaKey || ctrlKey` as "either": Ctrl+click is the context menu on macOS, and Super is
 * the desktop's key on Linux, so each OS has exactly one.
 */
import { isMacOS } from '#lib/platform/index.js'

/** The modifier flags a keyboard or mouse event carries. */
export type ModifierFlags = Pick<KeyboardEvent, 'metaKey' | 'ctrlKey'>

/** True when the event holds the platform's primary modifier (other modifiers aren't checked). */
export function holdsPrimaryModifier(event: ModifierFlags): boolean {
  return isMacOS() ? event.metaKey : event.ctrlKey
}

/**
 * True when the event holds the platform's primary modifier and no other: not Alt, not Shift, and
 * not the other of ⌘ / Ctrl. For an "exactly ⌘Enter / Ctrl+Enter" key that has no registry command.
 */
export function holdsOnlyPrimaryModifier(event: ModifierFlags & Pick<KeyboardEvent, 'altKey' | 'shiftKey'>): boolean {
  if (event.altKey || event.shiftKey) return false
  return isMacOS() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
}
