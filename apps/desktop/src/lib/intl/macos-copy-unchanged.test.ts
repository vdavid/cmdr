/**
 * macOS copy doesn't move: every shipped locale's macOS rendering of every
 * platform-copy key stays what `__fixtures__/macos-copy-baseline.json` froze.
 *
 * The baseline holds the RESOLVED value a Mac reader saw (locale, then the
 * catalogs it inherits from, then `en`), read from disk for all 17 catalogs, since
 * the runtime only loads `en` and one chain outside `test-setup.ts`. Its keys are
 * the platform-copy rows of `docs/notes/windows-port/copy-proposal.json`; its
 * writer is `scripts/i18n-macos-copy-baseline.ts`, which refuses to overwrite it.
 * ❌ Never edit or regenerate the baseline to make this pass: a red here means a
 * catalog edit changed what a Mac shows.
 *
 * The comparison (`macosCopyDrift`) lives beside the baseline reader, in
 * `scripts/i18n-macos-copy-baseline-lib.ts`.
 *
 * Allowed differences, and only these:
 *  - `ACCEPTED_ENGLISH_CHANGES`: the English values David changed on purpose. Only
 *    where the value resolves from `en`; a translation of each stays put.
 *  - `RETIRED_KEYS`: a frozen key David deleted on purpose, which must then be gone
 *    from every catalog.
 *  - `COLLAPSED_PAIRS`: a `.mac`/`.other` key pair folded into one block-valued
 *    survivor key. Once the `.mac` key is gone, the survivor's macOS branch must
 *    equal the old `.mac` baseline value, and the two old keys are no longer read.
 *  - `ACCEPTED_TRANSLATION_CHANGES`: a translator's reviewed fix of a wrong frozen
 *    translation, keyed by catalog and key with the new macOS text and a reason.
 *    The catalog must then read exactly that text.
 *  - `SPLIT_KEYS`: the reverse of a collapsed pair, a new key split off a frozen one for one case it
 *    used to word. The new key must exist, and its macOS branch must render the old
 *    key's frozen value (in every locale, so a Mac keeps its translation).
 *
 * ICU values compare ASTs (same rendering, whatever the spelling); the raw families
 * (`isRawKey`) compare text.
 */
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

import {
  BASELINE_RELATIVE_PATH,
  desktopPath,
  macosCopyDrift,
  type MacosCopyBaseline,
} from '../../../scripts/i18n-macos-copy-baseline-lib.ts'

const baseline = JSON.parse(readFileSync(desktopPath(BASELINE_RELATIVE_PATH), 'utf8')) as MacosCopyBaseline

describe('macOS copy stays byte-identical', () => {
  it('covers every shipped locale and the platform-copy keys', () => {
    expect(Object.keys(baseline.locales).length).toBe(17)
    expect(Object.keys(baseline.locales.en).length).toBeGreaterThan(400)
  })

  it('every locale renders the frozen macOS text for every key', () => {
    expect(macosCopyDrift(baseline)).toEqual([])
  })
})
