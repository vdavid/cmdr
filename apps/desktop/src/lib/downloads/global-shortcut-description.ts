/**
 * Pure builder for the "Go to latest download" toggle description in
 * `Behavior > Notifications`. The description references the LIVE global binding
 * so the moment the user rebinds the hotkey in `Keyboard shortcuts`, the
 * toggle's helper text updates to match.
 *
 * The binding arrives in the user-facing macOS-symbol form (`'⌃⌥⌘J'`), which
 * is also what we want to show — no translation needed.
 */
import { tString } from '$lib/intl/messages.svelte'

export function globalGoToLatestDescription(binding: string): string {
  if (!binding) {
    return tString('downloads.toggleDescription.unbound')
  }
  return tString('downloads.toggleDescription.bound', { binding })
}
