/// <reference types="unplugin-icons/types/svelte" />

/**
 * Build-time flag injected by Vite's `define` (see `vite.config.js`). TRUE only
 * in the dedicated i18n screenshot-capture build; FALSE (and dead-code-eliminated)
 * in prod and ordinary dev/E2E builds. Gates the capture instrumentation in
 * `src/lib/intl/messages.svelte.ts`.
 */
declare const __CMDR_I18N_CAPTURE__: boolean

/**
 * Dev-only label of which working tree this session runs against (worktree slug, "main", or
 * the worktree directory name), injected by Vite's `define` (see `vite.config.js`) from the
 * wrapper-set `CMDR_WORKTREE_LABEL`. Empty string in prod, E2E, and plain `vite dev`. The
 * dev-mode title bar wraps it around the window title (see `src/lib/app-mode.ts`).
 */
declare const __CMDR_WORKTREE_LABEL__: string
