// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import adapter from '@sveltejs/adapter-static'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'
import { listLocales } from './scripts/i18n-catalog-lib.ts'
import { buildBootGuardData, injectBootGuardData } from './scripts/gen-boot-guard-lib.ts'

// A11y warnings to suppress (same as in package.json check script)
const suppressedWarnings = [
  'a11y_no_noninteractive_element_interactions',
  'a11y_click_events_have_key_events',
  'a11y_no_noninteractive_tabindex',
  'a11y_interactive_supports_focus',
  'state_referenced_locally',
  'non_reactive_update',
]

// Where adapter-static writes the site. Default `build` matches tauri.conf.json's
// `frontendDist: "../build"`. The Linux-E2E Docker build overrides this (paired with a
// tauri `--config` frontendDist override in `scripts/e2e-linux.sh`): the container
// builds from the SAME bind-mounted tree the host may be building in (`pnpm check
// --include-slow` runs the host Playwright build and the container build
// concurrently), so it redirects its output into its own Docker-volume-backed dir.
// The adapter rimrafs this dir on every build, so it must never BE a mount point
// (rmdir on a mount point is EBUSY) — pointing it INSIDE the container's
// `.svelte-kit` volume satisfies both.
const pagesDir = process.env.CMDR_FRONTEND_BUILD_DIR ?? 'build'

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
 * reads `kit.files.appTemplate` with a plain `readFileSync`, outside Vite's
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

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  compilerOptions: {
    warningFilter: (warning) => !suppressedWarnings.includes(warning.code),
  },
  kit: {
    adapter: adapter({
      fallback: 'index.html',
      pages: pagesDir,
      assets: pagesDir,
    }),
    files: {
      appTemplate: generateAppTemplate(),
    },
  },
}

export default config
