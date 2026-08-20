/**
 * Vite plugin: drop ARB-style `@key` metadata from locale catalogs at BUILD time.
 *
 * ## Why this exists
 *
 * `src/lib/intl/messages.svelte.ts` eagerly globs every `messages/<locale>/*.json`
 * into one chunk, then calls `stripMetadata()` to keep only the renderable
 * strings. That call happens at RUNTIME, in the app, after the metadata has
 * already been bundled, downloaded, parsed, and turned into objects: Rollup can
 * tree-shake a module's named exports, but not properties of a JSON default
 * export, so nothing removed them earlier.
 *
 * The metadata is substantial. `en` carries the translator `description` prose
 * (77% of that catalog's bytes), and the nine translated locales each carry a
 * `sourceHash` plus a repeat of every key name. Measured on a production build
 * (2026-08-20, `pnpm build` with the pseudolocale absent, as in a release):
 * the messages chunk goes 5.63 MB -> 2.69 MB raw and 1,294 KB -> 722 KB gzipped,
 * taking the whole frontend build from 8.3 MB to 5.4 MB.
 *
 * ## What it does NOT do
 *
 * It does not reduce the catalog set: every locale still ships, because the
 * language can change live with no restart (`intl/live_locale.rs`), and the
 * eager glob is what makes that switch synchronous. Per-locale chunks are a
 * separate, larger change.
 *
 * ## Why `stripMetadata()` stays
 *
 * `vitest.config.ts` does not load this plugin, so unit tests still see raw
 * catalogs and still exercise the runtime strip. It also does the narrowing from
 * the JSON's `unknown` values to `string`, which the type system wants
 * regardless. ❌ Don't delete it as "now dead": it is the behavior this plugin
 * mirrors, and the two are pinned to each other through `splitCatalogFile`.
 */

import { NON_LOCALE_DIRS, splitCatalogFile } from './i18n-catalog-lib.ts'

/**
 * A module id under `intl/messages/<locale>/<area>.json`. Vite ids use forward
 * slashes on every platform, and an id may carry a `?query` suffix.
 */
const CATALOG_ID = /\/lib\/intl\/messages\/([^/]+)\/[^/]+\.json$/

/**
 * Whether a Vite module id is a locale catalog this plugin owns.
 *
 * Two deliberate exclusions. `NON_LOCALE_DIRS` (today `screenshots/`) sits
 * beside the locale dirs and holds its own JSON, so a path shape alone would
 * misclassify it. And an id carrying ANY query is left alone: `?raw` and `?url`
 * are a caller explicitly asking for the file as it is on disk, and silently
 * handing back different bytes than the file holds is the kind of thing nobody
 * debugs twice.
 * @param id a Vite module id
 */
export function isLocaleCatalogId(id: string): boolean {
  if (id.includes('?')) return false
  const match = CATALOG_ID.exec(id)
  return match !== null && !NON_LOCALE_DIRS.has(match[1])
}

/**
 * Strips `@key` metadata from one catalog's JSON source, returning JSON text.
 *
 * Delegates the "what is metadata" question to `splitCatalogFile`, which is also
 * what the codegen and the checks use, so this can't drift from the runtime's
 * own answer. Returns JSON (not JS) because the plugin runs `pre`, i.e. ahead of
 * Vite's built-in JSON handling, which then compiles the result as usual.
 * @param code the raw contents of an `<area>.json`
 */
export function stripCatalogSource(code: string): string {
  const { messages } = splitCatalogFile(JSON.parse(code) as Record<string, unknown>)
  return JSON.stringify(messages)
}

/** The plugin. `enforce: 'pre'` puts it ahead of Vite's JSON transform. */
export function stripCatalogMetadata(): {
  name: string
  enforce: 'pre'
  transform: (code: string, id: string) => { code: string; map: null } | null
} {
  return {
    name: 'cmdr:strip-catalog-metadata',
    enforce: 'pre',
    transform(code: string, id: string) {
      if (!isLocaleCatalogId(id)) return null
      return { code: stripCatalogSource(code), map: null }
    },
  }
}
