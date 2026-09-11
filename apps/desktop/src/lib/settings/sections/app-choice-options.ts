/**
 * What every "pick an app" row shares, whatever the app is for: the row shape and
 * the "Choose an app…" sentinel. `AppChoiceSelect.svelte` renders the rows; each
 * row's own `*-options.ts` builds them from its backend answer.
 */

/**
 * One dropdown row. Structurally a `SelectItem` (`$lib/ui/Select.svelte`), spelled
 * out here rather than imported: this module is plain TypeScript, and a type
 * reaching out of a `.svelte` module block doesn't resolve for the TS-aware lint
 * pass over `.ts` files.
 */
export interface AppChoiceOption {
  value: string
  label: string
  iconUrl?: string
}

/**
 * The "Choose an app…" row's value. Never stored: the row intercepts it and
 * opens the app picker instead. It's neither a bundle id, nor an absolute path,
 * nor `system`, the shapes Rust's `parse_choice` reads, so it can't be mistaken
 * for a real choice even if it somehow reached the store.
 */
export const CHOOSE_APP_VALUE = '__choose_app__'
