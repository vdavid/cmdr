/**
 * Structural guards over the window route entry points: a new window can't
 * silently come up missing a piece of per-window wiring.
 *
 * Each Cmdr window is its own webview with its own module graph, so every
 * cross-cutting layer starts empty in every one of them and has to be seeded
 * per window. Nothing throws when a window forgets — it just renders wrong
 * forever — so the only way to catch it is by reading the sources.
 *
 * Three guards live here: the reactive-settings layer, the language and
 * formatting sync, and the store-access classification.
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

/**
 * The call that gates a window's body on settings being loaded. A page making
 * it is a secondary window with its own webview, its own i18n runtime, and so
 * its own language and formatting to sync. (The root layout makes the same call
 * for every window; the pages that ALSO make it are the ones that are windows in
 * their own right.)
 */
const WINDOW_GATE_CALL = 'initWindowSettings('

/** What such a window must call to follow the UI language and the OS's formats. */
const LANGUAGE_SYNC_CALL = 'initWindowLanguageSync('

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

describe('window route coverage', () => {
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

  it('syncs the language and the OS formats in every secondary window', () => {
    // Each webview resolves its own locale. A window that seeds settings but
    // never calls `initWindowLanguageSync()` sits on the webview's own tag for
    // the rest of its life: it renders English under a Hungarian UI language,
    // formats `58.03 KB` where the main window writes `58,03 KB`, and never
    // reacts to a live language change. Nothing throws — the copy is simply in
    // the wrong language — so this reads the sources instead.
    const missing = pages.filter((page) => {
      const source = readFileSync(path.join(routesRoot, page), 'utf8')
      return source.includes(WINDOW_GATE_CALL) && !source.includes(LANGUAGE_SYNC_CALL)
    })

    expect(
      missing,
      `These windows gate on settings but never sync their language, so they stay on the webview's ` +
        `own locale forever. Call \`${LANGUAGE_SYNC_CALL})\` right after \`${WINDOW_GATE_CALL})\` in ` +
        `\`onMount\`, keep the returned teardown, and call it from \`onDestroy\`. See ` +
        `\`$lib/settings/window-settings.ts\`.`,
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
