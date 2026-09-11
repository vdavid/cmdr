/// <reference types="unplugin-icons/types/svelte" />

/**
 * Build-time flag injected by Vite's `define` (see `vite.config.js`). TRUE in every E2E build
 * (`CMDR_E2E_BUILD=1`: the Playwright lane's binary, which the i18n screenshot run shares, and the
 * Linux Docker build); FALSE (and dead-code-eliminated) in production and `pnpm dev`. Gates the
 * harness-only instruments: the i18n capture sink in `src/lib/intl/messages.svelte.ts` on its own,
 * and together with `import.meta.env.DEV` the dialog gallery and the dev Graphics catalog. It says
 * what a bundle CARRIES, never which run is in flight: that's `getAppMode()` in `$lib/app-mode`.
 */
declare const __CMDR_E2E_BUILD__: boolean

/**
 * Dev-only label of which working tree this session runs against (worktree slug, "main", or
 * the worktree directory name), injected by Vite's `define` (see `vite.config.js`) from the
 * wrapper-set `CMDR_WORKTREE_LABEL`. Empty string in prod, E2E, and plain `vite dev`. The
 * dev-mode title bar wraps it around the window title (see `src/lib/app-mode.ts`).
 */
declare const __CMDR_WORKTREE_LABEL__: string
