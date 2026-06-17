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
 * The pure catalog-mutation core is exported (`couplingsFromReport`,
 * `coupleCatalog`, `serializeCatalog`) so it's unit-testable without touching the
 * real catalogs (see `couple-screenshots.test.js`). The CLI shell below is only
 * file I/O around that core, and runs only when invoked as a script.
 */

import { readFileSync, writeFileSync, existsSync } from 'node:fs'
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
 * @typedef {object} CoupleResult
 * @property {Record<string, unknown>} json The catalog object after coupling, with twins reordered.
 * @property {boolean} changed Whether any `@key.screenshot` was added or updated.
 * @property {number} couplingCount How many keys were (re)coupled.
 * @property {string[]} coupledWithoutDescription `key → screenshot` for keys whose twin has no description.
 * @property {string[]} missingKeys Keys requested but absent from this catalog.
 * @property {Array<{ key: string; screenshot: string; current: string | undefined }>} stale
 *   For `--check`: keys whose coupling is missing/stale (the writes that WOULD happen).
 */

/**
 * Pure core: returns a NEW catalog object with `@key.screenshot` set for every
 * requested key present in `json`, touching ONLY the `screenshot` field of each
 * twin. Message values and every other twin field are carried through unchanged.
 * Does not read or write the filesystem.
 * @param {Record<string, unknown>} json A parsed catalog (`messages/en/<area>.json`).
 * @param {Map<string, string>} keyToScreenshot key → screenshot filename for THIS catalog.
 * @returns {CoupleResult}
 */
export function coupleCatalog(json, keyToScreenshot) {
  let changed = false
  let couplingCount = 0
  /** @type {string[]} */
  const coupledWithoutDescription = []
  /** @type {string[]} */
  const missingKeys = []
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
    // Merge into the existing twin (preserving `description`/`placeholders`); we
    // only ever own the `screenshot` field. A key with no (or a malformed) twin,
    // or one whose twin has no non-empty `description`, still gets coupled but is
    // reported — the catalog convention wants a description, even though checks
    // don't require it.
    /** @type {Record<string, unknown>} */
    const meta = metaIsObject ? /** @type {Record<string, unknown>} */ (existing) : {}
    if (meta.screenshot === screenshot) continue // already coupled — idempotent
    const hasDescription = typeof meta.description === 'string' && meta.description.trim() !== ''
    if (!hasDescription) coupledWithoutDescription.push(`${key} → ${screenshot}`)
    couplingCount++
    stale.push({ key, screenshot, current: typeof meta.screenshot === 'string' ? meta.screenshot : undefined })
    meta.screenshot = screenshot
    json[metaKey] = meta
    changed = true
  }

  return {
    json: reorderTwins(json),
    changed,
    couplingCount,
    coupledWithoutDescription,
    missingKeys,
    stale,
  }
}

/**
 * Reorders the catalog so each `@key` metadata twin sits immediately after its
 * message key (the repo convention). Returns a new object; the input is not
 * mutated structurally (values are shared by reference, never altered).
 * @param {Record<string, unknown>} json
 * @returns {Record<string, unknown>}
 */
function reorderTwins(json) {
  /** @type {Record<string, unknown>} */
  const ordered = {}
  const seen = new Set()
  for (const k of Object.keys(json)) {
    if (k.startsWith('@')) continue // placed alongside its message key below
    if (seen.has(k)) continue
    ordered[k] = json[k]
    seen.add(k)
    const twin = `@${k}`
    if (twin in json) {
      ordered[twin] = json[twin]
      seen.add(twin)
    }
  }
  // Preserve any orphan `@`-entries (no message key) at the end, defensively.
  for (const k of Object.keys(json)) {
    if (!seen.has(k)) ordered[k] = json[k]
  }
  return ordered
}

/**
 * Serializes a catalog object: 2-space indent, trailing newline. (oxfmt runs
 * afterward on disk; this keeps the pre-oxfmt shape deterministic for tests.)
 * @param {Record<string, unknown>} json
 * @returns {string}
 */
export function serializeCatalog(json) {
  return JSON.stringify(json, null, 2) + '\n'
}

// ── CLI shell (file I/O only; skipped when imported as a module) ──────────────

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

  for (const [file, keyMap] of byFile) {
    const filePath = join(messagesDir, file)
    if (!existsSync(filePath)) {
      console.warn(`Skipping ${file}: no such catalog (key area without a catalog file?)`)
      continue
    }
    /** @type {Record<string, unknown>} */
    const json = JSON.parse(readFileSync(filePath, 'utf8'))
    const result = coupleCatalog(json, keyMap)

    for (const key of result.missingKeys) {
      console.warn(`Skipping ${key}: not present in ${file} (catalog may have drifted from the report)`)
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
      writeFileSync(filePath, serializeCatalog(result.json))
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

  // Normalize formatting on the changed catalogs so the result is oxfmt-clean.
  if (changedFiles.length > 0) {
    const res = spawnSync('pnpm', ['exec', 'oxfmt', ...changedFiles], { cwd: desktopDir, stdio: 'inherit' })
    if (res.status !== 0) {
      console.warn('oxfmt did not exit cleanly; run `pnpm exec oxfmt` on the changed files manually.')
    }
  }
}

// Run the CLI only when executed directly, not when imported by a test.
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main()
}
