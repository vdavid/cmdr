// uFuzzy's `intraMode` values at runtime. The types live in `ufuzzy-modes.d.ts`, which says why.

/** One substitution, transposition, insertion, or deletion per term: catches typos like "tyoe" → "type". */
export const INTRA_MODE_SINGLE_ERROR = 1

/** Insertions only, never a substitution; with `intraIns: 0`, each term must appear exactly as typed. */
export const INTRA_MODE_MULTI_INSERT = 0
