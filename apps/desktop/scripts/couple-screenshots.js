#!/usr/bin/env node
/**
 * Couples catalog keys to screenshots from an i18n capture run.
 *
 * Reads the capture report written by `test/e2e-playwright/i18n-capture.spec.ts`
 * (`src/lib/intl/messages/en/screenshots/capture-report.json`: surface → its
 * recorded keys + screenshot filename) and writes `@key.screenshot` into the
 * right `messages/en/*.json` for every key that rendered on a captured surface.
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
 */

import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const here = dirname(fileURLToPath(import.meta.url))
const desktopDir = join(here, '..')
const messagesDir = join(desktopDir, 'src', 'lib', 'intl', 'messages', 'en')
const reportPath = join(messagesDir, 'screenshots', 'capture-report.json')

const checkOnly = process.argv.includes('--check')

if (!existsSync(reportPath)) {
  console.error(`No capture report at ${reportPath}.\nRun \`pnpm i18n:capture\` first to produce it.`)
  process.exit(1)
}

/** @type {Record<string, { screenshot: string; keys: string[] }>} */
const report = JSON.parse(readFileSync(reportPath, 'utf8'))

// Build key → screenshot, first-surface-wins (report insertion order).
/** @type {Map<string, string>} */
const keyToScreenshot = new Map()
for (const { screenshot, keys } of Object.values(report)) {
  for (const key of keys) {
    if (!keyToScreenshot.has(key)) keyToScreenshot.set(key, screenshot)
  }
}

/**
 * The catalog filename a key belongs to: first dot-segment + `.json`.
 * @param {string} key
 * @returns {string}
 */
function fileForKey(key) {
  const area = key.split('.')[0]
  return `${area}.json`
}

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

for (const [file, keyMap] of byFile) {
  const filePath = join(messagesDir, file)
  if (!existsSync(filePath)) {
    console.warn(`Skipping ${file}: no such catalog (key area without a catalog file?)`)
    continue
  }
  const raw = readFileSync(filePath, 'utf8')
  /** @type {Record<string, unknown>} */
  const json = JSON.parse(raw)
  let fileChanged = false

  for (const [key, screenshot] of keyMap) {
    if (!(key in json)) {
      console.warn(`Skipping ${key}: not present in ${file} (catalog may have drifted from the report)`)
      continue
    }
    const metaKey = `@${key}`
    const existing = json[metaKey]
    const metaIsObject = typeof existing === 'object' && existing !== null && !Array.isArray(existing)
    // No (or malformed) metadata twin: create a minimal one. A real description
    // should be authored separately; we only own `screenshot`.
    /** @type {Record<string, unknown>} */
    const meta = metaIsObject ? /** @type {Record<string, unknown>} */ (existing) : {}
    if (meta.screenshot === screenshot) continue // already coupled — idempotent
    couplingCount++
    if (checkOnly) {
      staleForCheck.push(`${key} → ${screenshot} (currently ${JSON.stringify(meta.screenshot)})`)
      continue
    }
    meta.screenshot = screenshot
    // Insert the metadata twin directly AFTER its message key, rebuilding the
    // object so a freshly-created twin lands in the conventional position.
    json[metaKey] = meta
    fileChanged = true
  }

  if (fileChanged && !checkOnly) {
    writeFileSync(filePath, reorderTwins(json))
    changedFiles.push(filePath)
  }
}

/**
 * Serializes the catalog with each `@key` metadata twin placed immediately after
 * its message key (the repo convention), 2-space indent, trailing newline.
 * @param {Record<string, unknown>} json
 * @returns {string}
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
  return JSON.stringify(ordered, null, 2) + '\n'
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

// Normalize formatting on the changed catalogs so the result is oxfmt-clean.
if (changedFiles.length > 0) {
  const res = spawnSync('pnpm', ['exec', 'oxfmt', ...changedFiles], { cwd: desktopDir, stdio: 'inherit' })
  if (res.status !== 0) {
    console.warn('oxfmt did not exit cleanly; run `pnpm exec oxfmt` on the changed files manually.')
  }
}
