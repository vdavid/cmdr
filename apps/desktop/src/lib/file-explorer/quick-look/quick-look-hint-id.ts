/**
 * Toast ID for the Quick Look educational hint. Split into its own file so
 * `quick-look-hint.ts` (which renders the toast) and `QuickLookHintToastContent.svelte`
 * (which dismisses it) can both reference the constant without forming a cycle.
 */
export const QUICK_LOOK_HINT_TOAST_ID = 'quick-look-hint'
