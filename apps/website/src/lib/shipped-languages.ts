/**
 * How many languages the app ships, counted at build time from the desktop's generated
 * shipped-locale table, so no page has to state the number by hand. That table is the list the
 * Settings language dropdown offers (the `en` source, every full translation, and the regional
 * overlays like `en-GB`), it excludes the `en-XA` pseudolocale, and its `shipped-locales-fresh`
 * check keeps it equal to the catalog dirs. Copy the build can't reach (the listings, the README,
 * blog posts) is held to the same number by the `copy-language-count` check.
 */
import shippedLocalesRs from '../../../desktop/src-tauri/src/intl/shipped_locales.gen.rs?raw'

/** The tags in `SHIPPED_LOCALES`, read off the generated Rust table (layout pinned by its generator). */
export function parseShippedLocaleTags(rust: string): string[] {
  const table = /SHIPPED_LOCALES: &\[ShippedLocale\] = &\[([\s\S]*?)\n\];/.exec(rust)
  if (!table) {
    throw new Error('shipped_locales.gen.rs has no SHIPPED_LOCALES table; did gen-shipped-locales.ts change shape?')
  }
  const tags = [...table[1].matchAll(/^\s*tag: "([^"]+)",$/gm)].map((match) => match[1])
  if (tags.length === 0) {
    throw new Error('shipped_locales.gen.rs lists no locales; did gen-shipped-locales.ts change shape?')
  }
  return tags
}

export const shippedLocaleTags = parseShippedLocaleTags(shippedLocalesRs)

/** The number for copy like "Translated into N languages." */
export const shippedLanguageCount = shippedLocaleTags.length
