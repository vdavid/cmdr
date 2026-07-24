// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from '@sveltejs/adapter-static'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'

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
  },
}

export default config
