/**
 * uFuzzy's `intraMode` values, typed as the library's own enum.
 *
 * The library declares `IntraMode` as an ambient `const enum`: there's no runtime object to read,
 * and `verbatimModuleSyntax` refuses `uFuzzy.IntraMode.SingleError` as a value (TS2748). A bare
 * `1`, or `1 as IntraMode`, fails `@typescript-eslint/no-unsafe-enum-assignment`. So the runtime
 * value sits in `ufuzzy-modes.js` and this declaration gives it the enum member's type, the shape a
 * library with a real runtime enum would ship.
 */
import type uFuzzy from '@leeoniya/ufuzzy'

/** One substitution, transposition, insertion, or deletion per term: catches typos like "tyoe" → "type". */
export const INTRA_MODE_SINGLE_ERROR: uFuzzy.IntraMode.SingleError
