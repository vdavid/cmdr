/**
 * Loads the acknowledgements lists on demand, for `AcknowledgementsDialog.svelte`.
 *
 * Loaded on open, not at startup: the generated JSON is ~119 KB and nothing else in
 * the app needs it. Vite code-splits the `import()`.
 *
 * ❗ A module of its own so tests stub THIS (a static import) and never the JSON behind
 * the `import()`. Vitest's mock of that dynamically imported JSON misses once another
 * dynamic import has run earlier in the same test file, silently handing back the real
 * ~850 rows: the licensing a11y suite scanned the full list, took seconds, and timed out
 * on a loaded CI runner (verified on vitest 4, 2026-09-24).
 */

export interface AttributedPackage {
  name: string
  version: string
  license: string
  url: string
}

export interface ThirdPartyPackages {
  vendored: AttributedPackage[]
  rust: AttributedPackage[]
  npm: AttributedPackage[]
}

export async function loadThirdPartyPackages(): Promise<ThirdPartyPackages> {
  const packages = await import('./third-party-packages.gen.json')
  return packages.default
}
