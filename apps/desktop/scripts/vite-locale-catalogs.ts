/**
 * Vite plugin: one module per locale catalog, plus a small always-loaded manifest, so a
 * window loads English and the language it speaks instead of every catalog we ship.
 *
 * ## The two virtual modules
 *
 * - `virtual:cmdr-catalog/<tag>`: every `messages/<tag>/*.json` merged into one object, `@key`
 *   metadata stripped, as `export default {...}`. The runtime imports `en` statically and every
 *   other tag through the manifest's loaders, so each becomes its own chunk.
 * - `virtual:cmdr-locale-manifest`: the shipped tags, the few messages every window needs from
 *   EVERY catalog (`MANIFEST_KEYS`: Settings search indexes the Language label in every
 *   language), and one `() => import('virtual:cmdr-catalog/<tag>')` loader per non-English tag.
 *
 * ## Why a plugin, not `import.meta.glob`
 *
 * A lazy glob over `messages/*\/*.json` makes one chunk per FILE (every area × every locale), so a
 * language switch would be dozens of requests, and the per-tag merge and the shipped-tag list
 * would move into the runtime. Here a switch is one request, the merge happens at build time, and
 * the shipped list is data in the manifest.
 *
 * ## Which dirs are locales
 *
 * `listLocales()`, the answer every i18n check uses, narrowed to BCP 47-shaped names. So
 * `screenshots/` (translator tooling; `capture-report.json` alone was ~280 kB, 4.5% of the whole
 * frontend, before it was kept out) can't become a language, and it never reaches the bundle:
 * nothing imports a file the manifest doesn't name. `en-XA`, the gitignored pseudolocale, ships
 * exactly when it's on disk at build time.
 *
 * ## Dev
 *
 * Each module registers the files it read with `addWatchFile`, so editing a catalog in `pnpm dev`
 * reloads the page with the new text. A NEW locale dir needs a dev-server restart.
 */

import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { listLocales, resolveMessagesRoot, splitCatalogFile } from './i18n-catalog-lib.ts'

export const CATALOG_MODULE_PREFIX = 'virtual:cmdr-catalog/'
export const MANIFEST_MODULE_ID = 'virtual:cmdr-locale-manifest'

/** Messages the manifest carries from every catalog. Keep it tiny: every window loads it. */
export const MANIFEST_KEYS: readonly string[] = ['settings.appearance.language.label']

/** A 2–3 letter language subtag plus optional `-` subtags: `en`, `zh-Hant`, `es-419`, `en-XA`. */
const BCP47_DIR = /^[a-z]{2,3}(-[a-z0-9]+)*$/i

/** The shipped locale tags under `messagesRoot`, sorted. */
export function shippedLocaleTags(messagesRoot: string): string[] {
  return listLocales(messagesRoot).filter((tag) => BCP47_DIR.test(tag))
}

function catalogFiles(messagesRoot: string, tag: string): string[] {
  const dir = join(messagesRoot, tag)
  return readdirSync(dir)
    .filter((f) => f.endsWith('.json'))
    .sort()
    .map((f) => join(dir, f))
}

/** `tag`'s catalog files merged, metadata stripped, through the same split the checks use. */
export function mergedCatalog(messagesRoot: string, tag: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const file of catalogFiles(messagesRoot, tag)) {
    const { messages } = splitCatalogFile(JSON.parse(readFileSync(file, 'utf8')) as Record<string, unknown>)
    Object.assign(out, messages)
  }
  return out
}

/**
 * `JSON.stringify` for text spliced into generated JS. JSON leaves `<`, `>`, `/`, U+2028, and U+2029
 * raw, and those break out of an inline `<script>` or an older parser; each becomes its `\uXXXX`
 * escape, which parses back to the same value (CodeQL `js/bad-code-sanitization`).
 */
function toCodeLiteral(value: unknown): string {
  return JSON.stringify(value).replace(
    /[<>/\u2028\u2029]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`,
  )
}

/** The source of `virtual:cmdr-catalog/<tag>`. */
export function catalogModuleCode(messagesRoot: string, tag: string): string {
  return `export default ${toCodeLiteral(mergedCatalog(messagesRoot, tag))};\n`
}

/** The source of `virtual:cmdr-locale-manifest`. */
export function manifestModuleCode(messagesRoot: string): string {
  const tags = shippedLocaleTags(messagesRoot)
  const manifestMessages: Record<string, Record<string, string>> = {}
  for (const tag of tags) {
    const catalog = mergedCatalog(messagesRoot, tag)
    const picked: Record<string, string> = {}
    for (const key of MANIFEST_KEYS) if (key in catalog) picked[key] = catalog[key]
    manifestMessages[tag] = picked
  }
  const loaders = tags
    .filter((tag) => tag !== 'en')
    .map((tag) => `  ${toCodeLiteral(tag)}: () => import(${toCodeLiteral(CATALOG_MODULE_PREFIX + tag)}),`)
  return [
    `export const shippedLocales = ${toCodeLiteral(tags)};`,
    `export const manifestMessages = ${toCodeLiteral(manifestMessages)};`,
    `export const loaders = {\n${loaders.join('\n')}\n};`,
    '',
  ].join('\n')
}

interface LoadContext {
  addWatchFile: (file: string) => void
}

/** The plugin. `messagesRoot` is for tests; the app reads `src/lib/intl/messages/`. */
export function localeCatalogs(options: { messagesRoot?: string } = {}): {
  name: string
  enforce: 'pre'
  resolveId: (id: string) => string | null
  load: (this: LoadContext, id: string) => string | null
} {
  const root = resolveMessagesRoot(options.messagesRoot)
  const watchAll = (ctx: LoadContext, tags: string[]): void => {
    for (const tag of tags) for (const file of catalogFiles(root, tag)) ctx.addWatchFile(file)
  }
  return {
    name: 'cmdr:locale-catalogs',
    enforce: 'pre',
    resolveId(id: string) {
      if (id === MANIFEST_MODULE_ID || id.startsWith(CATALOG_MODULE_PREFIX)) return `\0${id}`
      return null
    },
    load(id: string) {
      if (id === `\0${MANIFEST_MODULE_ID}`) {
        watchAll(this, shippedLocaleTags(root))
        return manifestModuleCode(root)
      }
      if (id.startsWith(`\0${CATALOG_MODULE_PREFIX}`)) {
        const tag = id.slice(CATALOG_MODULE_PREFIX.length + 1)
        if (!shippedLocaleTags(root).includes(tag)) throw new Error(`No locale catalog named "${tag}"`)
        watchAll(this, [tag])
        return catalogModuleCode(root, tag)
      }
      return null
    },
  }
}
