// Tauri doesn't have a Node.js server to do proper SSR, so we use adapter-static with a
// fallback to index.html to put the site in SPA mode.
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import adapter from '@sveltejs/adapter-static'
import { sveltekit } from '@sveltejs/kit/vite'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'
import Icons from 'unplugin-icons/vite'
import { buildBootGuardData, injectBootGuardData } from './scripts/gen-boot-guard-lib.ts'
import { listLocales } from './scripts/i18n-catalog-lib.ts'
import { frontendSbom } from './scripts/vite-frontend-sbom.ts'
import { localeCatalogs } from './scripts/vite-locale-catalogs.ts'
import { demoVoidCommands } from './scripts/vite-demo-void-commands.ts'
import { webkitFloor } from './scripts/vite-webkit-floor.ts'

// Compiler warnings suppressed app-wide. The ONE list: `svelte-check` (the `svelte-check`
// lane, which fails on any warning) and the Vite build both read this filter.
//
// The keyboard-reachability warnings (`a11y_click_events_have_key_events`,
// `a11y_no_noninteractive_element_interactions`, `a11y_interactive_supports_focus`) stay ON:
// they catch a click handler with no keyboard path. A site whose keys live elsewhere (a
// listbox input, a grid's page-level keydown, a link delegate) opts out with a
// `<!-- svelte-ignore <code> -->` plus the reason, next to the code (#383).
const suppressedWarnings = [
  // Flags the opposite of a gap: a focusable scroll region or `role="document"` view, which
  // axe's `scrollable-region-focusable` wants focusable so arrow keys can scroll it.
  'a11y_no_noninteractive_tabindex',
  'state_referenced_locally',
  'non_reactive_update',
]

/**
 * The Svelte compiler setup every build of our components shares: the app build here, and
 * `vitest.config.ts`'s bare `svelte()` plugin, which no longer finds a `svelte.config.js` to
 * load it from.
 */
export const svelteOptions = {
  preprocess: vitePreprocess(),
  compilerOptions: {
    warningFilter: (/** @type {{ code: string }} */ warning) => !suppressedWarnings.includes(warning.code),
  },
}

// Where adapter-static writes the site. Default `build` matches tauri.conf.json's
// `frontendDist: "../build"`. The Linux-E2E Docker build overrides this (paired with a
// tauri `--config` frontendDist override in `scripts/e2e-linux.sh`): the container
// builds from the SAME bind-mounted tree the host may be building in (`pnpm check
// --include-slow` runs the host Playwright build and the container build
// concurrently), so it redirects its output into its own Docker-volume-backed dir.
// The adapter rimrafs this dir on every build, so it must never BE a mount point
// (rmdir on a mount point is EBUSY) — pointing it INSIDE the container's
// `.svelte-kit` volume satisfies both.
// The browser demo build (`pnpm build:demo`, `CMDR_DEMO_BUILD=1`): the same frontend on the fake
// backend in `src/lib/demo/`, served from getcmdr.com/demo, so it gets that base path and its own
// output dir (never `build`, which Tauri embeds). Every other build gets `__CMDR_DEMO_BUILD__`
// false and `#lib/demo/boot.js` aliased to an empty module (grep a bundle for `__cmdrDemo`). See
// `src/lib/demo/DETAILS.md`.
const demoBuild = process.env.CMDR_DEMO_BUILD === '1'

const pagesDir = process.env.CMDR_FRONTEND_BUILD_DIR ?? (demoBuild ? 'build-demo' : 'build')

// The demo's WASM listing engine (`pnpm build:listing-wasm`'s output, gitignored).
const listingWasmDir = fileURLToPath(new URL('../../crates/cmdr-listing-wasm/pkg', import.meta.url))

/**
 * Bakes the old-WebKit boot guard's translated strings into the app shell, and
 * returns the path SvelteKit should read the shell from.
 *
 * The guard is an inline ES5 script in `src/app.html` that runs before the module
 * bundle, on a WebKit that may not be able to parse that bundle at all, so it
 * can't reach the i18n runtime. Its copy is resolved from the message catalogs
 * here instead (`scripts/gen-boot-guard-lib.ts`).
 *
 * Why HERE and not in a Vite plugin or a committed generated file: SvelteKit
 * reads `files.appTemplate` with a plain `readFileSync`, outside Vite's
 * plugin pipeline, so `transformIndexHtml` never sees it. Running at config-load
 * time is the one hook guaranteed to be earlier, and it means the shipped shell
 * cannot drift from the catalog: there's no committed copy to go stale and no
 * freshness check to keep honest.
 *
 * `src/app.html` stays the file you edit. The trade is that `pnpm dev` doesn't
 * hot-reload an edit to it, since SvelteKit watches the generated path; restart
 * the dev server after touching the shell.
 *
 * `VITE_CMDR_FORCE_OLD_WEBKIT=unsupported` bakes in a forced block, which is how
 * the screen is reachable on a modern Mac. See
 * `src/lib/utils/DETAILS.md` § The two old-WebKit answers.
 */
function generateAppTemplate() {
  const desktopDir = dirname(fileURLToPath(import.meta.url))
  const template = readFileSync(join(desktopDir, 'src', 'app.html'), 'utf8')
  const data = buildBootGuardData({
    locales: listLocales(),
    force: process.env.VITE_CMDR_FORCE_OLD_WEBKIT === 'unsupported',
  })
  const outDir = join(desktopDir, '.svelte-kit')
  const outFile = join(outDir, 'cmdr-app.html')
  mkdirSync(outDir, { recursive: true })
  writeFileSync(outFile, injectBootGuardData(template, data))
  return outFile
}

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

// Every E2E build: `test:e2e:playwright:build` (the Playwright lane's binary, which the i18n
// screenshot run reuses) and the Linux Docker build set `CMDR_E2E_BUILD=1`. It bakes in the
// harness-only instruments: the i18n capture sink (`window.__cmdrI18nCapture`, inert until the
// harness calls `enable()`; see `src/lib/intl/messages.svelte.ts`) and the dialog gallery the
// capture photographs and `dialog-inset.spec.ts` measures. A production build and `pnpm dev` set
// neither, so esbuild dead-code-eliminates the lot: verifiably absent from prod (grep the bundle
// for `__cmdrI18nCapture`). Sites dev also wants gate on `import.meta.env.DEV || __CMDR_E2E_BUILD__`.
const e2eBuild = process.env.CMDR_E2E_BUILD === '1'

// Dev-only label of which working tree this session runs against (worktree slug, "main", or
// the worktree directory name), set by the wrapper (scripts/tauri-wrapper.js). The dev-mode
// title bar wraps it around the window title so side-by-side worktree windows are tellable
// apart. Empty for prod builds and plain `vite dev` outside the wrapper. See
// `src/lib/app-mode.ts`.
const worktreeLabel = process.env.CMDR_WORKTREE_LABEL ?? ''

// Set by the release workflow's `sbom` job, for a build whose only product is
// `build/sbom/frontend.cdx.json` (see `scripts/vite-frontend-sbom.ts`). Every other build leaves
// it unset, so the shipped app carries no SBOM file.
const withFrontendSbom = process.env.CMDR_FRONTEND_SBOM === '1'

export default defineConfig(async () => ({
  // `localeCatalogs` serves the message catalogs: one chunk per locale, metadata
  // stripped, plus the always-loaded manifest. `vitest.config.ts` loads it too, so
  // tests read the same catalogs the app does.
  plugins: [
    localeCatalogs(),
    // The demo's void commands (`virtual:cmdr-demo-void-commands`), read off `bindings.ts`.
    ...(demoBuild ? [demoVoidCommands()] : []),
    Icons({ compiler: 'svelte' }),
    sveltekit({
      ...svelteOptions,
      adapter: adapter({
        fallback: 'index.html',
        pages: pagesDir,
        assets: pagesDir,
      }),
      files: {
        appTemplate: generateAppTemplate(),
      },
      paths: { base: demoBuild ? '/demo' : '' },
    }),
    ...(withFrontendSbom ? [frontendSbom()] : []),
    // `build.target` can't lower a regex feature or a runtime API, so this fails the build on a
    // regex or an unchecked call the Safari 15.4 floor can't run (0.59.0's blank window on
    // Catalina). See the plugin's header.
    webkitFloor(),
  ],

  // Outside the demo build, `hooks.client.ts`'s import of the demo backend resolves to an empty
  // module, so none of `src/lib/demo/` reaches the graph. `boot.ts`'s own `__CMDR_DEMO_BUILD__`
  // guard alone would leave any module-level demo data in the bundle.
  // In the demo build, `listing.ts`'s `virtual:cmdr-listing-wasm` is the WASM listing engine's JS
  // glue, built by `pnpm build:listing-wasm` (which `build:demo` and `dev:demo` run first).
  resolve: {
    alias: demoBuild
      ? [
          {
            find: /^virtual:cmdr-listing-wasm$/,
            replacement: join(listingWasmDir, 'cmdr_listing_wasm.js'),
          },
        ]
      : [
          {
            find: /^#lib\/demo\/boot\.js$/,
            replacement: fileURLToPath(new URL('./src/lib/demo/boot-absent.ts', import.meta.url)),
          },
        ],
  },

  define: {
    __CMDR_E2E_BUILD__: JSON.stringify(e2eBuild),
    __CMDR_DEMO_BUILD__: JSON.stringify(demoBuild),
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
    // SvelteKit's allow list covers the app; the demo's engine fetches its `.wasm` from outside it.
    fs: demoBuild ? { allow: [listingWasmDir] } : undefined,
  },
}))
