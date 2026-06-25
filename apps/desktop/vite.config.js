import { defineConfig } from 'vite'
import { sveltekit } from '@sveltejs/kit/vite'
import Icons from 'unplugin-icons/vite'

const host = process.env.TAURI_DEV_HOST

// The wrapper (scripts/tauri-wrapper.ts) reserves an ephemeral Vite port per instance and
// passes it via `CMDR_VITE_PORT` so two `pnpm dev` sessions from two worktrees don't
// collide on 1420. Raw `pnpm vite dev` outside the wrapper still gets the legacy 1420 so a
// dev poking around without the wrapper sees the same behavior as before. `strictPort` is
// on for both paths: a collision should be a loud `EADDRINUSE`, not a silent migration to
// a different port that breaks Tauri's `build.devUrl`. See
// docs/specs/instance-isolation-plan.md § P4 for the design.
const envPort = process.env.CMDR_VITE_PORT
const port = envPort ? Number(envPort) : 1420

// Build-time flag baking the i18n screenshot-capture instrumentation into the
// frontend bundle. TRUE only for the dedicated capture build (the i18n-capture
// orchestrator sets `CMDR_I18N_CAPTURE_BUILD=1` for its `tauri build`); FALSE for
// prod AND ordinary dev/E2E builds. Because it's a compile-time constant, esbuild
// dead-code-eliminates the whole capture path (the `window.__cmdrI18nCapture`
// install, the recording hooks, the sink) when it's false: true zero overhead,
// and verifiably absent from prod (grep the bundle for `__cmdrI18nCapture`). See
// `src/lib/intl/messages.svelte.ts` and `docs/specs/i18n-screenshots-plan.md`.
const i18nCaptureBuild = process.env.CMDR_I18N_CAPTURE_BUILD === '1'

// Dev-only label of which working tree this session runs against (worktree slug, "main", or
// the worktree directory name), set by the wrapper (scripts/tauri-wrapper.js). The dev-mode
// title bar wraps it around the window title so side-by-side worktree windows are tellable
// apart. Empty for prod builds and plain `vite dev` outside the wrapper. See
// `src/lib/app-mode.ts`.
const worktreeLabel = process.env.CMDR_WORKTREE_LABEL ?? ''

export default defineConfig(async () => ({
  plugins: [Icons({ compiler: 'svelte' }), sveltekit()],

  define: {
    __CMDR_I18N_CAPTURE__: JSON.stringify(i18nCaptureBuild),
    __CMDR_WORKTREE_LABEL__: JSON.stringify(worktreeLabel),
  },

  build: {
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      // Suppress Rolldown's PLUGIN_TIMINGS warning; sveltekit-guard taking 80%+ of
      // build time is normal and expected for SvelteKit builds, not actionable.
      checks: { pluginTimings: false },
    },
  },

  clearScreen: false,
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
}))
