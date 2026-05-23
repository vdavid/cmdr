/**
 * Pure matcher for the Selection dialog.
 *
 * The dialog passes a `getNameFor(index)` accessor; the matcher iterates indices
 * `0..total` and returns the matched ones. The accessor abstracts over regular vs
 * `search-results://` snapshot panes: regular panes pass `entry.name` (basename),
 * snapshot panes also pass `entry.name` (which IS the friendly full path on
 * adapted entries per `lib/file-explorer/CLAUDE.md` § "Search-results virtual
 * volume"). The matcher itself doesn't care which kind it's running against.
 *
 * Size and date predicates apply when the consumer provides per-index `getSizeFor` /
 * `getMtimeFor` accessors plus a filter half on the query. They're optional: a
 * query with only a `pattern` runs purely as a name match.
 *
 * Empty pattern → returns `[]`. Bad regex (SyntaxError) → returns `[]`. The
 * frontend dialog handles caveats; the matcher's contract is "matches or
 * nothing".
 */

export interface SizePredicate {
  kind: 'gte' | 'lte' | 'between'
  /** Bytes (inclusive lower bound). */
  min?: number
  /** Bytes (inclusive upper bound). */
  max?: number
}

export interface DatePredicate {
  kind: 'after' | 'before' | 'between'
  /** Unix seconds (inclusive lower bound). */
  after?: number
  /** Unix seconds (inclusive upper bound). */
  before?: number
}

export interface SelectionMatchQuery {
  pattern: string
  kind: 'glob' | 'regex'
  caseSensitive: boolean
  size?: SizePredicate
  date?: DatePredicate
}

export interface MatchAccessors {
  /** Returns the name the matcher should test (basename for regular panes, full friendly path for snapshot panes). */
  getNameFor: (index: number) => string
  /** Optional: size in bytes, or `null`/`undefined` when the entry has no size (directories). */
  getSizeFor?: (index: number) => number | null | undefined
  /** Optional: modified-at timestamp in unix seconds. */
  getMtimeFor?: (index: number) => number | null | undefined
}

/**
 * Translates a glob string into an anchored RegExp. Mirrors the Rust
 * `glob_to_regex` in `src-tauri/src/search/query.rs`: `*` → `.*`, `?` → `.`,
 * everything else literal (regex metacharacters escaped). Anchored with `^…$`
 * for full-name matching.
 */
function globToRegex(glob: string, caseSensitive: boolean): RegExp {
  let pattern = '^'
  for (const c of glob) {
    if (c === '*') {
      pattern += '.*'
    } else if (c === '?') {
      pattern += '.'
    } else if ('.+()[]{}^$|\\'.includes(c)) {
      pattern += `\\${c}`
    } else {
      pattern += c
    }
  }
  pattern += '$'
  return new RegExp(pattern, caseSensitive ? '' : 'i')
}

/**
 * Compiles a query's pattern. Returns `null` when the pattern is empty or
 * malformed (so the caller short-circuits to `[]`).
 */
function compilePattern(query: SelectionMatchQuery): RegExp | null {
  const trimmed = query.pattern.trim()
  if (!trimmed) return null
  try {
    if (query.kind === 'regex') {
      return new RegExp(trimmed, query.caseSensitive ? '' : 'i')
    }
    return globToRegex(trimmed, query.caseSensitive)
  } catch {
    // SyntaxError on malformed regex → no matches.
    return null
  }
}

function sizePredicateMatches(value: number | null | undefined, pred: SizePredicate): boolean {
  if (value == null) return false
  if (pred.kind === 'gte') return pred.min != null && value >= pred.min
  if (pred.kind === 'lte') return pred.max != null && value <= pred.max
  // between
  if (pred.min != null && value < pred.min) return false
  if (pred.max != null && value > pred.max) return false
  return true
}

function datePredicateMatches(value: number | null | undefined, pred: DatePredicate): boolean {
  if (value == null) return false
  if (pred.kind === 'after') return pred.after != null && value >= pred.after
  if (pred.kind === 'before') return pred.before != null && value <= pred.before
  // between
  if (pred.after != null && value < pred.after) return false
  if (pred.before != null && value > pred.before) return false
  return true
}

/**
 * Iterates `0..total`, returns indices whose entry matches the query. Pattern,
 * size, and date predicates compose with AND semantics. Empty / malformed
 * pattern → `[]`. Predicates without their accessor → ignored (size filter on
 * a folder with no `getSizeFor` skips the size check).
 */
export function matchEntries(accessors: MatchAccessors, total: number, query: SelectionMatchQuery): number[] {
  const re = compilePattern(query)
  if (!re) return []

  const out: number[] = []
  for (let i = 0; i < total; i++) {
    const name = accessors.getNameFor(i)
    if (!re.test(name)) continue
    if (query.size && accessors.getSizeFor) {
      if (!sizePredicateMatches(accessors.getSizeFor(i), query.size)) continue
    }
    if (query.date && accessors.getMtimeFor) {
      if (!datePredicateMatches(accessors.getMtimeFor(i), query.date)) continue
    }
    out.push(i)
  }
  return out
}
