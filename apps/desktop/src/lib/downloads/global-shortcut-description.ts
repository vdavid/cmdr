/**
 * Pure builder for the "Reveal latest download" toggle description in
 * `File system watching`. The description references the LIVE global binding
 * so the moment the user rebinds the hotkey in `Keyboard shortcuts`, the
 * toggle's helper text updates to match.
 *
 * The binding arrives in the user-facing macOS-symbol form (`'⌃⌥⌘J'`), which
 * is also what we want to show — no translation needed.
 */
export function globalRevealDescription(binding: string): string {
  if (!binding) {
    return 'Jump to your most recent download from any app.'
  }
  return `Press ${binding} from any app to jump to your most recent download.`
}
