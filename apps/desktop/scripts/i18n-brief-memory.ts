/**
 * The brief's translation memory: for each batch key, the nearest keys a target
 * locale has already shipped, so the translator sees how the same phrase or the
 * same dialog was translated. Also owns the content-word vocabulary the brief's
 * "No concept yet" line shares, so both agree on which words carry meaning.
 *
 * Pure and deterministic. Schemas and the why: `docs/i18n/termbase.md` § Tooling.
 */

import { englishMatchText } from './i18n-termbase-lib.ts'

/**
 * English function words: too common to say two strings are related, or to name
 * a missing concept. UI verbs (back, keep, make, try, use) stay OUT on purpose:
 * they're exactly the short labels a termbase has to rule on.
 */
const STOPWORDS = new Set(
  (
    'the and for you your yours this that these those with from are was were not but can will its has have had ' +
    'into our out all any also been being could did does doing each few here how just may might more most much ' +
    'must nor now off once only other over own same should some such than then there they them their what when ' +
    'where which while who whom why would yet about above after again against before below between both during ' +
    'further itself through very too isn aren don doesn didn can couldn won wouldn shouldn let lets one two'
  ).split(' '),
)

/**
 * Everyday English a file manager's copy uses without it being a term: fine as a
 * translation-memory signal, noise as a "No concept yet" candidate.
 */
const GENERIC_WORDS = new Set(
  (
    'about again almost already always another anything around away because become before being better case ' +
    'certain change come comes content could day days different differently done down either else enough entire ' +
    'even ever every everything find first follow found gets give given goes going gone good happen happening ' +
    'happens instead keeps kind know last later least less like likely little long look looks lot many mean means ' +
    'might moment need needs never next nothing often once otherwise part place please point possible probably ' +
    'quite rather ready really right says second see seems still sure take takes taken talks tell thing things think time ' +
    'times took turned under until usually want way ways well whether whole within without work working yet'
  ).split(' '),
)

/** Whether a content word is ordinary English rather than a candidate term. */
export function isGenericWord(word: string): boolean {
  return GENERIC_WORDS.has(word)
}

/** The content words of a key's English, in order: lowercase, three letters or more, no stopwords. */
export function contentWords(key: string, value: string): string[] {
  const words =
    englishMatchText(key, value)
      .toLowerCase()
      .match(/\p{L}+/gu) ?? []
  return [...new Set(words.filter((word) => word.length >= 3 && !STOPWORDS.has(word)))]
}

/** Per-key content words plus their document frequency and IDF, built once per run. */
export interface MemoryIndex {
  order: Map<string, number>
  words: Map<string, Set<string>>
  /** how many keys use each word: a term recurs, a one-off phrasing doesn't */
  df: Map<string, number>
  idf: Map<string, number>
}

/** Indexes the English catalog for `nearestKeys`. */
export function buildMemoryIndex(en: Record<string, string>): MemoryIndex {
  const order = new Map<string, number>()
  const words = new Map<string, Set<string>>()
  const df = new Map<string, number>()
  for (const [position, key] of Object.keys(en).entries()) {
    order.set(key, position)
    const set = new Set(contentWords(key, en[key]))
    words.set(key, set)
    for (const word of set) df.set(word, (df.get(word) ?? 0) + 1)
  }
  const total = Math.max(order.size, 1)
  const idf = new Map([...df].map(([word, count]) => [word, Math.log(1 + total / count)]))
  return { order, words, df, idf }
}

/** How two keys' content words overlap: the IDF-weighted cosine, and the raw counts the floor needs. */
interface Overlap {
  score: number
  shared: number
  otherSize: number
}

function overlap(index: MemoryIndex, key: string, other: string): Overlap {
  const mine = index.words.get(key) ?? new Set<string>()
  const theirs = index.words.get(other) ?? new Set<string>()
  const weight = (word: string) => index.idf.get(word) ?? 0
  let sharedWeight = 0
  let shared = 0
  for (const word of mine) {
    if (!theirs.has(word)) continue
    shared++
    sharedWeight += weight(word)
  }
  if (shared === 0) return { score: 0, shared, otherSize: theirs.size }
  const norm = (set: Set<string>) => [...set].reduce((sum, word) => sum + weight(word), 0)
  return { score: sharedWeight / Math.sqrt(norm(mine) * norm(theirs)), shared, otherSize: theirs.size }
}

/**
 * Whether a key from elsewhere in the catalog is related enough to show. One
 * shared word isn't: "Exit full screen" and "Error screen" share only "screen".
 * Two are, and so is a short label whose every word the key contains ("Overwrite"
 * for a sentence about overwriting), which is the same phrase on another surface.
 */
function relatedEnough({ shared, otherSize }: Overlap): boolean {
  return shared >= 2 || (shared > 0 && shared === otherSize)
}

/** The parent path of a key (`a.b.c` → `a.b`). */
const parentOf = (key: string) => key.slice(0, Math.max(key.lastIndexOf('.'), 0))

/**
 * A key's translation memory: the nearest keys that at least one target locale
 * has shipped, excluding the batch. Same-parent siblings come first (they share a
 * dialog, so they share its voice), capped at half the slots so a strong match
 * elsewhere in the catalog, often the same phrase on another surface, still gets
 * in. Siblings rank by word overlap, then by distance in the catalog; the rest must
 * pass `relatedEnough`, and an unfilled slot stays empty rather than padded.
 */
export function nearestKeys({
  key,
  en,
  targets,
  batch,
  count,
  index,
}: {
  key: string
  en: Record<string, string>
  targets: readonly Record<string, string>[]
  batch: ReadonlySet<string>
  count: number
  index?: MemoryIndex
}): string[] {
  const ix = index ?? buildMemoryIndex(en)
  const parent = parentOf(key)
  const position = ix.order.get(key) ?? 0
  const scored = Object.keys(en)
    .filter((other) => other !== key && !batch.has(other) && targets.some((target) => other in target))
    .map((other) => ({
      key: other,
      overlap: overlap(ix, key, other),
      distance: Math.abs((ix.order.get(other) ?? 0) - position),
      sibling: parentOf(other) === parent,
    }))
  const byScore = (a: (typeof scored)[number], b: (typeof scored)[number]) =>
    b.overlap.score - a.overlap.score || a.distance - b.distance || a.key.localeCompare(b.key)
  const siblings = scored.filter((entry) => entry.sibling).sort(byScore)
  const picked = siblings.slice(0, Math.ceil(count / 2)).map((entry) => entry.key)
  const rest = scored
    .filter((entry) => !entry.sibling && relatedEnough(entry.overlap))
    .sort(byScore)
    .map((entry) => entry.key)
  for (const other of [...rest, ...siblings.map((entry) => entry.key)]) {
    if (picked.length >= count) break
    if (!picked.includes(other)) picked.push(other)
  }
  return picked
}
