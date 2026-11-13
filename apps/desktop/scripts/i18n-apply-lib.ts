/**
 * Writes a translator's values into ONE locale's catalogs, the write-back half of
 * `pnpm i18n:apply` (`i18n-apply.ts` is the CLI). Before this existed, every
 * translator of the 2026-11-12 fan-out wrote its own apply-and-restamp script
 * (about 850 lines across 13 agents), each with its own idea of the file format.
 *
 * One call plans the whole batch first and writes nothing unless every value
 * passes, so a bad value can't leave a half-applied locale behind:
 *
 *  - the key must be an English key, and (for a full translation) already in the
 *    locale's catalog: a brand-new English key arrives through
 *    `sync-locale-keys.ts`, which this never replaces
 *  - the value passes the ERROR-class mechanics the locale checks hold it to:
 *    placeholder/tag parity (`parityDetail`), ICU validity and raw-family
 *    apostrophes (`icuError`), the per-OS block shape (`platformSelectIssues`),
 *    plural coverage (`platformPluralDetail`), and the frozen macOS text
 *    (`frozenMacosIssue`), each against the value's SOURCE: `en`, or for an overlay
 *    the catalog it overrides
 *  - an overlay's fork may not equal what it overrides (delete it with `null`)
 *
 * Each written key is stamped with the hash of its current source, and a value or
 * hash that moved drops the source-bound vouches (`reviewed`,
 * `sameAsSourceJustification`) exactly as `--restamp` does; a `@key` entry in the
 * values file then sets a fresh justification. Files are written in their
 * canonical shape (`JSON.stringify(…, null, 2)`, the shape `sync-locale-keys.ts`
 * writes), so untouched lines stay byte for byte; a file that isn't in that shape
 * is refused rather than reformatted.
 *
 * A per-OS key can be written whole, or as `{ "windows": "…", "other": "…" }`:
 * each branch left out keeps the text that OS reads in the catalog now, which is
 * how a translator leaves the macOS branch alone without retyping it.
 */

import { existsSync, mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { parsePlatformSelect, pickPlatformBranch } from '../src/lib/intl/platform-select.ts'
import type { PlatformSelectBranch } from '../src/lib/intl/platform-select.ts'
import {
  BASE_LOCALE,
  branchReader,
  formatPlatformBranches,
  isMetadataKey,
  layerCatalogs,
  listLocales,
  loadCatalog,
  platformBranches,
  readLocaleFiles,
  resolveLocaleSource,
  resolveMessagesRoot,
  sourceHash,
} from './i18n-catalog-lib.ts'
import { parityDetail } from './i18n-check-parity.ts'
import { icuError } from './i18n-check-icu.ts'
import { platformSelectIssues } from './i18n-check-platform-select.ts'
import { platformPluralDetail } from './i18n-check-plural.ts'
import {
  acceptedChangesPath,
  frozenMacosIssue,
  loadAcceptedTranslationChanges,
} from './i18n-macos-copy-baseline-lib.ts'
import type {
  AcceptedChangesFile,
  AcceptedTranslationChange,
  MacosCopyBaseline,
} from './i18n-macos-copy-baseline-lib.ts'
import { restampMeta } from './sync-locale-keys.ts'

const BRANCH_NAMES: readonly PlatformSelectBranch[] = ['macos', 'windows', 'other']

/** A per-OS value given branch by branch; a branch left out keeps what that OS reads now. */
export type BranchValues = Partial<Record<PlatformSelectBranch, string>>

/** `{ "keep": true }`: the translation already there still fits the new English, so stamp it as it is. */
export interface KeepEntry {
  keep: true
}

/**
 * One key's entry in a values file: the whole value, its branches, `{ "keep": true }`, or `null` (an overlay drops its
 * fork).
 */
export type ValueEntry = string | BranchValues | KeepEntry | null

/** A parsed values file. */
export interface ValuesFile {
  /** key → entry, in file order */
  values: Map<string, ValueEntry>
  /** key → the `@key.sameAsSourceJustification` to record */
  justifications: Map<string, string>
  /** key → why its frozen macOS text changes (`@key.macosChangeReason`), recorded as an accepted change */
  macosReasons: Map<string, string>
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/** Whether an entry is `{ "keep": true }`. */
export function isKeep(entry: ValueEntry): entry is KeepEntry {
  return isRecord(entry) && Object.keys(entry).length === 1 && (entry as Record<string, unknown>).keep === true
}

/** The `@key` fields a values file may carry, each a non-empty string. */
const NOTE_FIELDS = ['sameAsSourceJustification', 'macosChangeReason'] as const

/**
 * Reads a values file's JSON: `"key": "value"`, `"key": { "windows": "…" }`, `"key": { "keep": true }`, or
 * `"key": null`, plus an optional ARB-style `"@key": { "sameAsSourceJustification": "…", "macosChangeReason": "…" }`,
 * the same shape the catalogs use.
 * @throws one line per malformed entry
 */
export function parseValuesFile(raw: unknown): ValuesFile {
  if (!isRecord(raw)) throw new Error('The values file must be a JSON object of key → value.')
  const file: ValuesFile = { values: new Map(), justifications: new Map(), macosReasons: new Map() }
  const problems: string[] = []
  for (const [name, entry] of Object.entries(raw)) {
    if (isMetadataKey(name)) {
      const notes = notesOf(entry)
      if (notes === undefined) problems.push(`${name}: ${NOTE_SHAPE}`)
      else {
        if (notes.sameAsSourceJustification) file.justifications.set(name.slice(1), notes.sameAsSourceJustification)
        if (notes.macosChangeReason) file.macosReasons.set(name.slice(1), notes.macosChangeReason)
      }
      continue
    }
    const value = valueEntryOf(entry)
    if (value === undefined) problems.push(`${name}: ${VALUE_SHAPE}`)
    else file.values.set(name, value)
  }
  for (const key of new Set([...file.justifications.keys(), ...file.macosReasons.keys()])) {
    if (!file.values.has(key)) problems.push(`@${key}: names a key the file gives no value for`)
  }
  if (problems.length > 0) throw new Error(problems.join('\n'))
  return file
}

const NOTE_SHAPE =
  'a "@key" entry takes "sameAsSourceJustification" and/or "macosChangeReason", each a non-empty string, and nothing else'
const VALUE_SHAPE =
  'the value must be a string, null, { "keep": true }, or a per-OS entry mapping "macos", "windows", or "other" to a string'

/** A `@key` entry's notes, or `undefined` when the entry has any other shape. */
function notesOf(entry: unknown): Partial<Record<(typeof NOTE_FIELDS)[number], string>> | undefined {
  if (!isRecord(entry) || Object.keys(entry).length === 0) return undefined
  const fields = Object.entries(entry)
  const wellFormed = fields.every(
    ([field, text]) =>
      (NOTE_FIELDS as readonly string[]).includes(field) && typeof text === 'string' && text.trim() !== '',
  )
  return wellFormed ? entry : undefined
}

/** A key's entry, or `undefined` when it's malformed. */
function valueEntryOf(entry: unknown): ValueEntry | undefined {
  if (entry === null || typeof entry === 'string') return entry
  if (!isRecord(entry) || Object.keys(entry).length === 0) return undefined
  if (isKeep(entry)) return { keep: true }
  const wellFormed = Object.entries(entry).every(
    ([branch, text]) => BRANCH_NAMES.includes(branch as PlatformSelectBranch) && typeof text === 'string',
  )
  return wellFormed ? entry : undefined
}

/** One key the plan refused, and why. Any refusal means nothing is written. */
export interface Refusal {
  key: string
  reason: string
}

/** One key the plan writes. */
export interface PlannedWrite {
  key: string
  /** the area file, like `settings.json` */
  file: string
  /** the value now, or `undefined` for an overlay's new fork */
  before: string | undefined
  /** the value to write, or `null` when an overlay drops its fork */
  after: string | null
  /** whether the stored `sourceHash` moves */
  restamped: boolean
}

/** What one `planApply` call would do. */
export interface ApplyPlan {
  tag: string
  isOverlay: boolean
  refused: Refusal[]
  writes: PlannedWrite[]
  /** keys whose value and stamp are already current */
  unchanged: string[]
  /** informational lines: branches kept from the catalog, branches still English */
  notes: string[]
  /** area file → its new text, for the files that change */
  files: Map<string, string>
  /** the batch's `@key` justifications, key → text */
  justifications: ReadonlyMap<string, string>
  /**
   * The locale's accepted macOS-text changes to write (`acceptedChangesPath`): key → the entry, or `null` to drop one a
   * value returned to the frozen text. Empty when nothing frozen moves.
   */
  accepted: Map<string, AcceptedChangesFile[string] | null>
  /** the locale's accepted-changes file, set on a clean plan */
  acceptedPath?: string
}

/** The inputs of `planApply`. */
export interface ApplyOptions {
  tag: string
  values: ValuesFile
  messagesRoot?: string
  /** the frozen macOS copy; its check is skipped when absent (fixtures) */
  baseline?: MacosCopyBaseline
  /** where the per-catalog accepted macOS-text changes live (`ACCEPTED_CHANGES_RELATIVE_DIR` by default) */
  acceptedDir?: string
}

/** The catalog file an English key lives in (`settings.json`), from `en`'s own layout. */
function englishFileOf(enFiles: Record<string, Record<string, unknown>>, key: string): string | undefined {
  return Object.keys(enFiles).find((name) => key in enFiles[name])
}

/**
 * Assembles a per-OS entry into one canonical block in the source's branch set.
 * A branch the entry leaves out keeps what that OS reads in `current` today.
 */
function assembleBranches(
  entry: BranchValues,
  source: string,
  current: string,
): { value: string; kept: PlatformSelectBranch[] } | { error: string } {
  const sourceBranches = parsePlatformSelect(source)
  if (sourceBranches === null) return { error: 'its source is a plain value, so give the whole value as a string' }
  const names = BRANCH_NAMES.filter((name) => sourceBranches[name] !== undefined)
  const extra = Object.keys(entry).filter((name) => !names.includes(name as PlatformSelectBranch))
  if (extra.length > 0) return { error: `its source has no ${extra.join(', ')} branch (it has ${names.join(', ')})` }
  const kept = names.filter((name) => entry[name] === undefined)
  const branches = names.map((name) => ({
    name,
    text: entry[name] ?? pickPlatformBranch(current, branchReader(name)),
  }))
  return { value: formatPlatformBranches(branches), kept }
}

/** Every reason `value` can't be written for `key` against `source`, or `[]`. */
function valueIssues(tag: string, key: string, value: string, source: string): string[] {
  const issues = [
    parityDetail(key, source, value),
    icuError(key, value),
    ...platformSelectIssues({ key, value, sourceValue: source }),
    platformPluralDetail(tag, source, value),
  ]
  return issues.filter((issue): issue is string => issue !== null)
}

/** Whether a catalog file's text is in the canonical shape apply writes. */
function isCanonical(text: string): boolean {
  return JSON.stringify(JSON.parse(text), null, 2) + '\n' === text
}

/**
 * Inserts `key` (and its `@key`) into an overlay file in English catalog order:
 * before the first existing key English lists after it.
 */
function insertInEnglishOrder(
  file: Record<string, unknown>,
  key: string,
  englishOrder: ReadonlyMap<string, number>,
): Record<string, unknown> {
  const rank = englishOrder.get(key) ?? Number.MAX_SAFE_INTEGER
  const entries = Object.entries(file)
  const at = entries.findIndex(([name]) => !isMetadataKey(name) && (englishOrder.get(name) ?? -1) > rank)
  const head = at === -1 ? entries : entries.slice(0, at)
  const tail = at === -1 ? [] : entries.slice(at)
  return Object.fromEntries([...head, [key, ''], ...tail])
}

/** What every key of one batch is planned against. */
interface PlanContext {
  tag: string
  isOverlay: boolean
  overrides: string
  english: Record<string, string>
  /** what each key's value is judged and stamped against: `en`, or an overlay's layered base */
  source: Record<string, string>
  enFiles: Record<string, Record<string, unknown>>
  englishOrder: ReadonlyMap<string, number>
  localeFiles: Record<string, Record<string, unknown>>
  /** area file → its working copy, for the files the batch touches */
  edited: Map<string, Record<string, unknown>>
  baseline: MacosCopyBaseline | undefined
  /** every accepted change on disk, the frozen check's allowance; this locale's are what a write may replace */
  acceptedChanges: readonly AcceptedTranslationChange[]
  macosReasons: ReadonlyMap<string, string>
  plan: ApplyPlan
}

/** Where a key lives in the locale, and what it holds there now. */
interface KeySlot {
  fileName: string
  /** the value now, or `undefined` for an overlay key it doesn't fork yet */
  before: string | undefined
}

/** The working copy of one area file, created from the catalog on first touch. */
function workingFile(ctx: PlanContext, fileName: string): Record<string, unknown> {
  const existing = ctx.edited.get(fileName)
  if (existing) return existing
  const copy = { ...(ctx.localeFiles[fileName] ?? {}) }
  ctx.edited.set(fileName, copy)
  return copy
}

/** The file `key` lives in (or, for a new overlay fork, will). A full locale's missing key is a refusal reason. */
function slotFor(ctx: PlanContext, key: string): KeySlot | string {
  const ownFile = Object.keys(ctx.localeFiles).find((name) => key in ctx.localeFiles[name])
  if (ownFile !== undefined) return { fileName: ownFile, before: workingFile(ctx, ownFile)[key] as string | undefined }
  if (!ctx.isOverlay) {
    return `not in ${ctx.tag}'s catalog yet: run \`node scripts/sync-locale-keys.ts ${ctx.tag}\` first`
  }
  return { fileName: englishFileOf(ctx.enFiles, key) ?? `${key.split('.')[0]}.json`, before: undefined }
}

/** `{ "keep": true }`: the value the catalog holds now, which the write restamps unchanged. */
function keptValue(ctx: PlanContext, key: string, slot: KeySlot): string | Refusal {
  if (slot.before === undefined) return { key, reason: `${ctx.tag} doesn't fork this key, so there's nothing to keep` }
  ctx.plan.notes.push(`${key}: kept as is, stamped against the current ${ctx.isOverlay ? ctx.overrides : 'English'}`)
  return slot.before
}

/** The value an entry resolves to: itself, or its branches assembled over what the catalog holds now. */
function resolveValue(ctx: PlanContext, key: string, entry: string | BranchValues, slot: KeySlot): string | Refusal {
  if (typeof entry === 'string') return entry
  const sourceValue = ctx.source[key]
  const assembled = assembleBranches(entry, sourceValue, slot.before ?? sourceValue)
  if ('error' in assembled) return { key, reason: `per-OS entry: ${assembled.error}` }
  if (assembled.kept.length > 0) ctx.plan.notes.push(`${key}: kept from the catalog: ${assembled.kept.join(', ')}`)
  return assembled.value
}

/**
 * The frozen macOS text rule for one value, recording what the accepted-changes file needs. The value is judged
 * against the frozen text alone (this key's own accepted entry left out): unchanged, any old entry goes; changed with a
 * reason in the file, or with the entry already accepting exactly this text, it lands and the entry is (re)written;
 * changed with neither, it's refused. A reason on a value whose macOS text doesn't change is refused too.
 */
function frozenTextIssue(ctx: PlanContext, key: string, value: string): string | null {
  if (!ctx.baseline) return null
  const reason = ctx.macosReasons.get(key)
  const others = ctx.acceptedChanges.filter((change) => change.locale !== ctx.tag || change.key !== key)
  const own = ctx.acceptedChanges.find((change) => change.locale === ctx.tag && change.key === key)
  const issue = frozenMacosIssue(ctx.baseline, { locale: ctx.tag, key, value }, others)
  const text = pickPlatformBranch(value, 'macos')
  if (issue === null) {
    if (reason !== undefined)
      return `macosChangeReason given, but the macOS text doesn't change from the frozen one: drop it`
    if (own) ctx.plan.accepted.set(key, null)
    return null
  }
  if (reason !== undefined) ctx.plan.accepted.set(key, { text, reason })
  else if (own?.text !== text) return issue
  return null
}

/** Every reason the resolved value can't land: the mechanics, then the overlay and macOS rules. */
function landingIssues(ctx: PlanContext, key: string, value: string, justification: string | undefined): string[] {
  const sourceValue = ctx.source[key]
  const issues = valueIssues(ctx.tag, key, value, sourceValue)
  const frozen = frozenTextIssue(ctx, key, value)
  if (frozen !== null) issues.push(frozen)
  if (ctx.isOverlay && value === sourceValue) {
    issues.push(`identical to what it overrides (${ctx.overrides}): drop the fork with null instead`)
  }
  if (ctx.isOverlay && justification !== undefined) {
    issues.push('an overlay records no sameAsSourceJustification: a fork identical to its source is deleted')
  }
  return issues
}

/**
 * The branches of a translating locale's value still byte-identical to English's
 * same branch: pending translation, as the judging checks read them. An overlay
 * forks on purpose, so it has none.
 */
function stillEnglishBranches(ctx: PlanContext, key: string, value: string): PlatformSelectBranch[] {
  if (ctx.isOverlay) return []
  const englishBranches = parsePlatformSelect(ctx.english[key])
  return platformBranches(value).flatMap(({ name, text }) =>
    name !== null && englishBranches?.[name] === text ? [name] : [],
  )
}

/** Notes on a value that lands but leaves work: branches still English, or a whole value identical to English. */
function workLeftNotes(
  ctx: PlanContext,
  { key, value, stillEnglish, justification }: WriteRequest & { stillEnglish: readonly string[] },
): string[] {
  const notes: string[] = []
  if (stillEnglish.length > 0 && justification === undefined) {
    notes.push(`${key}: still English in ${stillEnglish.join(', ')}: the key stays stale until they're done`)
  }
  if (!ctx.isOverlay && value === ctx.english[key] && justification === undefined) {
    notes.push(`${key}: identical to English; add "@${key}": { "sameAsSourceJustification": "…" } if that's deliberate`)
  }
  return notes
}

/** Drops an overlay's fork, or refuses the `null` a full translation can't take. */
function planDrop(ctx: PlanContext, key: string, slot: KeySlot): void {
  if (!ctx.isOverlay) {
    ctx.plan.refused.push({ key, reason: `${ctx.tag} is a full translation, so every key stays: give it a value` })
    return
  }
  if (slot.before === undefined) {
    ctx.plan.refused.push({ key, reason: `${ctx.tag} doesn't fork this key, so there's nothing to drop` })
    return
  }
  const metaKey = `@${key}`
  const file = workingFile(ctx, slot.fileName)
  ctx.edited.set(
    slot.fileName,
    Object.fromEntries(Object.entries(file).filter(([name]) => name !== key && name !== metaKey)),
  )
  ctx.plan.writes.push({ key, file: slot.fileName, before: slot.before, after: null, restamped: false })
}

/** One value to write, resolved and judged. */
interface WriteRequest {
  key: string
  value: string
  slot: KeySlot
  justification?: string
}

/**
 * Writes the value and its `@key` into the working copy: stamped with the
 * source's hash, the source-bound vouches dropped when the value or hash moved,
 * and the file's justification on top. An unchanged, current key is left alone.
 *
 * `holdStamp` keeps the stored hash while a per-OS branch is still English: the
 * stale check's worklist is the only thing that says a half-translated key still
 * owes work (`docs/guides/i18n-translation.md`, a key whose English became a
 * per-OS value), so a stamp would hide it. A justification in the file lifts it.
 */
function planWrite(
  ctx: PlanContext,
  { key, value, slot, justification, holdStamp }: WriteRequest & { holdStamp: boolean },
): void {
  const metaKey = `@${key}`
  const file = workingFile(ctx, slot.fileName)
  const stored = file[metaKey]
  const meta = isRecord(stored) ? stored : {}
  const current = sourceHash(ctx.source[key])
  const hash = holdStamp && typeof meta.sourceHash === 'string' ? meta.sourceHash : current
  const restamped = meta.sourceHash !== hash
  const valueMoved = slot.before !== value
  const justificationMoved = justification !== undefined && justification !== meta.sameAsSourceJustification
  if (!valueMoved && !restamped && !justificationMoved) {
    ctx.plan.unchanged.push(key)
    return
  }
  const nextMeta = valueMoved || restamped ? restampMeta(meta, hash) : { ...meta }
  if (justification !== undefined) nextMeta.sameAsSourceJustification = justification
  // A new fork goes in English order; either way `@key` sits right after its key, the catalogs' layout.
  const placed = slot.before === undefined ? insertInEnglishOrder(file, key, ctx.englishOrder) : file
  ctx.edited.set(
    slot.fileName,
    Object.fromEntries(
      Object.entries(placed).flatMap(([name, item]): [string, unknown][] => {
        if (name === metaKey) return []
        return name === key
          ? [
              [name, value],
              [metaKey, nextMeta],
            ]
          : [[name, item]]
      }),
    ),
  )
  ctx.plan.writes.push({ key, file: slot.fileName, before: slot.before, after: value, restamped })
}

/** Plans one key of the batch. */
function planKey(ctx: PlanContext, key: string, entry: ValueEntry): void {
  if (!(key in ctx.english)) {
    ctx.plan.refused.push({ key, reason: 'not an English key (misspelled, or retired)' })
    return
  }
  const slot = slotFor(ctx, key)
  if (typeof slot === 'string') {
    ctx.plan.refused.push({ key, reason: slot })
    return
  }
  if (entry === null) {
    planDrop(ctx, key, slot)
    return
  }
  const value = isKeep(entry) ? keptValue(ctx, key, slot) : resolveValue(ctx, key, entry, slot)
  if (typeof value !== 'string') {
    ctx.plan.refused.push(value)
    return
  }
  const justification = ctx.plan.justifications.get(key)
  if (isKeep(entry) && justification === undefined && stillEnglishBranches(ctx, key, value).length > 0) {
    const branches = stillEnglishBranches(ctx, key, value).join(', ')
    ctx.plan.refused.push({
      key,
      reason: `keep says the translation is current, but ${branches} is still English: translate it`,
    })
    return
  }
  const issues = landingIssues(ctx, key, value, justification)
  if (issues.length > 0) {
    ctx.plan.refused.push({ key, reason: issues.join('; ') })
    return
  }
  const stillEnglish = stillEnglishBranches(ctx, key, value)
  const request: WriteRequest = { key, value, slot, justification }
  ctx.plan.notes.push(...workLeftNotes(ctx, { ...request, stillEnglish }))
  planWrite(ctx, { ...request, holdStamp: stillEnglish.length > 0 && justification === undefined })
}

/** The new text of every touched file, or a refusal per file that isn't in the canonical shape. */
function renderFiles(ctx: PlanContext, dir: string): void {
  for (const name of new Set(ctx.plan.writes.map((write) => write.file))) {
    const path = join(dir, name)
    if (existsSync(path) && !isCanonical(readFileSync(path, 'utf8'))) {
      ctx.plan.refused.push({
        key: name,
        reason: `${ctx.tag}/${name} isn't in the canonical catalog shape (JSON.stringify, two-space indent), so writing it would reformat untouched lines`,
      })
      continue
    }
    ctx.plan.files.set(name, JSON.stringify(ctx.edited.get(name), null, 2) + '\n')
  }
  if (ctx.plan.refused.length > 0) ctx.plan.files.clear()
}

/**
 * Plans one batch for one locale: every refusal, and for a clean batch the new
 * text of each catalog file it touches. Reads only; `writePlan` writes.
 */
export function planApply({ tag, values, messagesRoot, baseline, acceptedDir }: ApplyOptions): ApplyPlan {
  const root = resolveMessagesRoot(messagesRoot)
  const available = listLocales(root)
  if (tag === BASE_LOCALE) throw new Error(`Refusing to write the source locale '${BASE_LOCALE}'.`)
  if (!available.includes(tag)) throw new Error(`Not a locale: ${tag} (have: ${available.join(', ')}).`)
  const { overrides, isOverlay } = resolveLocaleSource(tag, available)
  const english = loadCatalog(BASE_LOCALE, root)
  const plan: ApplyPlan = {
    tag,
    isOverlay,
    refused: [],
    writes: [],
    unchanged: [],
    notes: [],
    files: new Map(),
    justifications: values.justifications,
    accepted: new Map(),
  }
  const ctx: PlanContext = {
    tag,
    isOverlay,
    overrides,
    english: english.messages,
    source: isOverlay ? layerCatalogs(english, loadCatalog(overrides, root)).messages : english.messages,
    enFiles: readLocaleFiles(BASE_LOCALE, root),
    englishOrder: new Map(Object.keys(english.messages).map((key, index) => [key, index])),
    localeFiles: readLocaleFiles(tag, root),
    edited: new Map(),
    baseline,
    acceptedChanges: baseline ? loadAcceptedTranslationChanges(acceptedDir) : [],
    macosReasons: values.macosReasons,
    plan,
  }
  for (const [key, entry] of values.values) planKey(ctx, key, entry)
  if (plan.refused.length === 0) renderFiles(ctx, join(root, tag))
  if (plan.refused.length === 0) plan.acceptedPath = acceptedChangesPath(tag, acceptedDir)
  return plan
}

/** Writes a clean plan's files, each through a temp file and a rename so a crash never leaves half a catalog. */
export function writePlan(plan: ApplyPlan, messagesRoot?: string): void {
  if (plan.refused.length > 0) throw new Error('Refusing to write a plan with refusals.')
  const dir = join(resolveMessagesRoot(messagesRoot), plan.tag)
  for (const [name, text] of plan.files) writeAtomically(join(dir, name), text)
  if (plan.accepted.size > 0 && plan.acceptedPath !== undefined) writeAcceptedChanges(plan.acceptedPath, plan.accepted)
}

/** Writes through a temp file and a rename, so a crash never leaves half a file. */
function writeAtomically(path: string, text: string): void {
  const temp = `${path}.tmp-${String(process.pid)}`
  writeFileSync(temp, text, 'utf8')
  renameSync(temp, path)
}

/**
 * Merges a plan's accepted-change edits into the locale's file (keys sorted, so two runs never reorder it), and
 * deletes the file once it holds nothing.
 */
function writeAcceptedChanges(path: string, edits: ReadonlyMap<string, AcceptedChangesFile[string] | null>): void {
  const stored = existsSync(path) ? (JSON.parse(readFileSync(path, 'utf8')) as AcceptedChangesFile) : {}
  const merged = new Map(Object.entries(stored))
  for (const [key, entry] of edits) {
    if (entry === null) merged.delete(key)
    else merged.set(key, entry)
  }
  if (merged.size === 0) {
    if (existsSync(path)) unlinkSync(path)
    return
  }
  mkdirSync(dirname(path), { recursive: true })
  const sorted = Object.fromEntries([...merged].sort(([a], [b]) => a.localeCompare(b)))
  writeAtomically(path, JSON.stringify(sorted, null, 2) + '\n')
}

/** The `expected` keys the batch gives no value for, in the order given. */
export function uncoveredKeys(expected: readonly string[], values: ValuesFile): string[] {
  return expected.filter((key) => !values.values.has(key))
}
