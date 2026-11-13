/**
 * Looks a term up in the reference pile, the evidence `pnpm i18n:pile`
 * (`i18n-pile.ts`) prints. It does what `docs/i18n/reference-pile/how-to-mine.md`'s
 * recipes do by hand, source by source, so a translator never writes a grep loop
 * over the main clone's path (a worktree harness guard rejects those, which is
 * why the 2026-11-12 translators each scripted their own `tbx.sh` or `mine.sh`):
 *
 *  - **macOS** (Tier 1): `en/macOS/<bundle>/<file>.json` and the same file in the
 *    language's folder, joined by KEY (the values-only grep trap 3 warns about
 *    returns English Siri phrases)
 *  - **Microsoft** (Tier 2): each `<termEntry>` of the `.tbx` whose English term
 *    matches, with every sense's definition (trap 4: the first sense is often not
 *    the UI one) and the target terms with their region
 *  - **Linux and the orthodox pair** (Tier 3): the gettext catalogs (Nautilus,
 *    Thunar, Dolphin, Double Commander, git) by `msgid`, mnemonics (`_`, `&`) ignored
 *  - **Total Commander**: no English side exists, so only a reverse lookup reads it
 *
 * A forward lookup (`--term`) matches the English; a reverse one (`--target`)
 * matches the translation and prints the English beside it. Exact matches come
 * first, then entries that contain the words, capped per source.
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'

/** Where a hit came from, in authority order. */
export type PileSource = 'macos' | 'microsoft' | 'linux' | 'orthodox' | 'git'

/** Every source, in the order a lookup prints them. */
export const PILE_SOURCES: readonly PileSource[] = Object.freeze(['macos', 'microsoft', 'linux', 'orthodox', 'git'])

/** The gettext catalogs per source, by pile subfolder. */
const GETTEXT: Readonly<Record<'linux' | 'orthodox' | 'git', readonly string[]>> = {
  linux: ['gnome-nautilus', 'xfce-thunar', 'kde-dolphin'],
  orthodox: ['double-commander'],
  git: ['git'],
}

/** One piece of evidence. */
export interface PileHit {
  source: PileSource
  /** the file it came from, relative to the pile root */
  file: string
  /** the English side (empty when the source has none) */
  english: string
  /** the translation */
  target: string
  /** a Microsoft sense's definition, a gettext context, or a macOS key */
  detail?: string
  exact: boolean
}

/** What to look for. */
export interface PileQuery {
  /** match the English side (forward) or the translation (reverse) */
  side: 'english' | 'target'
  text: string
}

/** Lowercased, mnemonics and a trailing ellipsis or colon gone, whitespace folded: how two labels compare. */
export function normalizeLabel(text: string): string {
  return text
    .replace(/&(?=\p{L})/gu, '')
    .replace(/_(?=\p{L})/gu, '')
    .replace(/(?:…|\.\.\.|:)\s*$/u, '')
    .replace(/\s+/gu, ' ')
    .trim()
    .toLowerCase()
}

/** Whether `haystack` contains `needle`'s words as a whole phrase. */
function containsPhrase(haystack: string, needle: string): boolean {
  if (needle.length === 0) return false
  const body = needle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/\s+/g, '\\s+')
  return new RegExp(`(?<![\\p{L}\\p{N}])${body}(?![\\p{L}\\p{N}])`, 'iu').test(haystack)
}

/** How `candidate` answers `query`: exactly, by containing it, or not at all. */
function matchKind(candidate: string, query: string): 'exact' | 'contains' | undefined {
  const label = normalizeLabel(candidate)
  const wanted = normalizeLabel(query)
  if (label === wanted) return 'exact'
  return containsPhrase(label, wanted) ? 'contains' : undefined
}

/** Every `*.json` under a directory, relative to it. */
function jsonFiles(dir: string): string[] {
  if (!existsSync(dir)) return []
  return readdirSync(dir, { recursive: true, encoding: 'utf8' }).filter(
    (path) => path.endsWith('.json') && statSync(join(dir, path)).isFile(),
  )
}

function readFlatJson(path: string): Record<string, unknown> {
  try {
    const parsed: unknown = JSON.parse(readFileSync(path, 'utf8'))
    return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : {}
  } catch {
    return {}
  }
}

/**
 * macOS hits: each English string in `en/macOS/` joined by key to the same file
 * in `folder/macOS/`. A key the target lacks, or whose value is still English
 * (the Siri-intent files), isn't evidence and is skipped.
 */
export function macosHits(pileRootDir: string, folder: string, query: PileQuery): PileHit[] {
  const englishDir = join(pileRootDir, 'en', 'macOS')
  const targetDir = join(pileRootDir, folder, 'macOS')
  const hits: PileHit[] = []
  for (const file of jsonFiles(englishDir)) {
    const targetPath = join(targetDir, file)
    if (!existsSync(targetPath)) continue
    const english = readFlatJson(join(englishDir, file))
    const target = readFlatJson(targetPath)
    for (const [key, englishValue] of Object.entries(english)) {
      const targetValue = target[key]
      if (typeof englishValue !== 'string' || typeof targetValue !== 'string' || targetValue === englishValue) continue
      const kind = matchKind(query.side === 'english' ? englishValue : targetValue, query.text)
      if (kind === undefined) continue
      hits.push({
        source: 'macos',
        file: relative(pileRootDir, targetPath),
        english: englishValue,
        target: targetValue,
        detail: key,
        exact: kind === 'exact',
      })
    }
  }
  return hits
}

const XML_ENTITIES: Readonly<Record<string, string>> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" }

function decodeXml(text: string): string {
  return text.replace(/&(amp|lt|gt|quot|apos);/g, (_match, name: string) => XML_ENTITIES[name])
}

/** One `<termEntry>`, read: the English side's definition and terms, and each target term with its region. */
export interface TbxEntry {
  definition?: string
  english: string[]
  targets: { term: string; region?: string }[]
}

/** Parses the `<termEntry>` blocks of a TBX file's text. */
export function parseTbx(text: string): TbxEntry[] {
  const entries: TbxEntry[] = []
  for (const block of text.split('<termEntry').slice(1)) {
    const langSets = block.split('<langSet').slice(1)
    const entry: TbxEntry = { english: [], targets: [] }
    for (const langSet of langSets) {
      const isEnglish = /^\s+xml:lang="en(?:-US)?"/.test(langSet)
      if (isEnglish) {
        const definition = /<descrip type="definition">([\s\S]*?)<\/descrip>/.exec(langSet)?.[1]
        if (definition !== undefined) entry.definition = decodeXml(definition.trim())
      }
      for (const group of langSet.split('<termGrp>').slice(1)) {
        const term = /<term[^>]*>([\s\S]*?)<\/term>/.exec(group)?.[1]
        if (term === undefined) continue
        if (isEnglish) entry.english.push(decodeXml(term))
        else {
          const region = /<termNote type="geographicalUsage">([\s\S]*?)<\/termNote>/.exec(group)?.[1]
          entry.targets.push({ term: decodeXml(term), region })
        }
      }
    }
    entries.push(entry)
  }
  return entries
}

/** Microsoft hits: each TBX entry whose English (or, reversed, target) term matches, one per sense. */
export function microsoftHits(pileRootDir: string, folder: string, query: PileQuery): PileHit[] {
  const dir = join(pileRootDir, folder, 'microsoft-terminology')
  if (!existsSync(dir)) return []
  const hits: PileHit[] = []
  for (const name of readdirSync(dir).filter((file) => file.endsWith('.tbx'))) {
    const path = join(dir, name)
    for (const entry of parseTbx(readFileSync(path, 'utf8'))) {
      const sides = query.side === 'english' ? entry.english : entry.targets.map((target) => target.term)
      const kinds = sides.map((side) => matchKind(side, query.text))
      if (!kinds.includes('exact') && !kinds.includes('contains')) continue
      const targets = entry.targets.map(({ term, region }) => (region ? `${term} (${region})` : term)).join(' / ')
      hits.push({
        source: 'microsoft',
        file: relative(pileRootDir, path),
        english: entry.english.join(' / '),
        target: targets,
        detail: entry.definition,
        exact: kinds.includes('exact'),
      })
    }
  }
  return hits
}

/** One gettext entry: its context, English, and the first translation (singular). */
export interface PoEntry {
  context?: string
  msgid: string
  msgstr: string
}

/** Reads one quoted gettext string continuation line, unescaped. */
function poString(line: string): string {
  const match = /^"(.*)"\s*$/.exec(line.trim())
  if (!match) return ''
  return match[1].replace(/\\(["\\nt])/g, (_m, ch: string) => ({ n: '\n', t: '\t', '"': '"', '\\': '\\' })[ch] ?? ch)
}

type PoField = 'context' | 'msgid' | 'msgstr'

/** The field a gettext keyword fills, or `undefined` for one this reader skips (plurals beyond the first). */
function poField(keyword: string): PoField | undefined {
  if (keyword === 'msgctxt') return 'context'
  if (keyword === 'msgid') return 'msgid'
  return keyword === 'msgstr' || keyword === 'msgstr[0]' ? 'msgstr' : undefined
}

/** Parses a gettext catalog's text into its translated entries (the header and untranslated ones dropped). */
export function parsePo(text: string): PoEntry[] {
  const entries: PoEntry[] = []
  let current: Partial<Record<PoField, string>> = {}
  let field: PoField | undefined
  const flush = () => {
    if (current.msgid && current.msgstr) {
      entries.push({ context: current.context, msgid: current.msgid, msgstr: current.msgstr })
    }
    current = {}
    field = undefined
  }
  for (const line of text.split('\n')) {
    const head = /^(msgctxt|msgid_plural|msgid|msgstr(?:\[\d+\])?)\s+(".*")\s*$/.exec(line)
    if (head) {
      // A new entry starts at its context or its msgid, whichever comes first.
      if (head[1] === 'msgctxt' || (head[1] === 'msgid' && current.msgstr !== undefined)) flush()
      field = poField(head[1])
      if (field) current[field] = poString(head[2])
    } else if (line.startsWith('"') && field) current[field] = (current[field] ?? '') + poString(line)
    else if (line.trim() === '') flush()
  }
  flush()
  return entries
}

/** Gettext hits for one source: each entry whose `msgid` (or, reversed, `msgstr`) matches. */
export function gettextHits(
  pileRootDir: string,
  folder: string,
  source: 'linux' | 'orthodox' | 'git',
  query: PileQuery,
): PileHit[] {
  const hits: PileHit[] = []
  for (const sub of GETTEXT[source]) {
    const dir = join(pileRootDir, folder, sub)
    if (!existsSync(dir)) continue
    for (const name of readdirSync(dir).filter((file) => file.endsWith('.po'))) {
      const path = join(dir, name)
      for (const entry of parsePo(readFileSync(path, 'utf8'))) {
        const kind = matchKind(query.side === 'english' ? entry.msgid : entry.msgstr, query.text)
        if (kind === undefined) continue
        hits.push({
          source,
          file: relative(pileRootDir, path),
          english: entry.msgid,
          target: entry.msgstr,
          detail: entry.context,
          exact: kind === 'exact',
        })
      }
    }
  }
  return hits
}

/** Total Commander hits, reverse only: `ID="value"` lines of `WCMD.LNG.utf8` / `WCMD.INC.utf8` whose value matches. */
export function totalCommanderHits(pileRootDir: string, folder: string, query: PileQuery): PileHit[] {
  if (query.side !== 'target') return []
  const dir = join(pileRootDir, folder, 'total-commander')
  if (!existsSync(dir)) return []
  const hits: PileHit[] = []
  for (const name of readdirSync(dir).filter((file) => file.endsWith('.utf8'))) {
    const path = join(dir, name)
    for (const line of readFileSync(path, 'utf8').split('\n')) {
      const value = /^\s*(\d+)\s*=\s*"?(.*?)"?\s*$/.exec(line) ?? /^\s*(?:MENUITEM|POPUP)\s+"(.*?)"/.exec(line)
      const text = value?.[2] ?? value?.[1]
      if (text === undefined) continue
      const kind = matchKind(text, query.text)
      if (kind === undefined) continue
      hits.push({
        source: 'orthodox',
        file: relative(pileRootDir, path),
        english: '',
        target: line.trim(),
        exact: kind === 'exact',
      })
    }
  }
  return hits
}

/** Every hit for a query across a language's pile folders and the chosen sources, exact ones first. */
export function lookUp({
  pileRootDir,
  folders,
  query,
  sources = PILE_SOURCES,
}: {
  pileRootDir: string
  folders: readonly string[]
  query: PileQuery
  sources?: readonly PileSource[]
}): PileHit[] {
  const hits: PileHit[] = []
  for (const folder of folders) {
    if (sources.includes('macos')) hits.push(...macosHits(pileRootDir, folder, query))
    if (sources.includes('microsoft')) hits.push(...microsoftHits(pileRootDir, folder, query))
    for (const source of ['linux', 'orthodox', 'git'] as const) {
      if (sources.includes(source)) hits.push(...gettextHits(pileRootDir, folder, source, query))
    }
    if (sources.includes('orthodox')) hits.push(...totalCommanderHits(pileRootDir, folder, query))
  }
  return hits.sort(
    (a, b) => PILE_SOURCES.indexOf(a.source) - PILE_SOURCES.indexOf(b.source) || Number(b.exact) - Number(a.exact),
  )
}

/** The pile folders a language has, of those it mines; a missing one is reported, not guessed around. */
export function presentFolders(
  pileRootDir: string,
  folders: readonly string[],
): { present: string[]; missing: string[] } {
  const present = folders.filter((folder) => existsSync(join(pileRootDir, folder)))
  return { present, missing: folders.filter((folder) => !present.includes(folder)) }
}
