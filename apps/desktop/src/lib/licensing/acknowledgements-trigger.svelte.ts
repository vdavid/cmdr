/**
 * Open/close state for the Acknowledgements dialog.
 *
 * A module store rather than a `$state` in `+page.svelte` plus a
 * `ctx.dialogs.*` callback: `+page.svelte` is already over its `file-length`
 * allowlist, and this dialog needs no context from it. Same shape as
 * `whats-new-trigger.svelte.ts`.
 */
export const acknowledgementsState = $state({ open: false })

export function openAcknowledgements(): void {
  acknowledgementsState.open = true
}

export function closeAcknowledgements(): void {
  acknowledgementsState.open = false
}
