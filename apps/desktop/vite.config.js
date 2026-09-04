import { defineConfig } from 'vite'
import { sveltekit } from '@sveltejs/kit/vite'
import Icons from 'unplugin-icons/vite'
import { stripCatalogMetadata } from './scripts/vite-strip-catalog-metadata.ts'

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

// Every build that carries the dialog gallery: the capture build (which photographs
// gallery states for translators) and the E2E build (whose `dialog-inset.spec.ts`
// measures every dialog through it). Set by `test:e2e:playwright:build` and by the
// Linux Docker build; a production build sets neither, so the harness and every
// dialog it imports still drop out of the shipped bundle.
const e2eBuild = process.env.CMDR_E2E_BUILD === '1'

// Dev-only label of which working tree this session runs against (worktree slug, "main", or
// the worktree directory name), set by the wrapper (scripts/tauri-wrapper.js). The dev-mode
// title bar wraps it around the window title so side-by-side worktree windows are tellable
// apart. Empty for prod builds and plain `vite dev` outside the wrapper. See
// `src/lib/app-mode.ts`.
const worktreeLabel = process.env.CMDR_WORKTREE_LABEL ?? ''

export default defineConfig(async () => ({
  // `stripCatalogMetadata` runs `pre`, so it sees each locale catalog before
  // Vite's JSON transform and hands on messages-only JSON. Every build gets it,
  // dev included, so dev and prod agree on what a catalog contains; only
  // `vitest.config.ts` omits it, which is what keeps the runtime `stripMetadata`
  // under test. See that plugin's module doc for the measured saving.
  plugins: [stripCatalogMetadata(), Icons({ compiler: 'svelte' }), sveltekit()],

  define: {
    __CMDR_I18N_CAPTURE__: JSON.stringify(i18nCaptureBuild),
    __CMDR_DIALOG_GALLERY__: JSON.stringify(i18nCaptureBuild || e2eBuild),
    __CMDR_WORKTREE_LABEL__: JSON.stringify(worktreeLabel),
  },

  build: {
    // The oldest WebKit Cmdr promises to run on, pinned rather than inherited.
    // Vite's default target is `ESBUILD_BASELINE_WIDELY_AVAILABLE_TARGET`, a
    // moving "widely available" baseline (Safari 16.4 as of Vite 8), so leaving
    // it unset lets a routine Vite major bump raise the browser floor above the
    // macOS version `tauri.conf.json`'s `minimumSystemVersion` promises, and the
    // app just white-screens on a syntax it can't parse. Monterey ships Safari
    // 15.0 and a fully patched Catalina tops out at 15.6.1, so 15.0 is the worst
    // case on both. `build.cssTarget` follows `build.target`, so the CSS is
    // downleveled to the same floor. `desktop-vite-build-target` keeps this
    // pinned; see `docs/notes/system-requirements-and-es2025.md`.
    target: 'safari15',
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
    // Forbid the webview from STORING dev-server responses on disk. See
    // `apps/desktop/DETAILS.md` for the measurements and why `no-cache` isn't enough.
    headers: { 'Cache-Control': 'no-store' },
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
