// uFuzzy's `intraMode` values at runtime. The types live in `ufuzzy-modes.d.ts`, which says why.

/** One substitution, transposition, insertion, or deletion per term: catches typos like "tyoe" → "type". */
export const INTRA_MODE_SINGLE_ERROR = 1
