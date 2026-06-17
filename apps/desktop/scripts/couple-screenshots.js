#!/usr/bin/env node
/**
 * Couples catalog keys to screenshots from an i18n capture run.
 *
 * Reads the capture report written by `test/e2e-playwright/i18n-capture.spec.ts`
 * (`src/lib/intl/messages/screenshots/capture-report.json`, a SIBLING of `en/`:
 * surface → its recorded keys + screenshot filename) and writes `@key.screenshot`
 * into the right `messages/en/*.json` for every key that rendered on a captured
 * surface.
 *
 * Coupling policy (kept simple, one screenshot per key): a key may render on
 * several surfaces; it's assigned the FIRST surface it appeared on, in the
 * report's insertion order (the spec orders surfaces narrow-to-broad, so the
 * most specific surface a key belongs to wins). Re-runnable and idempotent: a
 * second run with the same report produces no diff. It only ADDS or UPDATES the
 * `screenshot` field; it never removes a key's existing manual coupling for a
 * surface that wasn't in this run.
 *
 * Run via `pnpm i18n:couple` (after `pnpm i18n:capture`), or directly with
 * `node scripts/couple-screenshots.js`. Pass `--check` to fail (exit 1) if any
 * coupling is missing/stale instead of writing — useful in CI once a full
 * capture exists.
 *
 * The `@key` metadata (including `screenshot`) is stripped by the runtime and by
 * `gen-message-keys.js`, so this never changes rendered output or the key union.
 *
 * Alongside coupling, it writes a TRACKED coverage report
 * (`messages/screenshots/coverage-report.md`): per catalog area, how many keys are
 * coupled to a screenshot vs not, and for the uncoupled ones a likely-reason
 * bucket (dynamic-only keys that no static surface can name, vs keys on a surface
 * the driver doesn't visit yet). Coverage is partial by design until the driver
 * covers the full surface inventory, so the report says so rather than implying
 * gaps are bugs (Decision 4: no silent gaps).
 *
 * The pure cores are exported (`couplingsFromReport`, `coupleCatalog`,
 * `buildCoverageReport`, `fileForKey`) so they're unit-testable without touching
 * the real catalogs (see `couple-screenshots.test.js`). The CLI shell below is
 * only file I/O around those cores, and runs only when invoked as a script.
 */

import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

/**
 * The catalog filename a key belongs to: first dot-segment + `.json`.
 * @param {string} key
 * @returns {string}
 */
export function fileForKey(key) {
  const area = key.split('.')[0]
  return `${area}.json`
}

/**
 * Flattens a capture report (surface → { screenshot, keys }) into a key →
 * screenshot map, first-surface-wins in the report's insertion order.
 * @param {Record<string, { screenshot: string; keys: string[] }>} report
 * @returns {Map<string, string>}
 */
export function couplingsFromReport(report) {
  /** @type {Map<string, string>} */
  const keyToScreenshot = new Map()
  for (const { screenshot, keys } of Object.values(report)) {
    for (const key of keys) {
      if (!keyToScreenshot.has(key)) keyToScreenshot.set(key, screenshot)
    }
  }
  return keyToScreenshot
}

/**
 * Message-key prefixes whose keys are assembled at runtime (a reason variable
 * spliced into the dotted path), so the static capture report can never name them
 * individually — the rendered surface records the RESOLVED key only if capture is
 * active at resolution time. Uncoupled keys under one of these are bucketed as
 * "dynamic-only" in the coverage report rather than as a missed surface. Mirrors
 * `unusedKeyDynamicPrefixes` in `scripts/check/checks/desktop-message-keys-unused.go`
 * (kept in sync by hand; both are small, closed, and tied to live construction
 * sites in `apps/desktop/src/lib/`).
 * @type {string[]}
 */
export const DYNAMIC_KEY_PREFIXES = ['errors.git.', 'errors.listing.', 'errors.provider.', 'errors.write.']

/**
 * @typedef {object} AreaCoverage
 * @property {string} area The catalog area (filename minus `.json`).
 * @property {number} total Renderable keys in the area.
 * @property {number} coupled Keys with a screenshot coupling from this run.
 * @property {number} dynamicUncoupled Uncoupled keys under a dynamic prefix.
 * @property {number} surfaceUncoupled Uncoupled keys NOT under a dynamic prefix (surface not driven yet).
 */

/**
 * @typedef {object} CoverageReport
 * @property {AreaCoverage[]} areas Per-area coverage rows, sorted by area name.
 * @property {number} total Renderable keys across all areas.
 * @property {number} coupled Coupled keys across all areas.
 * @property {number} dynamicUncoupled Uncoupled dynamic-only keys across all areas.
 * @property {number} surfaceUncoupled Uncoupled not-yet-driven keys across all areas.
 */

/**
 * Pure coverage core: given every renderable catalog key (by area) and the
 * key→screenshot couplings from this run, tallies per area how many keys are
 * coupled vs not, splitting the uncoupled into "dynamic-only" (built at runtime,
 * see DYNAMIC_KEY_PREFIXES) and "surface not driven yet". No filesystem access.
 * @param {Map<string, string>} keyToScreenshot key → screenshot for coupled keys.
 * @param {Map<string, string[]>} keysByArea area → its renderable keys.
 * @param {string[]} [dynamicPrefixes] prefixes whose keys are runtime-assembled.
 * @returns {CoverageReport}
 */
export function buildCoverageReport(keyToScreenshot, keysByArea, dynamicPrefixes = DYNAMIC_KEY_PREFIXES) {
  /** @type {AreaCoverage[]} */
  const areas = []
  let total = 0
  let coupled = 0
  let dynamicUncoupled = 0
  let surfaceUncoupled = 0

  for (const area of [...keysByArea.keys()].sort()) {
    const keys = keysByArea.get(area) ?? []
    let areaCoupled = 0
    let areaDynamic = 0
    let areaSurface = 0
    for (const key of keys) {
      if (keyToScreenshot.has(key)) {
        areaCoupled++
      } else if (dynamicPrefixes.some((p) => key.startsWith(p))) {
        areaDynamic++
      } else {
        areaSurface++
      }
    }
    areas.push({
      area,
      total: keys.length,
      coupled: areaCoupled,
      dynamicUncoupled: areaDynamic,
      surfaceUncoupled: areaSurface,
    })
    total += keys.length
    coupled += areaCoupled
    dynamicUncoupled += areaDynamic
    surfaceUncoupled += areaSurface
  }

  return { areas, total, coupled, dynamicUncoupled, surfaceUncoupled }
}

/**
 * Renders a CoverageReport as Markdown for the tracked artifact. Kept text + small
 * so its diff stays readable. Pure (no filesystem, no Date — the caller stamps any
 * timestamp), so it's snapshot-testable.
 * @param {CoverageReport} report
 * @returns {string}
 */
export function renderCoverageReport(report) {
  /** @param {number} n @param {number} d @returns {string} */
  const pct = (n, d) => (d === 0 ? '—' : `${String(Math.round((n / d) * 100))}%`)
  const lines = [
    '# Screenshot coverage',
    '',
    'Generated by `scripts/couple-screenshots.js` (via `pnpm i18n:shots`). Tracked, regenerable.',
    '',
    'Per catalog area: how many renderable keys are coupled to a screenshot vs not. Uncoupled keys split into',
    '`dynamic-only` (built at runtime, so no static surface can name them — see `DYNAMIC_KEY_PREFIXES`) and `not driven`',
    '(on a surface the capture driver does not visit yet).',
    '',
    `Coverage is PARTIAL until the driver covers the full surface inventory. Low numbers here are expected, not bugs.`,
    '',
    `**Total: ${String(report.coupled)} / ${String(report.total)} keys coupled (${pct(report.coupled, report.total)}).** ` +
      `${String(report.dynamicUncoupled)} dynamic-only and ${String(report.surfaceUncoupled)} not-yet-driven keys remain uncoupled.`,
    '',
    '| Area | Coupled | Total | % | Dynamic-only | Not driven |',
    '| --- | ---: | ---: | ---: | ---: | ---: |',
  ]
  for (const a of report.areas) {
    lines.push(
      `| ${a.area} | ${String(a.coupled)} | ${String(a.total)} | ${pct(a.coupled, a.total)} | ${String(a.dynamicUncoupled)} | ${String(a.surfaceUncoupled)} |`,
    )
  }
  lines.push('')
  return lines.join('\n')
}

/**
 * @typedef {object} CoupleResult
 * @property {string} text The catalog TEXT after coupling (line-surgical; all other bytes byte-identical).
 * @property {boolean} changed Whether any `@key.screenshot` was added or updated.
 * @property {number} couplingCount How many keys were (re)coupled.
 * @property {string[]} coupledWithoutDescription `key → screenshot` for keys whose twin has no description.
 * @property {string[]} missingKeys Keys requested but absent from this catalog.
 * @property {string[]} missingTwins Keys present but with no `@key` twin to host the screenshot (skipped).
 * @property {Array<{ key: string; screenshot: string; current: string | undefined }>} stale
 *   For `--check`: keys whose coupling is missing/stale (the writes that WOULD happen).
 */

/**
 * Pure core: returns the catalog TEXT with `@key.screenshot` set for every
 * requested key, edited LINE-SURGICALLY so every other byte — message values,
 * other twin fields, indentation, AND the blank lines that group the catalog —
 * is preserved exactly. (A `JSON.parse` → `JSON.stringify` round-trip would drop
 * the blank-line grouping; oxfmt doesn't restore it, so it would reflatten every
 * file on every run. The spec's gotcha: preserve oxfmt'd formatting, touch ONLY
 * `@key.screenshot`.) Does not read or write the filesystem.
 *
 * We parse the JSON once (read-only) to learn which keys exist, their current
 * screenshot (for idempotency), and whether the twin has a description; the
 * actual mutation is on the raw text.
 * @param {string} rawText The catalog file contents (`messages/en/<area>.json`).
 * @param {Map<string, string>} keyToScreenshot key → screenshot filename for THIS catalog.
 * @returns {CoupleResult}
 */
export function coupleCatalog(rawText, keyToScreenshot) {
  /** @type {Record<string, unknown>} */
  const json = JSON.parse(rawText)
  let text = rawText
  let changed = false
  let couplingCount = 0
  /** @type {string[]} */
  const coupledWithoutDescription = []
  /** @type {string[]} */
  const missingKeys = []
  /** @type {string[]} */
  const missingTwins = []
  /** @type {Array<{ key: string; screenshot: string; current: string | undefined }>} */
  const stale = []

  for (const [key, screenshot] of keyToScreenshot) {
    if (!(key in json)) {
      missingKeys.push(key)
      continue
    }
    const metaKey = `@${key}`
    const existing = json[metaKey]
    const metaIsObject = typeof existing === 'object' && existing !== null && !Array.isArray(existing)
    if (!metaIsObject) {
      // No `@key` twin to host the screenshot. The migration gave every key a
      // twin, so this is rare; skip rather than synthesize a twin in raw text.
      missingTwins.push(key)
      continue
    }
    const meta = /** @type {Record<string, unknown>} */ (existing)
    if (meta.screenshot === screenshot) continue // already coupled — idempotent

    const next = setTwinScreenshot(text, metaKey, screenshot)
    if (next === null) {
      // Shouldn't happen (the twin parsed as an object), but never corrupt a file.
      missingTwins.push(key)
      continue
    }
    text = next
    const hasDescription = typeof meta.description === 'string' && meta.description.trim() !== ''
    if (!hasDescription) coupledWithoutDescription.push(`${key} → ${screenshot}`)
    couplingCount++
    stale.push({ key, screenshot, current: typeof meta.screenshot === 'string' ? meta.screenshot : undefined })
    changed = true
  }

  return { text, changed, couplingCount, coupledWithoutDescription, missingKeys, missingTwins, stale }
}

/**
 * Sets the `screenshot` field of one `"@key": { … }` object in the catalog text,
 * touching only that object. Replaces an existing `"screenshot": "…"` line if
 * present, else inserts the field as the last property (appending a comma to the
 * previous last line). Returns the new text, or null if the twin block can't be
 * located/parsed (caller then skips, never corrupting the file).
 * @param {string} text
 * @param {string} metaKey e.g. `@common.ok`
 * @param {string} screenshot
 * @returns {string | null}
 */
function setTwinScreenshot(text, metaKey, screenshot) {
  // The twin is an oxfmt'd object opening on its own line: `  "@key": {`.
  const open = `  ${JSON.stringify(metaKey)}: {`
  const openIdx = text.indexOf(open)
  if (openIdx === -1) return null
  // Walk from the `{` tracking brace depth (skipping braces inside strings) to
  // find this object's matching close brace — `placeholders` nests, so a naive
  // "first }" is wrong.
  const braceStart = openIdx + open.length - 1 // index of the `{`
  let depth = 0
  let inStr = false
  let esc = false
  let closeIdx = -1
  for (let i = braceStart; i < text.length; i++) {
    const c = text[i]
    if (inStr) {
      if (esc) esc = false
      else if (c === '\\') esc = true
      else if (c === '"') inStr = false
      continue
    }
    if (c === '"') inStr = true
    else if (c === '{') depth++
    else if (c === '}') {
      depth--
      if (depth === 0) {
        closeIdx = i
        break
      }
    }
  }
  if (closeIdx === -1) return null

  const body = text.slice(braceStart + 1, closeIdx) // between the braces
  const before = text.slice(0, braceStart + 1)
  const after = text.slice(closeIdx)

  // Replace an existing top-level `"screenshot": "…"` in this object if present.
  // (Top-level: 4-space indent, the twin's own field indent.)
  const existingRe = /\n {4}"screenshot": "(?:[^"\\]|\\.)*"(,?)/
  const m = existingRe.exec(body)
  if (m) {
    const replaced = body.replace(existingRe, `\n    "screenshot": ${JSON.stringify(screenshot)}${m[1]}`)
    return before + replaced + after
  }

  // Insert as the last field: append a comma to the current last property line,
  // then add the screenshot line before the closing brace. `body` ends with the
  // last field then a newline + the close brace's indent; trim that trailing
  // newline/indent, add `,\n    "screenshot": "…"\n  ` back.
  const trimmed = body.replace(/\n {2}$/, '') // drop the "\n  " before `}`
  return before + trimmed + `,\n    "screenshot": ${JSON.stringify(screenshot)}\n  ` + after
}

// ── CLI shell (file I/O only; skipped when imported as a module) ──────────────

/**
 * Reads every `en/*.json` catalog and returns area → its renderable keys (the
 * `@key` metadata twins dropped), matching the runtime + codegen's key set. Used
 * to compute coverage over the WHOLE catalog, not just the captured subset.
 * @param {string} messagesDir absolute path to `messages/en`
 * @returns {Map<string, string[]>}
 */
function keysByAreaFromCatalogs(messagesDir) {
  /** @type {Map<string, string[]>} */
  const byArea = new Map()
  for (const name of readdirSync(messagesDir)) {
    if (!name.endsWith('.json')) continue
    const area = name.slice(0, -'.json'.length)
    /** @type {Record<string, unknown>} */
    const json = JSON.parse(readFileSync(join(messagesDir, name), 'utf8'))
    const keys = Object.keys(json).filter((k) => !k.startsWith('@'))
    byArea.set(area, keys)
  }
  return byArea
}

function main() {
  const here = dirname(fileURLToPath(import.meta.url))
  const desktopDir = join(here, '..')
  const messagesRoot = join(desktopDir, 'src', 'lib', 'intl', 'messages')
  // Catalog writes go under `messages/en/`; the capture report lives in
  // `messages/screenshots/`, a SIBLING of `en/` (where the spec writes it).
  const messagesDir = join(messagesRoot, 'en')
  const reportPath = join(messagesRoot, 'screenshots', 'capture-report.json')

  const checkOnly = process.argv.includes('--check')

  if (!existsSync(reportPath)) {
    console.error(`No capture report at ${reportPath}.\nRun \`pnpm i18n:capture\` first to produce it.`)
    process.exit(1)
  }

  /** @type {Record<string, { screenshot: string; keys: string[] }>} */
  const report = JSON.parse(readFileSync(reportPath, 'utf8'))
  const keyToScreenshot = couplingsFromReport(report)

  // Group target keys by their catalog file so each file is read/written once.
  /** @type {Map<string, Map<string, string>>} filename → (key → screenshot) */
  const byFile = new Map()
  for (const [key, screenshot] of keyToScreenshot) {
    const file = fileForKey(key)
    let m = byFile.get(file)
    if (m === undefined) {
      m = new Map()
      byFile.set(file, m)
    }
    m.set(key, screenshot)
  }

  const changedFiles = []
  let couplingCount = 0
  const staleForCheck = []
  const coupledWithoutDescription = []
  const missingTwins = []

  for (const [file, keyMap] of byFile) {
    const filePath = join(messagesDir, file)
    if (!existsSync(filePath)) {
      console.warn(`Skipping ${file}: no such catalog (key area without a catalog file?)`)
      continue
    }
    const result = coupleCatalog(readFileSync(filePath, 'utf8'), keyMap)

    for (const key of result.missingKeys) {
      console.warn(`Skipping ${key}: not present in ${file} (catalog may have drifted from the report)`)
    }
    for (const key of result.missingTwins) {
      missingTwins.push(`${key} (in ${file})`)
    }
    couplingCount += result.couplingCount
    coupledWithoutDescription.push(...result.coupledWithoutDescription)
    if (checkOnly) {
      for (const { key, screenshot, current } of result.stale) {
        staleForCheck.push(`${key} → ${screenshot} (currently ${JSON.stringify(current)})`)
      }
      continue
    }
    if (result.changed) {
      writeFileSync(filePath, result.text)
      changedFiles.push(filePath)
    }
  }

  if (checkOnly) {
    if (staleForCheck.length > 0) {
      console.error(`Missing/stale screenshot couplings (${String(staleForCheck.length)}):`)
      for (const line of staleForCheck) console.error(`  - ${line}`)
      process.exit(1)
    }
    console.log('All captured keys are already coupled to their screenshots.')
    process.exit(0)
  }

  console.log(
    `Coupled ${String(couplingCount)} key(s) to screenshots across ${String(changedFiles.length)} catalog file(s).`,
  )

  if (coupledWithoutDescription.length > 0) {
    console.warn(
      `Coupled ${String(coupledWithoutDescription.length)} key(s) that lack a description twin (screenshot-only; checks still pass, but the catalog convention wants a description):`,
    )
    for (const line of coupledWithoutDescription) console.warn(`  - ${line}`)
  }

  if (missingTwins.length > 0) {
    console.warn(
      `Skipped ${String(missingTwins.length)} key(s) with no @key twin to host the screenshot (author a twin to couple them):`,
    )
    for (const line of missingTwins) console.warn(`  - ${line}`)
  }

  // Safety net: confirm the surgical edits are oxfmt-clean. With line-surgical
  // editing this is a no-op in practice (we preserve oxfmt's shape), but if a
  // future catalog has an unusual layout, oxfmt repairs it rather than leaving a
  // formatting-check failure.
  if (changedFiles.length > 0) {
    const res = spawnSync('pnpm', ['exec', 'oxfmt', ...changedFiles], { cwd: desktopDir, stdio: 'inherit' })
    if (res.status !== 0) {
      console.warn('oxfmt did not exit cleanly; run `pnpm exec oxfmt` on the changed files manually.')
    }
  }

  // Coverage report (Decision 4: no silent gaps) — a tracked, text artifact that
  // shows which areas have screenshots and which keys remain uncoupled (+ why).
  const keysByArea = keysByAreaFromCatalogs(messagesDir)
  const coverage = buildCoverageReport(keyToScreenshot, keysByArea)
  const coveragePath = join(messagesRoot, 'screenshots', 'coverage-report.md')
  writeFileSync(coveragePath, renderCoverageReport(coverage))
  console.log(
    `Wrote coverage report: ${String(coverage.coupled)}/${String(coverage.total)} keys coupled → ${coveragePath}`,
  )
}

// Run the CLI only when executed directly, not when imported by a test.
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main()
}
