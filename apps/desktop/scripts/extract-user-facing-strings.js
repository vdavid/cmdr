#!/usr/bin/env node
/**
 * One-shot ANALYSIS script (not a guarded check, not a catalog generator): scans
 * the frontend for user-facing string literals in the same CLOSED sink set the
 * `no-raw-user-facing-string` lint enforces, to size the i18n migration and
 * surface every multi-variable / rich-text case that needs ICU or `<Trans>`.
 *
 * Run: `node scripts/extract-user-facing-strings.js` (from `apps/desktop/`).
 * Writes a Markdown report to `docs/notes/i18n-extraction-dryrun.md` and prints
 * a summary. Re-run any time to refresh the migration planning numbers; the output is a
 * working artifact, NOT a shipped catalog.
 *
 * HONESTY: this is a regex heuristic over a closed sink set, so it UNDERCOUNTS.
 * It necessarily misses (logged at the end of the report):
 *  - dynamically built / concatenated strings (`'Copied ' + n + ' files'`),
 *  - template literals with expressions (`` `Moved ${n}` ``),
 *  - copy in unrecognized sinks (imperatively-set titles, composed strings
 *    returned from helpers, native menu labels built in Rust),
 *  - strings already migrated to `t()` / `<Trans>` (correctly NOT counted).
 * Treat the total as a LOWER BOUND on the real string count.
 */

import { readFileSync, readdirSync, writeFileSync, statSync } from 'node:fs'
import { dirname, join, relative, extname } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const desktopDir = join(here, '..')
const srcDir = join(desktopDir, 'src')
const repoRoot = join(desktopDir, '..', '..')
const outFile = join(repoRoot, 'docs', 'notes', 'i18n-extraction-dryrun.md')

const SCANNED_EXTS = new Set(['.ts', '.svelte'])
const SKIP_DIRS = new Set(['node_modules', 'target', '.svelte-kit'])

// The closed sink set, matching the `no-raw-user-facing-string` lint:
//  - `addToast('literal'`            → toast content
//  - `title=` / `aria-label=` / `label=` / `placeholder=` "literal"  → props
//  - `>Plain text<`                  → markup text nodes (approximate)
const ADD_TOAST_RE = /\baddToast\(\s*(['"])((?:[^'"\\]|\\.)*)\1/g
const ATTR_RE = /\b(title|aria-label|label|placeholder)=(["'])((?:[^"'\\]|\\.)*)\2/g
// Text between tags: a `>` then non-tag, non-brace text with a letter, then `<`.
const TEXT_NODE_RE = />\s*([A-Za-z][^<>{}]*?)\s*</g

// Things that look like copy but aren't (filter the worst false positives so the
// number is meaningful). Conservative: when unsure, KEEP it (over-count beats
// hiding a real string in a sizing exercise).
const NON_COPY_RE = /^(?:[a-z][a-zA-Z0-9]*(\.[a-z][a-zA-Z0-9]*)+|[a-z-]+|https?:|#|\/|true|false|null|undefined)$/

/**
 * @typedef {{ value: string, file: string, line: number, sink: string }} Candidate
 * @typedef {Map<string, Candidate[]>} ByArea
 */

/**
 * Records a candidate user-facing string with where it was found.
 * @param {ByArea} map
 * @param {string} area
 * @param {string} value
 * @param {string} file
 * @param {number} line
 * @param {string} sink
 */
function record(map, area, value, file, line, sink) {
  if (!map.has(area)) map.set(area, [])
  map.get(area)?.push({ value, file, line, sink })
}

/**
 * The feature area a `src/...` path belongs to, for grouping.
 * @param {string} relPath
 * @returns {string}
 */
function areaOf(relPath) {
  const parts = relPath.split('/')
  // src/lib/<area>/...  or  src/routes/<area>/...
  if (parts[1] === 'lib') return parts[2] ?? 'lib'
  if (parts[1] === 'routes') return `routes/${parts[2] ?? ''}`
  return parts[1] ?? 'other'
}

/**
 * Whether a captured literal is plausibly real copy (a letter, not a key/path).
 * @param {string} value
 * @returns {boolean}
 */
function isCandidate(value) {
  const trimmed = value.trim()
  if (trimmed.length < 2) return false
  if (!/[A-Za-z]/.test(trimmed)) return false
  if (NON_COPY_RE.test(trimmed)) return false
  return true
}

/**
 * @param {string} path
 * @param {string} relPath
 * @param {ByArea} byArea
 */
function scanFile(path, relPath, byArea) {
  const source = readFileSync(path, 'utf8')
  const lines = source.split('\n')
  const area = areaOf(relPath)
  const isSvelte = extname(path) === '.svelte'

  lines.forEach((line, i) => {
    for (const m of line.matchAll(ADD_TOAST_RE)) {
      if (isCandidate(m[2])) record(byArea, area, m[2], relPath, i + 1, 'addToast')
    }
    for (const m of line.matchAll(ATTR_RE)) {
      if (isCandidate(m[3])) record(byArea, area, m[3], relPath, i + 1, `attr:${m[1]}`)
    }
    if (isSvelte) {
      for (const m of line.matchAll(TEXT_NODE_RE)) {
        if (isCandidate(m[1])) record(byArea, area, m[1], relPath, i + 1, 'text')
      }
    }
  })
}

/**
 * @param {string} dir
 * @param {ByArea} byArea
 */
function walk(dir, byArea) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) {
      if (!SKIP_DIRS.has(entry)) walk(full, byArea)
      continue
    }
    if (!SCANNED_EXTS.has(extname(entry))) continue
    if (entry.endsWith('.test.ts') || entry === 'keys.gen.ts') continue
    scanFile(full, relative(srcDir, full), byArea)
  }
}

/** @type {ByArea} */
const byArea = new Map()
walk(srcDir, byArea)

// Build the report. Materialize each area's list once (sorted by area) so the
// rest works over concrete `Candidate[]`, never `| undefined`.
/** @type {[string, Candidate[]][]} */
const entries = [...byArea.entries()].sort((a, b) => a[0].localeCompare(b[0]))
let total = 0
for (const [, list] of entries) total += list.length

let md = `# i18n extraction dry-run (analysis artifact)\n\n`
md += `> One-shot heuristic scan, NOT a shipped catalog. Regenerate with \`node apps/desktop/scripts/extract-user-facing-strings.js\`.\n\n`
md += `Candidate user-facing string literals found in the closed sink set (\`addToast\` content, \`title\`/\`aria-label\`/\`label\`/\`placeholder\` props, \`.svelte\` text nodes). This is a LOWER BOUND on the real string count (see "What this misses").\n\n`
md += `## Total: ${String(total)} candidate strings across ${String(entries.length)} areas\n\n`
md += `Candidates per area (a 2-column table would trip \`docs-table-hygiene\`, so this is a list):\n\n`
for (const [area, list] of entries) md += `- \`${area}\`: ${String(list.length)}\n`

md += `\n## What this heuristic MISSES (so the total isn't over-claimed)\n\n`
md += `- **Dynamic / concatenated strings** (\`'Copied ' + n + ' files'\`) and **template literals with expressions** (\`\\\`Moved \${n}\\\`\`): not captured. These are exactly the multi-variable cases that need ICU \`plural\`/\`select\` — they must be found by reading each area during its migration, not by this scan.\n`
md += `- **Imperatively-set copy**: \`element.title = ...\`, \`setAttribute('aria-label', ...)\`, document/window \`<title>\`s.\n`
md += `- **Composed strings returned from helpers** (the transfer toast was one): the literal is born in a function, far from its \`addToast\` display site.\n`
md += `- **Native menu labels** built in Rust (\`muda\`): not frontend literals at all (deferred surface, Open decision 5).\n`
md += `- **Already-migrated copy** (\`t()\` / \`<Trans>\`): correctly NOT counted.\n`

md += `\n## Multi-variable / rich-text candidates (need ICU or \`<Trans>\`)\n\n`
md += `Heuristic flag: a captured literal containing \`{\` (interpolation), a digit (likely a count), or a \`<tag>\` (inline component). Verify by hand per tranche.\n\n`
/** @type {(Candidate & { area: string })[]} */
const richCases = []
for (const [area, list] of entries) {
  for (const c of list) {
    if (/[{<]/.test(c.value) || /\d/.test(c.value)) richCases.push({ area, ...c })
  }
}
md += `Count: ${String(richCases.length)}\n\n`
for (const c of richCases.slice(0, 80)) {
  md += `- \`${c.area}\` ${c.file}:${String(c.line)} (${c.sink}): ${c.value.replace(/\|/g, '\\|')}\n`
}
if (richCases.length > 80) md += `- … and ${String(richCases.length - 80)} more\n`

md += `\n## Full candidate list by area\n\n`
for (const [area, list] of entries) {
  md += `### \`${area}\` (${String(list.length)})\n\n`
  for (const c of list) {
    md += `- ${c.file}:${String(c.line)} (${c.sink}): ${c.value.replace(/\|/g, '\\|')}\n`
  }
  md += `\n`
}

writeFileSync(outFile, md)
console.log(`Extraction dry-run: ${String(total)} candidate strings across ${String(entries.length)} areas.`)
console.log(`  Multi-variable/rich-text candidates: ${String(richCases.length)}.`)
console.log(`  Report written to ${relative(repoRoot, outFile)}`)
console.log(`  (Lower bound: dynamic/concatenated strings are NOT captured — see the report.)`)
for (const [area, list] of entries) {
  console.log(`    ${area.padEnd(28)} ${String(list.length).padStart(4)}`)
}
