#!/usr/bin/env node
/**
 * Regenerates the COMMITTED i18n pseudolocale test fixture: a tiny, hand-verifiable
 * `en/` + `en-XA/` catalog pair that the M2 (stale) and M3 (parity / ICU / plural /
 * key) check tests run against. It's separate from the full generated
 * `src/lib/intl/messages/en-XA/` (which is gitignored + regenerable from the real
 * `en` catalog and churns on every copy edit) so the checks have a stable, small,
 * eyeball-able input that doesn't move when product copy changes.
 *
 * The fixture is BUILT from the generator (`buildPseudoFile`), never hand-typed, so
 * its `en-XA` side is always exactly what the generator produces, and committing it
 * also pins the generator's output shape. The curated `en` keys deliberately cover
 * every shape a check must handle: a plain label, a `{placeholder}`, a `<tag>`, a
 * `plural`, a `select`, a multi-placeholder sentence, and a RAW `errors.*` key
 * (non-ICU path).
 *
 * Run after changing the generator's transform or this curated set:
 *   node apps/desktop/test/fixtures/i18n-pseudolocale/gen-fixture.js
 * Then commit the rewritten `en/fixture.json` + `en-XA/fixture.json`.
 */

import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { buildPseudoFile } from '../../../scripts/gen-pseudolocale.js'

/**
 * The curated English source. Apostrophes here are in `@key` metadata
 * (descriptions), which is NOT ICU, so they're written normally (no `''`).
 * @type {Record<string, unknown>}
 */
const en = {
  // Plain label: no placeholders, tags, or ICU structure.
  'fixture.plainLabel': 'Cancel',
  '@fixture.plainLabel': { description: 'A plain cancel button label.' },

  // Single {placeholder}.
  'fixture.greeting': 'Welcome back, {name}',
  '@fixture.greeting': { description: 'Greeting shown on launch; {name} is the display name.' },

  // <tag> wrapping a placeholder (inline interactive component).
  'fixture.openSettings': 'Open <link>{label}</link> to continue',
  '@fixture.openSettings': { description: 'Inline action; <link> wraps the clickable label.' },

  // ICU plural with a # pound.
  'fixture.fileCount': '{count, plural, one {# file} other {# files}}',
  '@fixture.fileCount': { description: 'File count; {count} drives plural selection.' },

  // ICU select.
  'fixture.paneSide': '{side, select, left {Left pane} other {Right pane}}',
  '@fixture.paneSide': { description: 'Names the focused pane.' },

  // Multi-placeholder sentence (the *Text params are preformatted counts).
  'fixture.transferSummary': 'Copied {fileText} from {source} to {target}',
  '@fixture.transferSummary': { description: 'Transfer summary; fileText is a preformatted count.' },

  // RAW error key (non-ICU path): {system_settings} is a substituted token,
  // <folder-path> is LITERAL text, the backticks/apostrophe are markdown.
  'errors.fixture.suggestion': "Open {system_settings}, then run `lsof <folder-path>`. Here's why.",
  '@errors.fixture.suggestion': {
    description: 'Raw error suggestion; {system_settings} is a substituted system label, <folder-path> is literal.',
  },

  // Brand/system WORDS (Cmdr, macOS) that must survive translation verbatim. The
  // generator keeps them un-accented, so this also pins that behavior and gives
  // the don't-translate check a clean fixture baseline.
  'fixture.brandLine': 'Cmdr needs Full Disk Access on macOS to continue',
  '@fixture.brandLine': { description: 'Brand line; Cmdr and macOS must not be translated.' },
}

const enXa = buildPseudoFile(en)

const base = import.meta.dirname
mkdirSync(join(base, 'en'), { recursive: true })
mkdirSync(join(base, 'en-XA'), { recursive: true })
writeFileSync(join(base, 'en', 'fixture.json'), JSON.stringify(en, null, 2) + '\n', 'utf8')
writeFileSync(join(base, 'en-XA', 'fixture.json'), JSON.stringify(enXa, null, 2) + '\n', 'utf8')
console.log(
  `Wrote i18n-pseudolocale fixture (${String(Object.keys(en).filter((k) => !k.startsWith('@')).length)} keys).`,
)
