/// <reference types="unplugin-icons/types/svelte" />

declare global {
  /**
   * Build-time flag injected by Vite's `define` (see `vite.config.js`). TRUE in every E2E build
   * (`CMDR_E2E_BUILD=1`: the Playwright lane's binary, which the i18n screenshot run shares, and the
   * Linux Docker build); FALSE (and dead-code-eliminated) in production and `pnpm dev`. Gates the
   * harness-only instruments: the i18n capture sink in `src/lib/intl/messages.svelte.ts` on its own,
   * and together with `import.meta.env.DEV` the dialog gallery and the dev Graphics catalog. It says
   * what a bundle CARRIES, never which run is in flight: that's `getAppMode()` in `#lib/app-mode`.
   */
  const __CMDR_E2E_BUILD__: boolean

  /**
   * Build-time flag injected by Vite's `define` (see `vite.config.js`): TRUE only in the browser demo
   * build (`CMDR_DEMO_BUILD=1`, served at getcmdr.com/demo), where `src/lib/demo/` stands in for the
   * Tauri backend. FALSE and dead-code-eliminated everywhere else.
   */
  const __CMDR_DEMO_BUILD__: boolean

  /**
   * Dev-only label of which working tree this session runs against (worktree slug, "main", or
   * the worktree directory name), injected by Vite's `define` (see `vite.config.js`) from the
   * wrapper-set `CMDR_WORKTREE_LABEL`. Empty string in prod, E2E, and plain `vite dev`. The
   * dev-mode title bar wraps it around the window title (see `src/lib/app-mode.ts`).
   */
  const __CMDR_WORKTREE_LABEL__: string

  namespace App {
    /** What `hooks.client.ts` hands `routes/+error.svelte` about a caught throw. */
    interface Error {
      status: number
      message: string
      /** The throw's `Name: message`, shown on the error page and put into the report note. */
      detail?: string
    }
  }
}

// A module, so the `declare global` block above is an augmentation.
export {}
