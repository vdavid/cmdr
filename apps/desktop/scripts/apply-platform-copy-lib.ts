/**
 * Plans the catalog rewrite for resolved platform-copy rows
 * (`platform-copy-rows.ts`): pure reads in, a map of new file contents and a
 * report out. `apply-platform-copy.ts` is the CLI that verifies and writes it.
 *
 * What a row does to each catalog (`src/lib/intl/DETAILS.md` § "Platform variants"):
 *
 *  - `en`: the key's value becomes the row's folded block (`englishValueOf`). A
 *    pair's two old keys go, and its survivor takes the `.mac` key's place and
 *    `@key` block.
 *  - A full locale: the same block, same branch set. The branch macOS reads
 *    ALWAYS keeps the locale's own text (macOS text never moves). Any other
 *    branch keeps the locale's text only when its English is unchanged AND the
 *    translation was current (`@key.sourceHash` matches the old English);
 *    otherwise it holds the branch's English, pending translation.
 *    `@key.sourceHash` is never touched, so the stale check lists the key.
 *  - An overlay (`en-GB`, `en-AU`, `es-419`): only keys it forks. Same rule per
 *    branch over its own fork text; a branch whose old key it never forked takes
 *    its parent's new branch (what its readers saw). ❌ Never the parent's
 *    translation where the overlay had its own fork: that puts Spain-only forms
 *    back into `es-419`. A fork left equal to its parent is dropped.
 *
 * Applying is idempotent: a row whose English already holds its block is skipped.
 * A row re-resolved later goes through the same per-branch rule against the block
 * it replaces.
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { pickPlatformBranch } from '../src/lib/intl/platform-select.ts'
import {
  BASE_LOCALE,
  GENERATED_LOCALES,
  branchReader,
  formatPlatformBranches,
  isMetadataKey,
  listLocales,
  platformBranches,
  resolveLocaleSource,
  resolveMessagesRoot,
  sourceHash,
} from './i18n-catalog-lib.ts'
import type { PlatformBranch, RenderedPlatform } from './i18n-catalog-lib.ts'
import { ACCEPTED_ENGLISH_CHANGES, COLLAPSED_PAIRS } from './i18n-macos-copy-baseline-lib.ts'
import { resolveDocsRoot } from './i18n-termbase-lib.ts'
import { englishValueIssues, englishValueOf } from './platform-copy-rows.ts'
import type { ApplyRow } from './platform-copy-rows.ts'

type AreaFile = Record<string, unknown>
type Meta = Record<string, unknown>

/** Where the plan reads and writes. Every default is the real repo; tests point at fixtures. */
export interface PlanPaths {
  messagesRoot?: string
  /** `docs/i18n`, whose guides and termbases cite keys */
  docsRoot?: string
  /** the term-consistency allowlist, whose reasons cite keys; `null` to skip it */
  termAllowlistPath?: string | null
}

/** What the plan did to one locale's catalog. */
export interface LocaleReport {
  locale: string
  /** branches holding the locale's own text */
  carried: number
  /** branches holding English, pending translation */
  english: number
  /** keys whose translation was already stale: it went into the macOS-read branch only */
  preStale: string[]
  /** pairs whose `.other` key the locale lacked, so that branch is English */
  missingOther: string[]
  /** an overlay's forks left equal to the parent's new value, and so removed */
  droppedForks: string[]
  /** an English overlay's keys with a branch taken from `en`: spell it, then restamp */
  spellingPass: string[]
}

/** A row the plan couldn't apply, and why. */
export interface RefusedRow {
  key: string
  reason: string
}

/** The whole rewrite, unwritten. */
export interface CopyPlan {
  /** absolute path → new content, for every catalog file that changes */
  catalogFiles: Map<string, string>
  /** absolute path → new content, for the docs, the allowlist, and the capture report (run oxfmt after writing) */
  otherFiles: Map<string, string>
  /** rows written, by survivor key */
  applied: string[]
  /** rows whose English already holds their block */
  alreadyApplied: string[]
  refused: RefusedRow[]
  locales: LocaleReport[]
  /** old key → survivor, for each pair collapsed by this plan */
  renames: Map<string, string[]>
  /** `file: segments` for each guide still naming an old key in shorthand (`.keyringBody`, `*deleteMac`), for a hand pass */
  leftoverMentions: string[]
  /** the old English and every new branch of each applied row, for the test-file search */
  texts: Map<string, string[]>
}

/** One row, checked against today's English and ready to place in every catalog. */
interface Target {
  row: ApplyRow
  value: string
  branches: PlatformBranch[]
  /** the key macOS read before this plan */
  macKey: string
  /** the key Windows and Linux read before this plan */
  otherKey: string
  collapsing: boolean
}

interface LocaleState {
  locale: string
  isOverlay: boolean
  /** the catalog an overlay overrides: `en` for a full locale */
  parent: string
  files: Record<string, AreaFile>
  texts: Record<string, string>
}

/** One survivor entry to write where an old key stood. */
interface Insertion {
  key: string
  value: string
  meta: Meta | undefined
}

/** The edits to one catalog file: what to drop, and what to write at which dropped key. */
interface FileEdits {
  dropped: Set<string>
  insertions: Map<string, Insertion[]>
}

function readCatalogDir(
  root: string,
  locale: string,
): { files: Record<string, AreaFile>; texts: Record<string, string> } {
  const dir = join(root, locale)
  const files: Record<string, AreaFile> = {}
  const texts: Record<string, string> = {}
  for (const name of readdirSync(dir)) {
    if (!name.endsWith('.json')) continue
    texts[name] = readFileSync(join(dir, name), 'utf8')
    files[name] = JSON.parse(texts[name]) as AreaFile
  }
  return { files, texts }
}

function stringOf(file: AreaFile | undefined, key: string): string | undefined {
  const value = file?.[key]
  return typeof value === 'string' ? value : undefined
}

function metaOf(file: AreaFile | undefined, key: string): Meta | undefined {
  const value = file?.[`@${key}`]
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Meta) : undefined
}

/**
 * Serializes a catalog file the way the catalogs are written: two-space JSON and
 * a trailing newline, plus the blank lines a hand-kept `en` file uses between
 * sections. A blank line is kept before the key it preceded (`blankBefore`).
 */
export function serializeCatalog(file: AreaFile, blankBefore: ReadonlySet<string> = new Set()): string {
  const text = `${JSON.stringify(file, null, 2)}\n`
  if (blankBefore.size === 0) return text
  return text
    .split('\n')
    .map((line) => {
      const key = topLevelKeyOf(line)
      return key !== undefined && blankBefore.has(key) ? `\n${line}` : line
    })
    .join('\n')
}

/** The entry name a top-level catalog line opens (`  "key": …`), or `undefined`. */
function topLevelKeyOf(line: string): string | undefined {
  const match = /^ {2}("(?:[^"\\]|\\.)*"): /.exec(line)
  return match ? (JSON.parse(match[1]) as string) : undefined
}

/** The top-level entry names a blank line precedes in a catalog file's text. */
export function blankLineKeys(text: string): Set<string> {
  const keys = new Set<string>()
  const lines = text.split('\n')
  lines.forEach((line, index) => {
    if (line !== '' || index + 1 >= lines.length) return
    const key = topLevelKeyOf(lines[index + 1])
    if (key !== undefined) keys.add(key)
  })
  return keys
}

/** Why a pair row can't fold: its survivor must be the one the frozen macOS copy maps the pair to. */
function pairIssue(row: ApplyRow): string | undefined {
  const pair = row.collapseFrom
  if (pair === undefined) return undefined
  const mapped = COLLAPSED_PAIRS.find((entry) => entry.mac === pair.mac && entry.other === pair.other)
  if (mapped?.survivor === row.key) return undefined
  return `COLLAPSED_PAIRS (i18n-macos-copy-baseline-lib.ts) doesn't fold ${pair.mac} into ${row.key}`
}

/** Why a value can't replace `oldMac`: it would change what macOS reads, outside the two accepted changes. */
function macosMoveIssue(row: ApplyRow, value: string, oldMac: string): string | undefined {
  const before = pickPlatformBranch(oldMac, 'macos')
  const after = pickPlatformBranch(value, 'macos')
  if (after === before) return undefined
  if (row.macosChanged === true && ACCEPTED_ENGLISH_CHANGES[row.key] === after) return undefined
  return `the macOS text would move from ${JSON.stringify(before)} to ${JSON.stringify(after)}; only the two accepted changes may`
}

/** Checks one row against today's English: a target to place, a skip, or a refusal. */
function targetOf(row: ApplyRow, en: AreaFile | undefined): Target | 'applied' | RefusedRow {
  const refuse = (reason: string): RefusedRow => ({ key: row.key, reason })
  const issues = [...englishValueIssues(row), pairIssue(row)].filter((issue) => issue !== undefined)
  if (issues.length > 0) return refuse(issues.join('; '))
  const value = englishValueOf(row)

  // A pair still folds while English holds its `.mac` key; afterwards the row is its survivor alone.
  const pair = row.collapseFrom
  const collapse = pair !== undefined && stringOf(en, pair.mac) !== undefined ? pair : undefined
  const macKey = collapse?.mac ?? row.key
  const otherKey = collapse?.other ?? row.key
  const oldMac = stringOf(en, macKey)
  if (oldMac === undefined) return refuse(`English ${row.file} lacks ${macKey}`)
  if (stringOf(en, otherKey) === undefined) return refuse(`English ${row.file} lacks ${otherKey}`)
  if (collapse === undefined && oldMac === value) return 'applied'

  const moved = macosMoveIssue(row, value, oldMac)
  if (moved !== undefined) return refuse(moved)
  return { row, value, branches: platformBranches(value), macKey, otherKey, collapsing: collapse !== undefined }
}

/** Whether macOS reads `branch` of a value with these branches. */
function isMacRead(branch: PlatformBranch, branches: readonly PlatformBranch[]): boolean {
  if (branch.name === null || branch.name === 'macos') return true
  return branch.name === 'other' && !branches.some(({ name }) => name === 'macos')
}

interface LocaleValueInput {
  target: Target
  state: LocaleState
  /** today's English, by key */
  enOld: AreaFile
  /** what the locale's stored hashes were made from: `en`, or an overlay's parent layered over it */
  sourceOld: (key: string) => string | undefined
  /** the parent's value for the survivor after this plan */
  parentNew: string
  report: LocaleReport
}

/**
 * A locale's new value for a row, or `undefined` when an overlay doesn't fork it.
 * @throws when a full locale lacks the key macOS reads: coverage is already broken
 */
function localeValueOf(input: LocaleValueInput): Insertion | undefined {
  const { target, state, enOld, sourceOld, parentNew, report } = input
  const { row, branches, macKey, otherKey } = target
  const own = state.files[row.file] as AreaFile | undefined
  const ownMac = stringOf(own, macKey)
  const ownOther = stringOf(own, otherKey)
  if (ownMac === undefined && ownOther === undefined) {
    if (state.isOverlay) return undefined
    throw new Error(`${state.locale}/${row.file} lacks ${macKey}: a full locale carries every key`)
  }
  const isStale = (key: string): boolean => {
    const source = sourceOld(key)
    return source === undefined || metaOf(own, key)?.sourceHash !== sourceHash(source)
  }

  const englishBefore = report.english
  const texts = branches.map((branch): PlatformBranch => {
    const macRead = isMacRead(branch, branches)
    const reader: RenderedPlatform = branch.name === null ? 'macos' : branchReader(branch.name)
    const sourceKey = macRead ? macKey : otherKey
    const ownText = macRead ? ownMac : ownOther
    if (ownText === undefined) {
      if (state.isOverlay) return { name: branch.name, text: pickPlatformBranch(parentNew, reader) }
      if (macRead) throw new Error(`${state.locale}/${row.file} lacks ${macKey}: a full locale carries every key`)
      report.missingOther.push(row.key)
      report.english += 1
      return branch
    }
    const stale = isStale(sourceKey)
    if (stale && !report.preStale.includes(sourceKey)) report.preStale.push(sourceKey)
    const unchanged = branch.text === pickPlatformBranch(stringOf(enOld, sourceKey) ?? '', reader)
    if (macRead || (unchanged && !stale)) {
      report.carried += 1
      return { name: branch.name, text: pickPlatformBranch(ownText, macRead ? 'macos' : reader) }
    }
    report.english += 1
    return branch
  })

  const value = formatPlatformBranches(texts)
  if (state.isOverlay && value === parentNew) {
    report.droppedForks.push(row.key)
    return undefined
  }
  const fromEnglish = report.english > englishBefore
  if (fromEnglish && state.isOverlay && state.parent === BASE_LOCALE) report.spellingPass.push(row.key)
  return { key: row.key, value, meta: metaOf(own, ownMac !== undefined ? macKey : otherKey) }
}

/** The survivor's English `@key` block: the `.mac` key's, plus any placeholder only the `.other` key documented. */
function englishMeta(en: AreaFile | undefined, target: Target): Meta | undefined {
  const macMeta = metaOf(en, target.macKey)
  const otherMeta = target.collapsing ? metaOf(en, target.otherKey) : undefined
  const otherPlaceholders = otherMeta?.placeholders
  if (macMeta === undefined || typeof otherPlaceholders !== 'object' || otherPlaceholders === null) {
    return macMeta ?? otherMeta
  }
  const macPlaceholders = (macMeta.placeholders ?? {}) as Record<string, unknown>
  return { ...macMeta, placeholders: { ...(otherPlaceholders as Record<string, unknown>), ...macPlaceholders } }
}

function editsFor(edits: Map<string, FileEdits>, file: string): FileEdits {
  let entry = edits.get(file)
  if (!entry) {
    entry = { dropped: new Set(), insertions: new Map() }
    edits.set(file, entry)
  }
  return entry
}

/** Records one row's outcome for a locale: its old keys go, and the survivor lands where the first of them stood. */
function recordEdit(
  edits: Map<string, FileEdits>,
  own: AreaFile,
  target: Target,
  insertion: Insertion | undefined,
): void {
  const fileEdits = editsFor(edits, target.row.file)
  const oldKeys = [target.macKey, target.otherKey].filter((key) => key in own)
  for (const key of oldKeys) fileEdits.dropped.add(key)
  if (insertion === undefined || oldKeys.length === 0) return
  const anchor = oldKeys[0]
  fileEdits.insertions.set(anchor, [...(fileEdits.insertions.get(anchor) ?? []), insertion])
}

/** Rewrites one parsed catalog file in its own key order: dropped keys go, each anchor yields its survivors. */
function applyEdits(file: AreaFile, edits: FileEdits): AreaFile {
  const out: AreaFile = {}
  for (const [key, value] of Object.entries(file)) {
    const base = isMetadataKey(key) ? key.slice(1) : key
    if (!edits.dropped.has(base)) {
      out[key] = value
      continue
    }
    if (isMetadataKey(key)) continue
    for (const insertion of edits.insertions.get(key) ?? []) {
      out[insertion.key] = insertion.value
      if (insertion.meta !== undefined) out[`@${insertion.key}`] = insertion.meta
    }
  }
  return out
}

/** The blank-line keys of a rewritten file: a dropped key's blank line moves to the first survivor written there. */
function movedBlankLines(blankBefore: ReadonlySet<string>, edits: FileEdits): Set<string> {
  const moved = new Set<string>()
  for (const key of blankBefore) {
    if (!edits.dropped.has(key)) moved.add(key)
    else {
      const first = edits.insertions.get(key)?.[0]
      if (first) moved.add(first.key)
    }
  }
  return moved
}

function loadStates(root: string): LocaleState[] {
  const locales = listLocales(root).filter((locale) => !GENERATED_LOCALES.has(locale))
  const states = locales.map((locale): LocaleState => {
    const { overrides, isOverlay } = resolveLocaleSource(locale, locales)
    return { locale, isOverlay, parent: overrides, ...readCatalogDir(root, locale) }
  })
  // `en`, then the full locales, then the overlays, so an overlay can read its parent's new values.
  const rank = (state: LocaleState): number => (state.locale === BASE_LOCALE ? 0 : state.isOverlay ? 2 : 1)
  return states.sort((a, b) => rank(a) - rank(b) || a.locale.localeCompare(b.locale))
}

function emptyReport(locale: string): LocaleReport {
  return { locale, carried: 0, english: 0, preStale: [], missingOther: [], droppedForks: [], spellingPass: [] }
}

interface PlacedLocale {
  /** survivor → the locale's own new value */
  values: Map<string, string>
  report: LocaleReport
}

/** Places every target in one non-English locale, recording the file edits. */
function placeLocale(
  state: LocaleState,
  targets: readonly Target[],
  context: { en: LocaleState; parentState: LocaleState | undefined; parentValues: Map<string, string> | undefined },
  edits: Map<string, FileEdits>,
): PlacedLocale {
  const report = emptyReport(state.locale)
  const values = new Map<string, string>()
  for (const target of targets) {
    const { row } = target
    const enOld = context.en.files[row.file]
    const parentOld: AreaFile | undefined = context.parentState?.files[row.file]
    const insertion = localeValueOf({
      target,
      state,
      enOld,
      sourceOld: (key) => stringOf(parentOld, key) ?? stringOf(enOld, key),
      parentNew: context.parentValues?.get(row.key) ?? target.value,
      report,
    })
    const own = state.files[row.file] as AreaFile | undefined
    if (own !== undefined) recordEdit(edits, own, target, insertion)
    if (insertion !== undefined) values.set(row.key, insertion.value)
  }
  return { values, report }
}

/** Old key → the survivors it folds into, for the pairs this plan collapses. */
function renamesOf(targets: readonly Target[]): Map<string, string[]> {
  const renames = new Map<string, string[]>()
  for (const { row, collapsing, macKey, otherKey } of targets) {
    if (!collapsing) continue
    for (const oldKey of [macKey, otherKey]) {
      if (oldKey === row.key) continue
      renames.set(oldKey, [...(renames.get(oldKey) ?? []), row.key])
    }
  }
  return renames
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** Renames every whole-key mention of an old key in prose to its (first) survivor. */
function renameInText(text: string, renames: ReadonlyMap<string, string[]>): string {
  let out = text
  for (const [oldKey, survivors] of renames) {
    // A whole key: not the tail of a longer one, and not the head of one (`….mac.label`).
    const mention = new RegExp(`(?<![A-Za-z0-9_.])${escapeRegExp(oldKey)}(?![A-Za-z0-9_]|\\.[A-Za-z_])`, 'g')
    out = out.replace(mention, survivors[0])
  }
  return out
}

/** An `exceptions` map with its old keys renamed; a pair's two reasons merge on the survivor. */
function renameExceptions(exceptions: Record<string, unknown>, renames: ReadonlyMap<string, string[]>): unknown {
  const merged = new Map<string, string[]>()
  const passthrough = new Map<string, unknown>()
  for (const [key, reason] of Object.entries(exceptions)) {
    if (typeof reason !== 'string') {
      passthrough.set(key, reason)
      continue
    }
    const text = renameInText(reason, renames)
    for (const survivor of renames.get(key) ?? [key]) {
      const held = merged.get(survivor) ?? []
      if (!held.includes(text)) held.push(text)
      merged.set(survivor, held)
    }
  }
  return {
    ...Object.fromEntries(passthrough),
    ...Object.fromEntries([...merged].map(([key, reasons]) => [key, reasons.join(' Off macOS: ')])),
  }
}

/** A termbase JSON with every key mention renamed: `exceptions` keys by structure, everything else as prose. */
function renameInJson(value: unknown, renames: ReadonlyMap<string, string[]>, parentKey?: string): unknown {
  if (typeof value === 'string') return renameInText(value, renames)
  if (Array.isArray(value)) return value.map((item) => renameInJson(item, renames))
  if (typeof value !== 'object' || value === null) return value
  if (parentKey === 'exceptions') return renameExceptions(value as Record<string, unknown>, renames)
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, renameInJson(item, renames, key)]))
}

/** The capture report with each surface's old keys renamed, deduped, and sorted as before. */
function renameInCaptureReport(text: string, renames: ReadonlyMap<string, string[]>): string {
  const report = JSON.parse(text) as Record<string, { keys?: unknown }>
  for (const surface of Object.values(report)) {
    if (!Array.isArray(surface.keys)) continue
    const keys = surface.keys as string[]
    const renamed = [...new Set(keys.flatMap((key) => renames.get(key) ?? [key]))]
    surface.keys = renamed.length === keys.length && renamed.every((key, i) => key === keys[i]) ? keys : renamed.sort()
  }
  return `${JSON.stringify(report, null, 2)}\n`
}

const TERMBASE_FILES: ReadonlySet<string> = new Set(['concepts.json', 'concepts-proposed.json', 'terms.json'])

/** Every translator guide under `docsRoot`: the markdown docs and the termbase JSON. */
function translatorGuides(docsRoot: string): string[] {
  if (!existsSync(docsRoot)) return []
  const guides: string[] = []
  const walk = (dir: string): void => {
    for (const name of readdirSync(dir).sort()) {
      const path = join(dir, name)
      if (statSync(path).isDirectory()) walk(path)
      else if (name.endsWith('.md') || TERMBASE_FILES.has(name)) guides.push(path)
    }
  }
  walk(docsRoot)
  return guides
}

/** Last segments that say nothing alone, so a leftover search for them would be noise. */
const GENERIC_SEGMENTS: ReadonlySet<string> = new Set(['mac', 'other', 'label'])

/** Renames collapsed keys in everything outside the catalogs that names them, and lists what a hand pass still owes. */
function planRenames(
  renames: ReadonlyMap<string, string[]>,
  paths: { messagesRoot: string; docsRoot: string; termAllowlistPath: string | null },
): { files: Map<string, string>; leftoverMentions: string[] } {
  const files = new Map<string, string>()
  const leftoverMentions: string[] = []
  if (renames.size === 0) return { files, leftoverMentions }
  const mentionsAny = (text: string): boolean => [...renames.keys()].some((key) => text.includes(key))
  const leaves = [...new Set([...renames.keys()].map((key) => key.slice(key.lastIndexOf('.') + 1)))].filter(
    (leaf) => !GENERIC_SEGMENTS.has(leaf),
  )
  // Shorthand only: the segment alone or behind a lone `.` or `*`, never the tail of some other full key.
  const leftover =
    leaves.length > 0
      ? new RegExp(`(?<![A-Za-z0-9_.*]|[A-Za-z0-9_][.*])[.*]?(${leaves.map(escapeRegExp).join('|')})\\b`, 'g')
      : undefined

  for (const path of translatorGuides(paths.docsRoot)) {
    const text = readFileSync(path, 'utf8')
    let renamed = text
    if (mentionsAny(text)) {
      renamed = path.endsWith('.json')
        ? `${JSON.stringify(renameInJson(JSON.parse(text), renames), null, 2)}\n`
        : renameInText(text, renames)
      if (renamed !== text) files.set(path, renamed)
    }
    if (leftover === undefined) continue
    const found = [...new Set([...renamed.matchAll(leftover)].map((match) => match[1]))]
    if (found.length > 0) leftoverMentions.push(`${path}: ${found.join(', ')}`)
  }

  const textual =
    paths.termAllowlistPath !== null && existsSync(paths.termAllowlistPath) ? [paths.termAllowlistPath] : []
  for (const path of textual) {
    const text = readFileSync(path, 'utf8')
    const renamed = renameInText(text, renames)
    if (renamed !== text) files.set(path, renamed)
  }

  const captureReport = join(paths.messagesRoot, 'screenshots', 'capture-report.json')
  if (existsSync(captureReport)) {
    const text = readFileSync(captureReport, 'utf8')
    if (mentionsAny(text)) files.set(captureReport, renameInCaptureReport(text, renames))
  }
  return { files, leftoverMentions }
}

const DEFAULT_TERM_ALLOWLIST = join(import.meta.dirname, 'i18n-term-consistency-allowlist.json')

/**
 * Plans the rewrite for `rows` over every shipped catalog. Reads only; nothing is
 * written. A row that can't be applied lands in `refused` and touches nothing.
 */
export function planPlatformCopy(rows: readonly ApplyRow[], paths: PlanPaths = {}): CopyPlan {
  const root = resolveMessagesRoot(paths.messagesRoot)
  const states = loadStates(root)
  const en = states.find((state) => state.locale === BASE_LOCALE)
  if (!en) throw new Error(`No English catalog under ${root}`)

  const targets: Target[] = []
  const alreadyApplied: string[] = []
  const refused: RefusedRow[] = []
  for (const row of rows) {
    const target = targetOf(row, en.files[row.file])
    if (target === 'applied') alreadyApplied.push(row.key)
    else if ('reason' in target) refused.push(target)
    else targets.push(target)
  }

  const catalogFiles = new Map<string, string>()
  const reports: LocaleReport[] = []
  const placed = new Map<string, Map<string, string>>()
  for (const state of states) {
    const edits = new Map<string, FileEdits>()
    if (state === en) {
      for (const target of targets) {
        const own = en.files[target.row.file]
        recordEdit(edits, own, target, { key: target.row.key, value: target.value, meta: englishMeta(own, target) })
      }
    } else {
      const parentState = state.isOverlay ? states.find((other) => other.locale === state.parent) : undefined
      const { values, report } = placeLocale(
        state,
        targets,
        { en, parentState, parentValues: placed.get(state.parent) },
        edits,
      )
      placed.set(state.locale, values)
      reports.push(report)
    }
    for (const [name, fileEdits] of edits) {
      const blankBefore = movedBlankLines(blankLineKeys(state.texts[name]), fileEdits)
      const text = serializeCatalog(applyEdits(state.files[name], fileEdits), blankBefore)
      if (text !== state.texts[name]) catalogFiles.set(join(root, state.locale, name), text)
    }
  }

  const renames = renamesOf(targets)
  const { files: otherFiles, leftoverMentions } = planRenames(renames, {
    messagesRoot: root,
    docsRoot: resolveDocsRoot(paths.docsRoot),
    termAllowlistPath: paths.termAllowlistPath === undefined ? DEFAULT_TERM_ALLOWLIST : paths.termAllowlistPath,
  })

  const texts = new Map<string, string[]>()
  for (const { row, branches, macKey, otherKey } of targets) {
    const old = [stringOf(en.files[row.file], macKey), stringOf(en.files[row.file], otherKey)]
    const all = [...old.filter((text) => text !== undefined), ...branches.map(({ text }) => text)]
    texts.set(row.key, [...new Set(all.flatMap((text) => platformBranches(text).map((branch) => branch.text)))])
  }

  return {
    catalogFiles,
    otherFiles,
    applied: targets.map(({ row }) => row.key),
    alreadyApplied,
    refused,
    locales: reports,
    renames,
    leftoverMentions,
    texts,
  }
}
