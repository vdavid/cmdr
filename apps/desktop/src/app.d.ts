/// <reference types="unplugin-icons/types/svelte" />

/**
 * Build-time flag injected by Vite's `define` (see `vite.config.js`). TRUE only
 * in the dedicated i18n screenshot-capture build; FALSE (and dead-code-eliminated)
 * in prod and ordinary dev/E2E builds. Gates the capture instrumentation in
 * `src/lib/intl/messages.svelte.ts`.
 */
declare const __CMDR_I18N_CAPTURE__: boolean
