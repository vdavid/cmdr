/**
 * Every window route that renders settings-dependent UI must initialize the
 * reactive-settings layer.
 *
 * `reactive-settings.svelte.ts` holds module-level `$state` seeded once by
 * `initReactiveSettings()`. A window that never calls it renders every reactive
 * setting at its registry default forever: sizes in binary when the user picked
 * SI, dates in ISO when they picked a custom format, and so on. Nothing throws,
 * nothing logs, so the only way to catch it is structurally.
 *
 * The check walks each route's static import graph. If any reachable module
 * imports from `reactive-settings.svelte`, the route must be covered by an
 * initialization call in itself or in one of its ancestor layouts. The root
 * layout covers every window, which is the point: a new window can't forget.
 *
 * Sibling guard: `window-settings.test.ts` pins the route → settings-access
 * classification the root layout's init reads.
 */
import { describe, it, expect } from 'vitest'
import { readFileSync, existsSync, readdirSync, statSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { windowSettingsAccess } from '$lib/settings/window-settings'

const here = path.dirname(fileURLToPath(import.meta.url))
// here = apps/desktop/src/routes → the frontend source root is one up.
const srcRoot = path.resolve(here, '..')
const routesRoot = here

/** Marks a module as reading the reactive-settings layer. */
const REACTIVE_SETTINGS_MODULE = 'reactive-settings.svelte'

/** Either name counts as "this window seeds the reactive layer". */
const INIT_CALLS = ['initWindowSettings(', 'initReactiveSettings(']

/** Static `import`/`export … from '…'` plus dynamic `import('…')`. */
const IMPORT_RE = /(?:import|export)\s+(?:[\s\S]*?)\s*from\s*['"]([^'"]+)['"]|import\s*\(\s*['"]([^'"]+)['"]\s*\)/g

/** Resolve a `$lib/…` or relative specifier to a file on disk, or `null` for a bare package. */
function resolveSpecifier(specifier: string, fromFile: string): string | null {
  let base: string
  if (specifier.startsWith('$lib/')) base = path.join(srcRoot, 'lib', specifier.slice('$lib/'.length))
  else if (specifier.startsWith('./') || specifier.startsWith('../'))
    base = path.resolve(path.dirname(fromFile), specifier)
  else return null

  const candidates = [
    base,
    `${base}.ts`,
    `${base}.js`,
    `${base}.svelte`,
    `${base}.svelte.ts`,
    path.join(base, 'index.ts'),
  ]
  for (const candidate of candidates) {
    if (existsSync(candidate) && statSync(candidate).isFile()) return candidate
  }
  return null
}

/** Every module reachable from `entry` through static and dynamic imports. */
function importClosure(entry: string): string[] {
  const seen = new Set<string>()
  const stack = [entry]
  while (stack.length > 0) {
    const file = stack.pop()
    if (file === undefined || seen.has(file)) continue
    seen.add(file)
    let source: string
    try {
      source = readFileSync(file, 'utf8')
    } catch {
      continue
    }
    IMPORT_RE.lastIndex = 0
    let match: RegExpExecArray | null
    while ((match = IMPORT_RE.exec(source)) !== null) {
      // Exactly one of the two alternation groups matches; the other is `undefined`
      // at runtime even though `RegExpExecArray` types both as `string`.
      const [, staticSpec, dynamicSpec] = match as unknown as (string | undefined)[]
      const specifier = staticSpec ?? dynamicSpec
      if (specifier === undefined) continue
      const resolved = resolveSpecifier(specifier, file)
      if (resolved !== null) stack.push(resolved)
    }
  }
  return [...seen]
}

/** Every `+page.svelte` under `src/routes`, as paths relative to the routes root. */
function windowRoutePages(): string[] {
  const found: string[] = []
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name)
      if (entry.isDirectory()) walk(full)
      else if (entry.name === '+page.svelte') found.push(path.relative(routesRoot, full))
    }
  }
  walk(routesRoot)
  return found.sort()
}

/** The layouts wrapping a page, nearest first, ending at the root layout. */
function ancestorLayouts(pageRelPath: string): string[] {
  const layouts: string[] = []
  let dir = path.dirname(pageRelPath)
  for (;;) {
    const layout = path.join(routesRoot, dir, '+layout.svelte')
    if (existsSync(layout)) layouts.push(layout)
    if (dir === '.') break
    dir = path.dirname(dir)
  }
  return layouts
}

function callsInit(file: string): boolean {
  const source = readFileSync(file, 'utf8')
  return INIT_CALLS.some((call) => source.includes(call))
}

/** `queue/+page.svelte` → `/queue`; SvelteKit group segments `(x)` drop out. */
function routePathFor(pageRelPath: string): string {
  const segments = path
    .dirname(pageRelPath)
    .split(path.sep)
    .filter((segment) => segment !== '' && segment !== '.' && !segment.startsWith('('))
  return `/${segments.join('/')}`
}

describe('reactive-settings coverage', () => {
  const pages = windowRoutePages()

  it('finds the window route pages', () => {
    expect(pages.length).toBeGreaterThan(0)
  })

  it('initializes the reactive layer in every window that renders settings-dependent UI', () => {
    const uncovered: { route: string; via: string }[] = []

    for (const page of pages) {
      const entry = path.join(routesRoot, page)
      const closure = importClosure(entry)
      const consumer = closure.find(
        (file) =>
          !file.endsWith(`${REACTIVE_SETTINGS_MODULE}.ts`) &&
          readFileSync(file, 'utf8').includes(REACTIVE_SETTINGS_MODULE),
      )
      if (consumer === undefined) continue

      const initialized = [entry, ...ancestorLayouts(page)].some(callsInit)
      if (!initialized) uncovered.push({ route: page, via: path.relative(srcRoot, consumer) })
    }

    expect(
      uncovered,
      'These window routes render settings-dependent UI but never seed the reactive layer, ' +
        'so every reactive setting silently falls back to its registry default. ' +
        'Initialize it in the route or in an ancestor layout.',
    ).toEqual([])
  })

  it('resolves a store-access classification for every window route', () => {
    // The root layout's `initWindowSettings()` reads this to pick the store path.
    // An unmapped route falls back to `'full'`, which throws in a window whose
    // capability file has no store grant — so every route must be mapped.
    for (const page of pages) {
      expect(['full', 'restricted'], page).toContain(windowSettingsAccess(routePathFor(page)))
    }
  })
})
