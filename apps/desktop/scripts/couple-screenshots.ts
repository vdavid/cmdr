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
 * second run with the same report produces no diff. It owns `screenshot` and
 * `screenshotNote` outright: a twin carrying either one for a key this run
 * doesn't couple (its surface is gone, or no representative rule reaches it
 * anymore) loses both.
 *
 * Run via `pnpm i18n:couple` (after `pnpm i18n:capture`), or directly with
 * `node scripts/couple-screenshots.ts`. Pass `--check` to write nothing and
 * report instead, exiting by the worst finding: 0 when clean,
 * `CHECK_EXIT_WARN` for stale couplings or a representative rule every key of
 * which has its own capture, `CHECK_EXIT_ERROR` for a structural break (a rule
 * that reaches no catalog key, or a rule or catalog screenshot naming an image
 * the report doesn't have). `desktop-message-screenshots-fresh` maps those
 * codes; any other non-zero code means the script itself failed.
 *
 * The `@key` metadata (including `screenshot` and `screenshotNote`) is stripped
 * by the runtime and by `gen-message-keys.ts`, so this never changes rendered
 * output or the key union.
 *
 * Two kinds of coupling, in two passes:
 *  - DIRECT: a key that rendered on a captured surface gets that surface's
 *    screenshot (`@key.screenshot`), no note. The precise capture.
 *  - REPRESENTATIVE: for a key STILL uncoupled after the direct pass that matches
 *    a curated `REPRESENTATIVE_SCREENSHOTS` prefix, it gets a STAND-IN screenshot
 *    (a real capture of the same panel/toast/dialog where the string appears)
 *    plus a `@key.screenshotNote` explaining the mapping. This raises coverage AND
 *    shrinks the number of distinct images a translator must load. Direct always
 *    wins; a representative never overwrites a precise screenshot, and a key that
 *    later gains its own capture sheds its representative note.
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
 * the real catalogs (see `couple-screenshots.test.ts`). The CLI shell below is
 * only file I/O around those cores, and runs only when invoked as a script.
 */

import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { type RepresentativeMapping, REPRESENTATIVE_SCREENSHOTS } from './representative-screenshots.ts'
import { isNativeKey } from './gen-native-strings-lib.ts'

export { type RepresentativeMapping, REPRESENTATIVE_SCREENSHOTS }

/**
 * The catalog filename a key belongs to: first dot-segment + `.json`.
 */
export function fileForKey(key: string): string {
  const area = key.split('.')[0]
  return `${area}.json`
}

/** One captured surface in the capture report: its screenshot and the keys that rendered on it. */
export interface CaptureSurface {
  screenshot: string
  keys: string[]
  /**
   * The UI zoom the driver had to photograph this surface at, present only when
   * it isn't the default 100%. Written by the capture driver for a surface too
   * tall to fit the display at full window height; surfaced in the coverage
   * report so a translator doesn't read that image as "this is the real size".
   */
  uiZoom?: number
}

/** The capture report: surface name → its screenshot + recorded keys. */
export type CaptureReport = Record<string, CaptureSurface>

/**
 * Flattens a capture report (surface → { screenshot, keys }) into a key →
 * screenshot map, first-surface-wins in the report's insertion order.
 */
export function couplingsFromReport(report: CaptureReport): Map<string, string> {
  const keyToScreenshot = new Map<string, string>()
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
 * individually: the rendered surface records the RESOLVED key only if capture is
 * active at resolution time. Uncoupled keys under one of these are bucketed as
 * "dynamic-only" in the coverage report rather than as a missed surface. Mirrors
 * `unusedKeyDynamicPrefixes` in `scripts/check/checks/desktop-message-keys-unused.go`
 * (kept in sync by hand; both are small, closed, and tied to live construction
 * sites in `apps/desktop/src/lib/`).
 */
export const DYNAMIC_KEY_PREFIXES: string[] = ['errors.git.', 'errors.listing.', 'errors.provider.', 'errors.write.']

/**
 * Returns the first representative mapping whose prefix matches `key`, or null.
 * Order matters: list more specific prefixes before broader ones.
 */
export function representativeFor(
  key: string,
  mappings: RepresentativeMapping[] = REPRESENTATIVE_SCREENSHOTS,
): RepresentativeMapping | null {
  for (const m of mappings) {
    if (key.startsWith(m.prefix)) return m
  }
  return null
}

export interface Couplings {
  /** Every key → its coupling (direct OR representative). */
  byKey: Map<string, Coupling>
  /** Keys coupled to their OWN captured screenshot. */
  directKeys: Set<string>
  /** Keys coupled to a representative stand-in. */
  representativeKeys: Set<string>
}

/**
 * Pure: merges DIRECT capture couplings with REPRESENTATIVE stand-ins. Direct
 * couplings always win (a precise screenshot is never overwritten by a stand-in).
 * A representative coupling is added only for a key that is (a) still uncoupled
 * after the direct pass, (b) matches a representative `prefix`, and (c) whose
 * representative screenshot is one the capture run actually produced
 * (`capturedScreenshots`), never pointing a key at an image that doesn't exist.
 * @param directKeyToScreenshot key → captured screenshot.
 * @param allKeys every renderable catalog key.
 * @param capturedScreenshots filenames present in the capture report.
 */
export function buildCouplings(
  directKeyToScreenshot: Map<string, string>,
  allKeys: Iterable<string>,
  capturedScreenshots: Set<string>,
  mappings: RepresentativeMapping[] = REPRESENTATIVE_SCREENSHOTS,
): Couplings {
  const byKey = new Map<string, Coupling>()
  const directKeys = new Set<string>()
  const representativeKeys = new Set<string>()

  for (const [key, screenshot] of directKeyToScreenshot) {
    byKey.set(key, { screenshot })
    directKeys.add(key)
  }

  for (const key of allKeys) {
    if (directKeys.has(key)) continue // direct wins
    const rep = representativeFor(key, mappings)
    if (!rep) continue
    if (!capturedScreenshots.has(rep.screenshot)) continue // its image wasn't captured
    byKey.set(key, { screenshot: rep.screenshot, note: rep.note })
    representativeKeys.add(key)
  }

  return { byKey, directKeys, representativeKeys }
}

export interface AreaCoverage {
  /** The catalog area (filename minus `.json`). */
  area: string
  /** Renderable keys in the area. */
  total: number
  /** Keys coupled to their OWN captured screenshot. */
  direct: number
  /** Keys coupled to a representative (stand-in) screenshot. */
  representative: number
  /** Keys with no screenshot at all. */
  uncoupled: number
  /**
   * Keys drawn by the OS, not the webview (the native menu bar, the window
   * title, the already-running alert). Counted apart from `uncoupled` because
   * the capture harness drives a webview and structurally cannot reach them: a
   * screenshot here would have to be faked, and a fake is worse for a translator
   * than an honest gap. See `isNativeKey`.
   */
  nativeOnly: number
}

export interface CoverageReport {
  /** Per-area coverage rows, sorted by area name. */
  areas: AreaCoverage[]
  /** Renderable keys across all areas. */
  total: number
  /** Directly-captured keys across all areas. */
  direct: number
  /** Representative-coupled keys across all areas. */
  representative: number
  /** Uncoupled keys across all areas. */
  uncoupled: number
  /** Native-surface keys across all areas (see `AreaCoverage.nativeOnly`). */
  nativeOnly: number
}

/**
 * Pure coverage core: given every renderable catalog key (by area), the keys
 * coupled to their OWN captured screenshot (`directKeys`), and the keys coupled
 * to a representative stand-in (`representativeKeys`), tallies per area how many
 * are direct vs representative vs uncoupled. A representative coupling is counted
 * separately from a direct one so the report never implies a stand-in image is a
 * precise capture. No filesystem access.
 * @param directKeys keys with their own captured screenshot.
 * @param representativeKeys keys coupled to a representative screenshot.
 * @param keysByArea area → its renderable keys.
 */
export function buildCoverageReport(
  directKeys: Set<string>,
  representativeKeys: Set<string>,
  keysByArea: Map<string, string[]>,
): CoverageReport {
  const areas: AreaCoverage[] = []
  let total = 0
  let direct = 0
  let representative = 0
  let uncoupled = 0
  let nativeOnly = 0

  for (const area of [...keysByArea.keys()].sort()) {
    const keys = keysByArea.get(area) ?? []
    let areaDirect = 0
    let areaRep = 0
    let areaUncoupled = 0
    let areaNative = 0
    for (const key of keys) {
      if (directKeys.has(key)) areaDirect++
      else if (representativeKeys.has(key)) areaRep++
      // Checked AFTER the two coupling passes, so a native key that somehow did
      // render in the webview keeps its real screenshot rather than being
      // written off as unreachable.
      else if (isNativeKey(key)) areaNative++
      else areaUncoupled++
    }
    areas.push({
      area,
      total: keys.length,
      direct: areaDirect,
      representative: areaRep,
      uncoupled: areaUncoupled,
      nativeOnly: areaNative,
    })
    total += keys.length
    direct += areaDirect
    representative += areaRep
    uncoupled += areaUncoupled
    nativeOnly += areaNative
  }

  return { areas, total, direct, representative, uncoupled, nativeOnly }
}

/**
 * Renders a CoverageReport as Markdown for the tracked artifact. Kept text + small
 * so its diff stays readable. Pure (no filesystem, no Date: the caller stamps any
 * timestamp), so it's snapshot-testable.
 */
export function renderCoverageReport(report: CoverageReport): string {
  const pct = (n: number, d: number): string => (d === 0 ? 'n/a' : `${String(Math.round((n / d) * 100))}%`)
  const anyCoverage = report.direct + report.representative
  const lines = [
    '# Screenshot coverage',
    '',
    'Generated by `scripts/couple-screenshots.ts` (via `pnpm i18n:shots`). Tracked, regenerable.',
    '',
    'Per catalog area, each renderable key is one of three:',
    '',
    '- **Direct**: coupled to a screenshot that actually shows THIS string in context (a real capture of its own surface).',
    '- **Representative**: coupled to a stand-in screenshot of the same panel/toast/dialog where the string appears, plus a',
    '  `@key.screenshotNote` explaining the mapping. Honest-by-design: it is NOT a precise capture, but it shows the right',
    '  layout and position so a translator loads one image for a whole family of strings.',
    '- **Uncoupled**: no screenshot yet (a surface the capture driver does not visit, or one with no honest representative).',
    '- **Native**: drawn by the operating system, not the webview (the menu bar, the window title, the already-running',
    '  alert). The capture harness drives a webview, so it can never reach these; the `@key` description is the whole',
    '  translator aid there, which is why those descriptions carry the menu, the verb-or-noun call, and the Finder',
    '  counterpart. Counted apart from Uncoupled so a permanent structural gap never reads as a missing capture.',
    '',
    `Coverage is PARTIAL by design. Uncoupled keys are expected, not bugs.`,
    '',
    `**Total: ${String(anyCoverage)} / ${String(report.total)} keys have a screenshot (${pct(anyCoverage, report.total)}):** ` +
      `${String(report.direct)} direct (${pct(report.direct, report.total)}) and ` +
      `${String(report.representative)} representative (${pct(report.representative, report.total)}). ` +
      `${String(report.uncoupled)} remain uncoupled, and ${String(report.nativeOnly)} are native surfaces a webview capture cannot reach.`,
    '',
    '| Area | Direct | Representative | Uncoupled | Native | Total | Any % |',
    '| --- | ---: | ---: | ---: | ---: | ---: | ---: |',
  ]
  for (const a of report.areas) {
    lines.push(
      `| ${a.area} | ${String(a.direct)} | ${String(a.representative)} | ${String(a.uncoupled)} | ${String(a.nativeOnly)} | ${String(a.total)} | ${pct(a.direct + a.representative, a.total)} |`,
    )
  }
  lines.push('')
  return lines.join('\n')
}

/** A captured surface that resolved no key some other captured surface didn't. */
export interface RedundantSurface {
  surface: string
  screenshot: string
  /** How many keys it recorded (all of them shared with other surfaces). */
  keys: number
}

/** A surface the driver had to shrink the UI for, because it wouldn't fit the display. */
export interface ReducedZoomSurface {
  surface: string
  screenshot: string
  uiZoom: number
}

/** The per-SURFACE review notes that ride along with the per-KEY coverage table. */
export interface SurfaceReview {
  /** Captured surfaces in the run. */
  surfaces: number
  redundant: RedundantSurface[]
  reducedZoom: ReducedZoomSurface[]
}

/**
 * Pure: reviews the capture report itself, rather than the catalog.
 *
 * Two things a human wants to know after a run and can't see from the coverage
 * table. First, which surfaces contributed NO unique key: the surface set grows
 * every time a dialog does, and nothing shrinks it, so without this the pruning
 * only happens when someone thinks to look. Second, which surfaces the driver
 * had to shrink the UI to photograph, since those images show text smaller than a
 * user sees it.
 */
export function buildSurfaceReview(report: CaptureReport): SurfaceReview {
  const surfaceCount = new Map<string, number>()
  for (const { keys } of Object.values(report)) {
    for (const key of new Set(keys)) surfaceCount.set(key, (surfaceCount.get(key) ?? 0) + 1)
  }
  const redundant: RedundantSurface[] = []
  const reducedZoom: ReducedZoomSurface[] = []
  for (const [surface, entry] of Object.entries(report)) {
    if (entry.keys.length > 0 && entry.keys.every((key) => (surfaceCount.get(key) ?? 0) > 1)) {
      redundant.push({ surface, screenshot: entry.screenshot, keys: entry.keys.length })
    }
    if (entry.uiZoom !== undefined && entry.uiZoom !== 100) {
      reducedZoom.push({ surface, screenshot: entry.screenshot, uiZoom: entry.uiZoom })
    }
  }
  return { surfaces: Object.keys(report).length, redundant, reducedZoom }
}

/**
 * Renders the surface review as the tail of the coverage report. Deliberately
 * worded as something to consider, not a verdict: a surface that adds no unique
 * key can still be the clearest CONTEXT for a key that several surfaces share.
 *
 * DRAFT (David reviews human-facing copy).
 */
export function renderSurfaceReview(review: SurfaceReview): string {
  const lines = ['## Surfaces to review', '']
  lines.push(
    `The run captured ${String(review.surfaces)} surfaces. This section is regenerated every run, so it stays ` +
      'true as the UI changes.',
  )
  lines.push('')
  lines.push(`### No unique keys (${String(review.redundant.length)})`)
  lines.push('')
  if (review.redundant.length === 0) {
    lines.push('Every captured surface is the only source of at least one key.')
  } else {
    lines.push(
      'Every key on these surfaces also renders on another captured surface, so dropping one costs no coverage: ' +
        'its keys would simply couple to whichever surface keeps them. Worth considering, NOT an automatic delete. ' +
        'A surface can be the clearest picture of a key several surfaces share, and being the clearest is reason ' +
        'enough to keep it. To drop one, remove its staging (or add it to `DROPPED_GALLERY_STATES` for a gallery ' +
        'state) in `test/e2e-playwright/`.',
    )
    lines.push('')
    for (const { surface, keys } of [...review.redundant].sort((a, b) => b.keys - a.keys)) {
      lines.push(`- \`${surface}\` (${String(keys)} key${keys === 1 ? '' : 's'}, none unique)`)
    }
  }
  lines.push('')
  lines.push(`### Captured at a reduced UI zoom (${String(review.reducedZoom.length)})`)
  lines.push('')
  if (review.reducedZoom.length === 0) {
    lines.push('Every surface fit the display at 100% zoom, so every image shows text at its real size.')
  } else {
    lines.push(
      '❗ These surfaces are taller than the display allows even with the window grown to full height, so the ' +
        'driver reduced the UI zoom to fit the whole surface in frame. **The text in these images is smaller ' +
        'than what a user sees.** Judge length against the other screenshots, not these.',
    )
    lines.push('')
    for (const { surface, uiZoom } of [...review.reducedZoom].sort((a, b) => a.surface.localeCompare(b.surface))) {
      lines.push(`- \`${surface}\`: captured at ${String(uiZoom)}% zoom`)
    }
  }
  lines.push('')
  return lines.join('\n')
}

/**
 * Something wrong with the coupling inputs themselves, found without a capture
 * run: every input is tracked (the report, the catalogs, the rules), so a key or
 * surface rename breaks one in the same commit that caused it.
 */
export type StructuralFinding =
  /** A representative rule that is the first match for no catalog key (renamed away, or shadowed by earlier rules). */
  | { kind: 'deadRule'; prefix: string }
  /** A representative rule whose stand-in image the report doesn't have, so it silently couples nothing. */
  | { kind: 'missingRuleTarget'; prefix: string; screenshot: string }
  /** A catalog `@key.screenshot` naming an image the report doesn't have. */
  | { kind: 'unknownScreenshot'; key: string; screenshot: string }
  /** A representative rule every key of which has its own capture, so it stands in for nothing. */
  | { kind: 'redundantRule'; prefix: string; keys: number }

/** `--check` exit code when the worst finding only warns (stale couplings, a redundant rule). */
export const CHECK_EXIT_WARN = 20
/** `--check` exit code for a structural break. Both codes sit clear of Node's own, so a crash can't pass for a finding. */
export const CHECK_EXIT_ERROR = 21

/**
 * Pure: the structural findings for one set of inputs.
 * @param allKeys every catalog key.
 * @param screenshotByKey each key's current `@key.screenshot`, as written in the catalogs.
 */
export function findStructuralProblems(
  report: CaptureReport,
  allKeys: Iterable<string>,
  screenshotByKey: Map<string, string>,
  mappings: RepresentativeMapping[] = REPRESENTATIVE_SCREENSHOTS,
): StructuralFinding[] {
  const captured = new Set(Object.values(report).map((surface) => surface.screenshot))
  const direct = couplingsFromReport(report)
  // A rule "reaches" the keys it's the FIRST match for, the same way
  // `representativeFor` picks, so a broad rule whose keys all sit under earlier
  // specific ones counts as dead.
  const reached = new Map<RepresentativeMapping, string[]>(mappings.map((rule) => [rule, []]))
  for (const key of allKeys) {
    const rule = representativeFor(key, mappings)
    if (rule) reached.get(rule)?.push(key)
  }

  const findings: StructuralFinding[] = []
  for (const rule of mappings) {
    const keys = reached.get(rule) ?? []
    if (keys.length === 0) findings.push({ kind: 'deadRule', prefix: rule.prefix })
    if (!captured.has(rule.screenshot)) {
      findings.push({ kind: 'missingRuleTarget', prefix: rule.prefix, screenshot: rule.screenshot })
    }
    if (keys.length > 0 && keys.every((key) => direct.has(key))) {
      findings.push({ kind: 'redundantRule', prefix: rule.prefix, keys: keys.length })
    }
  }
  for (const [key, screenshot] of screenshotByKey) {
    if (!captured.has(screenshot)) findings.push({ kind: 'unknownScreenshot', key, screenshot })
  }
  return findings
}

/** Whether a finding fails the check. A redundant rule only costs a translator nothing, so it warns. */
export function isBreakingFinding(finding: StructuralFinding): boolean {
  return finding.kind !== 'redundantRule'
}

/** The `--check` exit code: the worst of the structural findings and the stale couplings. */
export function checkExitCode(findings: StructuralFinding[], staleCount: number): number {
  if (findings.some((finding) => isBreakingFinding(finding))) return CHECK_EXIT_ERROR
  if (findings.length > 0 || staleCount > 0) return CHECK_EXIT_WARN
  return 0
}

export interface Coupling {
  /** The screenshot filename to write to `@key.screenshot`. */
  screenshot: string
  /**
   * An optional translator note (`@key.screenshotNote`). Present for
   * REPRESENTATIVE couplings (a stand-in image), absent for DIRECT (captured)
   * ones. When absent, any existing `screenshotNote` is REMOVED, so a key that
   * gains a direct capture sheds its old representative note.
   */
  note?: string
}

/** One missing/stale coupling for `--check`: the write that WOULD happen. */
export interface StaleCoupling {
  key: string
  /** The screenshot the run would write, or undefined when it clears the key's coupling. */
  screenshot: string | undefined
  current: string | undefined
}

export interface CoupleResult {
  /** The catalog TEXT after coupling (line-surgical; all other bytes byte-identical). */
  text: string
  /** Whether any `@key.screenshot`/`@key.screenshotNote` was added, updated, or removed. */
  changed: boolean
  /** How many keys were (re)coupled. */
  couplingCount: number
  /** `key → screenshot` for keys whose twin has no description. */
  coupledWithoutDescription: string[]
  /** Keys requested but absent from this catalog. */
  missingKeys: string[]
  /** Keys present but with no `@key` twin to host the screenshot (skipped). */
  missingTwins: string[]
  /** For `--check`: keys whose coupling is missing/stale (the writes that WOULD happen). */
  stale: StaleCoupling[]
  /** How many twins lost a coupling this run no longer produces. */
  clearedCount: number
}

/**
 * Pure core: returns the catalog TEXT with `@key.screenshot` (and, for
 * representative couplings, `@key.screenshotNote`) set for every requested key,
 * edited LINE-SURGICALLY so every other byte (message values, other twin
 * fields, indentation, AND the blank lines that group the catalog) is preserved
 * exactly. (A `JSON.parse` → `JSON.stringify` round-trip would drop the
 * blank-line grouping; oxfmt doesn't restore it, so it would reflatten every
 * file on every run. The spec's gotcha: preserve oxfmt'd formatting, touch ONLY
 * the `screenshot`/`screenshotNote` fields.) Does not read or write the
 * filesystem.
 *
 * We parse the JSON once (read-only) to learn which keys exist, their current
 * screenshot/note (for idempotency), and whether the twin has a description; the
 * actual mutation is on the raw text.
 * @param rawText The catalog file contents (`messages/en/<area>.json`).
 * @param keyToCoupling key → coupling for THIS catalog.
 */
export function coupleCatalog(rawText: string, keyToCoupling: Map<string, Coupling>): CoupleResult {
  const json = JSON.parse(rawText) as Record<string, unknown>
  let text = rawText
  let couplingCount = 0
  const coupledWithoutDescription: string[] = []
  const missingKeys: string[] = []
  const missingTwins: string[] = []
  const stale: StaleCoupling[] = []

  for (const [key, coupling] of keyToCoupling) {
    const { screenshot, note } = coupling
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
    const meta = existing as Record<string, unknown>
    const currentNote = typeof meta.screenshotNote === 'string' ? meta.screenshotNote : undefined
    // Idempotent: skip when both fields already match the desired state (note
    // absent means "no screenshotNote field").
    if (meta.screenshot === screenshot && currentNote === note) continue

    let next = setTwinField(text, metaKey, 'screenshot', screenshot)
    if (next === null) {
      // Shouldn't happen (the twin parsed as an object), but never corrupt a file.
      missingTwins.push(key)
      continue
    }
    // The note is set for representative couplings and REMOVED for direct ones,
    // so a key never carries a stale stand-in note once it has its own capture.
    const afterNote = setTwinField(next, metaKey, 'screenshotNote', note ?? null)
    if (afterNote === null) {
      missingTwins.push(key)
      continue
    }
    next = afterNote
    text = next
    const hasDescription = typeof meta.description === 'string' && meta.description.trim() !== ''
    if (!hasDescription) coupledWithoutDescription.push(`${key} → ${screenshot}`)
    couplingCount++
    stale.push({ key, screenshot, current: typeof meta.screenshot === 'string' ? meta.screenshot : undefined })
  }

  const cleared = clearUncoupledTwins(text, json, keyToCoupling)
  text = cleared.text
  stale.push(...cleared.stale)
  missingTwins.push(...cleared.unlocatable)

  // Every edit above is skipped when a field already holds its target, so any
  // byte of difference is a real change.
  const changed = text !== rawText
  const clearedCount = cleared.stale.length
  return { text, changed, couplingCount, coupledWithoutDescription, missingKeys, missingTwins, stale, clearedCount }
}

/**
 * Removes `screenshot` and `screenshotNote` from every twin whose key isn't in
 * `keyToCoupling`. The map is the whole truth for its catalog, so a coupling it
 * doesn't name is one no surface or rule produces anymore, and keeping it would
 * point a translator at the wrong image.
 * @param json The catalog as parsed BEFORE this run's edits (the twins' current fields).
 */
function clearUncoupledTwins(
  rawText: string,
  json: Record<string, unknown>,
  keyToCoupling: Map<string, Coupling>,
): { text: string; stale: StaleCoupling[]; unlocatable: string[] } {
  let text = rawText
  const stale: StaleCoupling[] = []
  const unlocatable: string[] = []
  for (const [metaKey, existing] of Object.entries(json)) {
    const key = metaKey.slice(1)
    if (!metaKey.startsWith('@') || keyToCoupling.has(key)) continue
    const meta = typeof existing === 'object' && existing !== null ? (existing as Record<string, unknown>) : {}
    const current = typeof meta.screenshot === 'string' ? meta.screenshot : undefined
    if (current === undefined && typeof meta.screenshotNote !== 'string') continue
    const withoutScreenshot = setTwinField(text, metaKey, 'screenshot', null)
    const withoutBoth =
      withoutScreenshot === null ? null : setTwinField(withoutScreenshot, metaKey, 'screenshotNote', null)
    if (withoutBoth === null) {
      unlocatable.push(key)
      continue
    }
    text = withoutBoth
    stale.push({ key, screenshot: undefined, current })
  }
  return { text, stale, unlocatable }
}

/**
 * Sets, replaces, or REMOVES one top-level string `field` of a single
 * `"@key": { … }` object in the catalog text, touching only that object. With a
 * non-null `value`, replaces an existing `"field": "…"` line if present, else
 * inserts the field as the LAST property (appending a comma to the previous last
 * line). With `value === null`, removes the field's line entirely (and its comma,
 * keeping the object's comma structure valid); a no-op if the field is absent.
 * Returns the new text, or null if the twin block can't be located/parsed
 * (caller then skips, never corrupting the file).
 *
 * Used for both `screenshot` and `screenshotNote` (representative-coupling note).
 * @param metaKey e.g. `@common.ok`
 * @param field e.g. `screenshot` or `screenshotNote`
 * @param value the JSON string value, or null to remove the field
 */
/**
 * Index of the `}` matching the `{` at `braceStart`, skipping braces inside
 * strings (`placeholders` nests, so a naive "first }" is wrong). Returns -1 if
 * the object never closes.
 */
function matchingBraceIndex(text: string, braceStart: number): number {
  let depth = 0
  let inStr = false
  let esc = false
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
      if (depth === 0) return i
    }
  }
  return -1
}

function setTwinField(text: string, metaKey: string, field: string, value: string | null): string | null {
  // The twin is an oxfmt'd object opening on its own line: `  "@key": {`.
  const open = `  ${JSON.stringify(metaKey)}: {`
  const openIdx = text.indexOf(open)
  if (openIdx === -1) return null
  const braceStart = openIdx + open.length - 1 // index of the `{`
  const closeIdx = matchingBraceIndex(text, braceStart)
  if (closeIdx === -1) return null

  const body = text.slice(braceStart + 1, closeIdx) // between the braces
  const before = text.slice(0, braceStart + 1)
  const after = text.slice(closeIdx)

  // Matches the existing top-level `"field": "…"` line (4-space indent, the
  // twin's own field indent) plus a trailing comma if it has one (`$1`).
  const fieldName = field.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const existingRe = new RegExp(`\\n {4}"${fieldName}": "(?:[^"\\\\]|\\\\.)*"(,?)`)
  const m = existingRe.exec(body)

  if (value === null) {
    // Remove the field. If it had a trailing comma, dropping the line + comma
    // keeps the rest valid. If it was the LAST field (no trailing comma), also
    // drop the comma on the now-last preceding field.
    if (!m) return before + body + after // already absent
    if (m[1] === ',') return before + body.replace(existingRe, '') + after
    // Last field: remove it AND the preceding field's trailing comma.
    const withoutField = body.replace(existingRe, '')
    const trimmedComma = withoutField.replace(/,(\n {2})$/, '$1') // last `,` before `\n  }`
    return before + trimmedComma + after
  }

  if (m) {
    const replaced = body.replace(existingRe, `\n    "${field}": ${JSON.stringify(value)}${m[1]}`)
    return before + replaced + after
  }

  // Insert as the last field: append a comma to the current last property line,
  // then add the new field line before the closing brace. `body` ends with the
  // last field then a newline + the close brace's indent; trim that trailing
  // newline/indent, add `,\n    "field": "…"\n  ` back.
  const trimmed = body.replace(/\n {2}$/, '') // drop the "\n  " before `}`
  return before + trimmed + `,\n    "${field}": ${JSON.stringify(value)}\n  ` + after
}

// ── CLI shell (file I/O only; skipped when imported as a module) ──────────────

/**
 * Reads every `en/*.json` catalog. Returns area → its renderable keys (the `@key`
 * metadata twins dropped, matching the runtime + codegen's key set), used to
 * compute coverage over the WHOLE catalog rather than the captured subset; and
 * key → the `@key.screenshot` its twin carries now, for the structural findings.
 * @param messagesDir absolute path to `messages/en`
 */
function readCatalogs(messagesDir: string): {
  keysByArea: Map<string, string[]>
  screenshotByKey: Map<string, string>
} {
  const keysByArea = new Map<string, string[]>()
  const screenshotByKey = new Map<string, string>()
  for (const name of readdirSync(messagesDir)) {
    if (!name.endsWith('.json')) continue
    const json = JSON.parse(readFileSync(join(messagesDir, name), 'utf8')) as Record<string, unknown>
    keysByArea.set(
      name.slice(0, -'.json'.length),
      Object.keys(json).filter((k) => !k.startsWith('@')),
    )
    for (const [metaKey, meta] of Object.entries(json)) {
      if (!metaKey.startsWith('@') || typeof meta !== 'object' || meta === null) continue
      const { screenshot } = meta as { screenshot?: unknown }
      if (typeof screenshot === 'string') screenshotByKey.set(metaKey.slice(1), screenshot)
    }
  }
  return { keysByArea, screenshotByKey }
}

/** Groups key→coupling pairs by their catalog file, so each file is read/written once. */
function groupByFile(byKey: Map<string, Coupling>): Map<string, Map<string, Coupling>> {
  const byFile = new Map<string, Map<string, Coupling>>()
  for (const [key, coupling] of byKey) {
    const file = fileForKey(key)
    let m = byFile.get(file)
    if (m === undefined) {
      m = new Map()
      byFile.set(file, m)
    }
    m.set(key, coupling)
  }
  return byFile
}

interface CoupleAllResult {
  changedFiles: string[]
  couplingCount: number
  clearedCount: number
  staleForCheck: string[]
  coupledWithoutDescription: string[]
  missingTwins: string[]
}

/**
 * Couples every catalog file on disk, including one this run couples nothing in
 * (its stale couplings still need clearing). With `checkOnly`, collects stale
 * couplings instead of writing; otherwise writes the changed files. Surfaces
 * drift (a coupled area with no catalog, missing keys) as warnings.
 */
function coupleAllFiles(
  byFile: Map<string, Map<string, Coupling>>,
  messagesDir: string,
  checkOnly: boolean,
): CoupleAllResult {
  const changedFiles: string[] = []
  let couplingCount = 0
  let clearedCount = 0
  const staleForCheck: string[] = []
  const coupledWithoutDescription: string[] = []
  const missingTwins: string[] = []

  const catalogFiles = readdirSync(messagesDir).filter((name) => name.endsWith('.json'))
  for (const file of byFile.keys()) {
    if (!catalogFiles.includes(file)) {
      console.warn(`Skipping ${file}: no such catalog (key area without a catalog file?)`)
    }
  }

  for (const file of catalogFiles) {
    const filePath = join(messagesDir, file)
    const result = coupleCatalog(readFileSync(filePath, 'utf8'), byFile.get(file) ?? new Map<string, Coupling>())

    for (const key of result.missingKeys) {
      console.warn(`Skipping ${key}: not present in ${file} (catalog may have drifted from the report)`)
    }
    for (const key of result.missingTwins) {
      missingTwins.push(`${key} (in ${file})`)
    }
    couplingCount += result.couplingCount
    clearedCount += result.clearedCount
    coupledWithoutDescription.push(...result.coupledWithoutDescription)
    if (checkOnly) {
      for (const { key, screenshot, current } of result.stale) {
        const target = screenshot === undefined ? 'no screenshot (stale coupling to clear)' : screenshot
        staleForCheck.push(`${key} → ${target} (currently ${JSON.stringify(current)})`)
      }
      continue
    }
    if (result.changed) {
      writeFileSync(filePath, result.text)
      changedFiles.push(filePath)
    }
  }

  return { changedFiles, couplingCount, clearedCount, staleForCheck, coupledWithoutDescription, missingTwins }
}

/** One finding as a line a person can act on. */
function describeFinding(finding: StructuralFinding): string {
  switch (finding.kind) {
    case 'deadRule':
      return `representative rule \`${finding.prefix}\` is the first match for no catalog key (renamed away, or every key it matches sits under an earlier rule); fix or delete it in \`representative-screenshots.ts\``
    case 'missingRuleTarget':
      return `representative rule \`${finding.prefix}\` points at ${finding.screenshot}, which \`capture-report.json\` doesn't have, so it couples nothing; aim it at a captured surface`
    case 'unknownScreenshot':
      return `${finding.key} names ${finding.screenshot}, which \`capture-report.json\` doesn't have; run \`pnpm i18n:couple\``
    case 'redundantRule':
      return `representative rule \`${finding.prefix}\` stands in for nothing: all ${String(finding.keys)} key(s) it reaches have their own capture; consider deleting it`
  }
}

/** Prints one headed, bulleted section to stderr, or nothing when it's empty. */
function printSection(header: string, lines: string[]): void {
  if (lines.length === 0) return
  console.error(`${header} (${String(lines.length)}):`)
  for (const line of lines) console.error(`  - ${line}`)
}

/** Prints the structural findings (breaking first) and the stale couplings. */
function printFindings(findings: StructuralFinding[], stale: string[]): void {
  const describe = (list: StructuralFinding[]): string[] => list.map((finding) => describeFinding(finding))
  printSection(
    'Broken screenshot rules and couplings',
    describe(findings.filter((finding) => isBreakingFinding(finding))),
  )
  printSection('Redundant representative rules', describe(findings.filter((finding) => !isBreakingFinding(finding))))
  printSection('Stale screenshot couplings, which `pnpm i18n:couple` rewrites', stale)
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

  const report = JSON.parse(readFileSync(reportPath, 'utf8')) as CaptureReport
  const directKeyToScreenshot = couplingsFromReport(report)

  // Every renderable catalog key (for the representative pass + coverage).
  const { keysByArea, screenshotByKey } = readCatalogs(messagesDir)
  const allKeys = [...keysByArea.values()].flat()
  // Screenshots the capture run actually produced (a representative may only
  // point at one of these, never at a missing image).
  const capturedScreenshots = new Set(Object.values(report).map((s) => s.screenshot))

  // Direct (captured) couplings, then representative stand-ins for the keys still
  // uncoupled. Direct always wins.
  const { byKey, directKeys, representativeKeys } = buildCouplings(directKeyToScreenshot, allKeys, capturedScreenshots)

  // Group target keys by their catalog file so each file is read/written once.
  const byFile = groupByFile(byKey)
  const { changedFiles, couplingCount, clearedCount, staleForCheck, coupledWithoutDescription, missingTwins } =
    coupleAllFiles(byFile, messagesDir, checkOnly)

  if (checkOnly) {
    // Read against the catalogs as committed: a screenshot the report lacks is a
    // break for as long as a twin still names it.
    const findings = findStructuralProblems(report, allKeys, screenshotByKey)
    printFindings(findings, staleForCheck)
    const exitCode = checkExitCode(findings, staleForCheck.length)
    if (exitCode === 0) {
      console.log('Every screenshot coupling matches the capture report, and every representative rule is live.')
    }
    process.exit(exitCode)
  }

  // This run just rewrote every twin from the report, so only the rules can still
  // be broken; a finding here doesn't stop the write, it tells the person running it.
  printFindings(findStructuralProblems(report, allKeys, new Map()), [])

  console.log(
    `Coupled ${String(couplingCount)} key(s) to screenshots and cleared ${String(clearedCount)} stale coupling(s) ` +
      `across ${String(changedFiles.length)} catalog file(s) ` +
      `(${String(directKeys.size)} direct, ${String(representativeKeys.size)} representative).`,
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

  // Coverage report (Decision 4: no silent gaps): a tracked, text artifact that
  // shows which areas have screenshots (direct vs representative) and which keys
  // remain uncoupled.
  const coverage = buildCoverageReport(directKeys, representativeKeys, keysByArea)
  const coveragePath = join(messagesRoot, 'screenshots', 'coverage-report.md')
  // The per-key table, then the per-SURFACE review (redundant surfaces, reduced
  // zoom). One tracked artifact, so a reviewer reads both in the same diff.
  writeFileSync(coveragePath, renderCoverageReport(coverage) + '\n' + renderSurfaceReview(buildSurfaceReview(report)))
  console.log(
    `Wrote coverage report: ${String(coverage.direct)} direct + ${String(coverage.representative)} representative ` +
      `/ ${String(coverage.total)} keys → ${coveragePath}`,
  )

  // Safety net: confirm every file this run wrote is oxfmt-clean. For the
  // surgical catalog edits it's a no-op in practice (we preserve oxfmt's shape),
  // but the coverage report is rendered from scratch and its reflowed prose and
  // padded tables are oxfmt's call, not ours; without this the next `pnpm check`
  // fails on a file nobody hand-edited.
  const toFormat = [...changedFiles, coveragePath]
  const res = spawnSync('pnpm', ['exec', 'oxfmt', ...toFormat], { cwd: desktopDir, stdio: 'inherit' })
  if (res.status !== 0) {
    console.warn('oxfmt did not exit cleanly; run `pnpm exec oxfmt` on the changed files manually.')
  }
}

// Run the CLI only when executed directly, not when imported by a test.
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main()
}
