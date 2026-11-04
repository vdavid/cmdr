/**
 * SvelteKit's client hooks.
 *
 * `handleError` sees every throw SvelteKit catches: a component that threw while rendering, a
 * `load`, and a throw inside a `<svelte:boundary>` (SvelteKit routes boundaries through it via
 * Svelte's `transformError`). Without it the throw only reached the devtools console, which
 * nobody reads under Tauri, and SvelteKit handed the error page and every boundary a bare
 * `{ status: 500, message: 'Internal Error' }`, so neither a crashed window nor `DialogManager`'s
 * boundary (ERR-VEZUB) could say what broke.
 *
 * SvelteKit merges what this returns over its fallback, so it returns only the fields it overrides.
 */

// First import, so in a demo build the fake backend is in place before any other module runs.
// `boot.ts` installs itself behind a `__CMDR_DEMO_BUILD__` guard, so every other build tree-shakes
// all of `#lib/demo/`. Never a top-level `await import()` instead: WebKit can run an importer
// before an awaiting module finishes, and Safari visitors run the demo too.
import '#lib/demo/boot.js'
import type { HandleClientError } from '@sveltejs/kit/hooks'
import { logPageCrash } from '#lib/logging/uncaught-errors.js'

export const handleError: HandleClientError = ({ kind, error, event }) => {
  // SvelteKit's own errors (a route that doesn't exist) aren't crashes, so they stay out of the log.
  if (kind === 'framework') return { detail: error.message }
  const detail = logPageCrash({ error, routeId: event.route.id ?? event.url.pathname })
  // An app error (`error(404, …)`) already carries a readable message; an unknown throw would
  // otherwise read "Internal Error" to the error page and to a boundary.
  if (kind === 'app') return { detail }
  return { message: error instanceof Error ? `${error.name}: ${error.message}` : detail, detail }
}
