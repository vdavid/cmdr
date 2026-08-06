/// <reference types="unplugin-icons/types/svelte" />

/**
 * Build-time flag injected by Vite's `define` (see `vite.config.js`). TRUE only
 * in the dedicated i18n screenshot-capture build; FALSE (and dead-code-eliminated)
 * in prod and ordinary dev/E2E builds. Gates the capture instrumentation in
 * `src/lib/intl/messages.svelte.ts`.
 */
declare const __CMDR_I18N_CAPTURE__: boolean

/**
 * Build-time flag injected by Vite's `define` (see `vite.config.js`). TRUE in the
 * i18n capture build AND in E2E builds (`CMDR_E2E_BUILD=1`); FALSE (and
 * dead-code-eliminated) in production. Gates the dialog gallery, which both of
 * those builds drive: the capture run photographs its states, `dialog-inset.spec.ts`
 * measures them. ❌ Never gate a gallery site on `__CMDR_I18N_CAPTURE__` alone —
 * that silently switches the E2E lane's dialog coverage off.
 */
declare const __CMDR_DIALOG_GALLERY__: boolean

/**
 * Dev-only label of which working tree this session runs against (worktree slug, "main", or
 * the worktree directory name), injected by Vite's `define` (see `vite.config.js`) from the
 * wrapper-set `CMDR_WORKTREE_LABEL`. Empty string in prod, E2E, and plain `vite dev`. The
 * dev-mode title bar wraps it around the window title (see `src/lib/app-mode.ts`).
 */
declare const __CMDR_WORKTREE_LABEL__: string
