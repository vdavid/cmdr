# Message catalog details

Depth behind [`CLAUDE.md`](CLAUDE.md). How catalogs are authored, the `@key` metadata schema, and the parity contract.
The runtime that consumes these files (resolution, fallback, ICU, the error-pipeline boundary, the formatting split):
[`../DETAILS.md`](../DETAILS.md).

## Why per-area-central files

JSON, per feature area, under `en/`, plus `common.json` for genuinely shared strings. This is the chosen middle between
one giant file and fully-colocated fragments: clean diffs, an agent editing one feature touches one file, and a future
translate-a-locale job globs ~12 predictable files. Key prefix ↔ filename is 1:1, so the area in a key
(`settings.fsWatch.title`) IS its catalog home (`settings.json`) AND its scope. The same English word can diverge per
window just by getting its own area key; shared strings live in `common.*`, and the moment one site needs different copy
it gets its own area key (never a positional "window" argument).

The catalog areas are a closed set, enforced by `desktop-message-key-naming`'s `messageKeyKnownAreas`, and they mirror
the `lib/` feature directories (lowerCamel): `common`, `transfer`, `settings`, `errors`, `fileExplorer`,
`fileOperations`, `queryUi`, `search`, `viewer`, `onboarding`, `licensing`, `downloads`, `ai`, `goToPath`, `mtp`, `ui`,
`updates`, `whatsNew`, `commandPalette`, `commands`, `shortcuts`, `crashReporter`, `errorReporter`, `feedback`, `menu`,
`indexing`, `lowDiskSpace`, `notifications`, `main`. Adding one means adding both the `<area>.json` file and the area to
that check.

## Message value format

A value is either a plain ICU string or, for plurals/selects, an ICU string with `{count, plural, …}` /
`{type, select, …}` inline (NOT a `{one, other}` object, since the engine parses the inline form). Examples live in
`en/transfer.json` (the hardest multi-variable case: a `kind` discriminator `select` wrapping independent `plural`
branches, with preformatted `*Text` count strings for display and raw integers for plural selection).

Rich-text (inline-component) messages use `<tag>…</tag>` markers that `<Trans>` maps to Svelte snippets, e.g.
`en/common.json`'s `common.downloadsFdaHint`: `"… <settingsLink>Open System Settings</settingsLink>"`. No `{@html}`;
text is text, components are components.

## ICU apostrophe rule

ICU MessageFormat treats `'` as an escape character. A lone `'` is literal UNLESS it immediately precedes a special char
(`{`, `<`, `#`), where it opens a quoted section that swallows following text until the next `'`. `''` always collapses
to a single `'`. (Verified on `intl-messageformat@11.2.7`, by reading the parser behavior, 2026-06-16.)

Cmdr copy is full of apostrophes ("doesn't", "can't", "you're", "already at the target"). The rule is to double EVERY
apostrophe in a catalog value, not only the dangerous ones: `''` is always safe, and a blanket rule survives future copy
edits that might move an apostrophe next to a placeholder. The per-area parity test is the net that catches a missed
double.

## ⚠️ `errors.*` are RAW (no ICU): translators must NOT add ICU syntax there

The entire `errors.*` family (`errors.listing.*`, `errors.git.*`, `errors.provider.*`, `errors.write.*`) does NOT render
through ICU. It resolves via `getMessage()` (a raw catalog lookup), then `interpolate()` + `expandSystemStrings()` do
plain `.replaceAll('{token}', value)` substitution (see [`../../errors/CLAUDE.md`](../../errors/CLAUDE.md) and the intl
runtime [`../CLAUDE.md`](../CLAUDE.md)). The apostrophe-doubling rule above is the OPPOSITE here. So in any `errors.*`
value:

- **Do NOT double apostrophes.** Write `doesn't`, not `doesn''t`: there's no ICU parser to un-double them, so `''` would
  render as a literal double apostrophe.
- **`{token}` is a literal replacement target, not an ICU argument.** `{system_settings}`, `{path}`, `{reason}`, etc.
  are substituted by name. Keep them verbatim; don't reorder their braces or add ICU formatting (`{count, number}`,
  `{x, plural, …}`): none of that is parsed, it'd render as literal text.
- **`<…>` is LITERAL text, not a tag.** `<folder-path>` in an `errors.*` value prints literally; it is NOT an inline
  component. Don't treat `<x>` as ICU/HTML here.
- **Markdown is literal.** `#`, bold markers, and backticks pass through untouched (the raw pipeline doesn't strip
  them).

The unit on which this raw/ICU split is decided is the KEY PREFIX (`errors.`), single-sourced as `isRawKey()` in
[`../../../../scripts/i18n-catalog-lib.js`](../../../../scripts/i18n-catalog-lib.js). The locale checks honor it: the
ICU-validity check (`desktop-i18n-icu`) SKIPS `errors.*` (so valid raw copy isn't flagged as invalid ICU), and the
parity check (`desktop-i18n-parity`) compares the raw `{token}` set instead of an ICU placeholder set for these keys.
Translator-facing version of this note: [`/docs/guides/i18n.md`](../../../../../../docs/guides/i18n.md) § Error
pipeline.

## `@key` metadata schema

Each message key MAY have a sibling `@`-prefixed entry holding ARB-style metadata, stripped before the runtime or
codegen ever sees it:

```jsonc
{
  "transfer.trash": "Moved {countText} {count, plural, one {file} other {files}} to trash",
  "@transfer.trash": {
    "description": "Toast confirming items were moved to the macOS Trash. Shown briefly after a delete-to-trash (F8).",
    "placeholders": {
      "countText": "how many files, already formatted for display (e.g. 1,234)",
      "count": "the same number of files (drives the singular/plural form of the noun)",
    },
    "screenshot": "transfer-complete-toast.png",
  },
}
```

- `description`: a free-form, translator-facing note. Optimize it to set a translator (human or agent) up to do
  EXCELLENT work without seeing the running app. Cover what's string-SPECIFIC: (1) where and when it appears (the
  surface and the trigger: "status-bar toast after a copy", "label in the Settings > Appearance section", "button in the
  delete-confirm dialog"); (2) the tone or intent if it's not obvious (reassuring, a warning, a terse control label);
  (3) any constraint that shapes the translation (a tight button/column that can't grow much, a term that must match a
  sibling string, a literal token that must NOT be translated, such as brand names like "Finder"/"GitHub", format tokens
  like `YYYY`/`MM`, shortcut glyphs). Keep it natural language, not a rigid schema. Do NOT explain the ICU plumbing (the
  translator already knows to preserve placeholder/`plural`/`select` syntax and apply their language's plural
  categories: that's a one-time instruction in the translator-agent prompt, not per-string noise). Two cases the
  description MUST cover (the audit's top blind-translation risks):
  - **A pass-through placeholder** (`{message}`, `{reason}`, a raw `{path}`, or any value Cmdr doesn't control, such as
    an OS error string or an arbitrary file path): the description MUST say the inserted value is uncontrolled, so the
    translator structures the sentence to tolerate any length, casing, gender, or number.
  - **A fragment / concatenation key** (a sentence part assembled at runtime, e.g. `*Part` keys): the description MUST
    name the `*Join` key that assembles it (today `fileOperations.shared.andJoin`), so the translator knows word order
    is owned by the join key and translates the fragment to read naturally once joined.
- `placeholders`: an ARB-style map giving each placeholder a PLAIN-LANGUAGE meaning plus an example value, in the
  translator's terms ("number of files", "the folder name"), never the ICU mechanics ("raw integer for plural
  selection"). Include it whenever a message has placeholders; omit it for static strings. This is what lets a
  translator reorder placeholders correctly for their grammar.
- `screenshot`: a filename in `screenshots/` showing the string in context. One screenshot may serve many keys; many
  keys can name the same file. A screenshot REPLACES the need to describe layout in prose. It's populated by the capture
  harness, not by hand (see Screenshots below); it's an optional aid, so a key without one is fine.
- `screenshotNote`: a translator-facing note, present ONLY on a REPRESENTATIVE coupling (see Screenshots below), that
  explains how a stand-in screenshot maps to this key ("this shows a different error, but your string is the
  title/explanation in this same pane"). Absent on direct (captured) couplings. Like `screenshot`, it's harness-written,
  never hand-authored, and stripped before runtime/codegen.
- `sourceHash` (non-`en` locales only): a 7-char lowercase hex hash (git-style; the SHA-256 prefix of the EXACT English
  value the translation was made from), computed by `sourceHash()` in `apps/desktop/scripts/i18n-catalog-lib.js`. The
  pseudolocale generator and any locale skeleton write it; the `desktop-i18n-stale` check compares the stored hash
  against the current English value's hash and flags a translation whose source has since changed as STALE.
  Deterministic and git-independent (survives rebases/reformats); not present in `en` (the source has no source).
  **Release-strict gate:** the stale check is warn-only in normal `pnpm check` (a maintenance signal, not a daily-dev
  build breaker), but at release time it escalates a stale finding to a build-failing ERROR. The release flow
  (`scripts/release.sh`) sets `CMDR_I18N_STALE_STRICT=1` before its `pnpm check i18n-stale`, so a release can NOT ship a
  stale translation: the fix lands first. English-only today, so it's a clean no-op until a real locale exists. The gate
  fires locally in `scripts/release.sh`, not a GitHub workflow; see
  [`/docs/guides/releasing.md`](../../../../../../docs/guides/releasing.md).
- `reviewed` (non-`en` locales only): an OPTIONAL boolean human sign-off (principle 6: a human reviewed this translated
  copy). Reset to absent/`false` by a human when the stale check reports that `sourceHash` changed, because a
  re-translation needs a fresh review. NOT a gate: no check requires `reviewed: true` to pass. The stale check only
  REPORTS that a stale key's prior sign-off no longer applies; a missing or `false` flag never fails anything. Likely
  lightly used; it exists so the review state has a home if David wants it.

- `sameAsSourceJustification` (non-`en` locales only): an OPTIONAL non-empty string recording WHY a value that is
  byte-identical to English is deliberately identical in this locale, not a missed translation — a brand name
  (`Dropbox`), a unit symbol (`GB`), a placeholder-only string (`{width} × {height}`), or a word the locale genuinely
  shares with English (German `Server`, Swedish `Smart`). Present and non-empty → the `desktop-i18n-coverage` check
  stops flagging that key as "possibly untranslated" (see `i18n-check-coverage.js`). It is a per-LOCALE judgment (German
  keeps `Server`; Spanish translates it to `Servidor`), so it lives in the locale catalog, never in `en`, and is
  repeated per locale even for universal brands (the repetition is accepted: each translator vouches for each identical
  key in their language). It only suppresses the IDENTICAL signal — a MISSING key still reports. Tie it to the source
  like `reviewed`: the stale check flags a stale key that still carries it, because a justification vouched for the OLD
  English value must be re-confirmed once the source changes. Write it as the translator's reason, sourced where the
  term came from (e.g. "brand name; do not translate" or "macOS Swedish Finder uses 'Smart'"). Full translator workflow:
  [`/docs/guides/i18n-translation.md`](../../../../../../docs/guides/i18n-translation.md) § Deliberately-identical
  strings.

`sourceHash`, `reviewed`, and `sameAsSourceJustification` are ordinary `@`-metadata fields: the whole `@`-entry is
stripped before the runtime and codegen see it (same as `description`/`screenshot`), so adding them needs NO codegen or
runtime change.

The guiding test for every `@key`: "Could a competent translator who has never run Cmdr render this perfectly into any
language, given this note plus a per-language style guide plus (optionally) the screenshot?" If not, the note is missing
context, a placeholder meaning, or a constraint. Two non-negotiables a new key can't ship without: a
pass-through-placeholder key MUST state the inserted value is uncontrolled, and a fragment key MUST name its `*Join`
assembler (both spelled out in the `description` bullet above). Tone/voice/formality are NOT per-string: they live in
the per-language style guide, so don't repeat them on every key.

Keep the `@key` twin's name in lockstep with its message key on a rename. `desktop-message-key-naming` validates the
twin too (it strips the leading `@` and checks the underlying key), so a metadata entry for a misnamed key also fails.

## Screenshots (capture harness)

`@key.screenshot` values are populated by a re-runnable harness, never hand-authored. **`pnpm i18n:shots`** is the one
command: it captures fresh screenshots, then rewrites every `@key.screenshot`. (Under the hood it's
`i18n:capture --build` then `i18n:couple`; the orchestrator is `apps/desktop/scripts/i18n-capture.js`, the coupler is
`couple-screenshots.js`, each carrying a full header comment. Conceptual overview + the prod-no-op define:
[`/docs/guides/i18n.md`](../../../../../../docs/guides/i18n.md) § Screenshots.)

What's where, and what's tracked:

- `screenshots/*.png`: the captured images. **Gitignored and regenerable** (`screenshots/.gitignore`): they're large and
  re-render byte-for-byte on every run. A checkout without them degrades gracefully (the `@key` descriptions are the
  primary translator aid). Don't commit PNGs.
- `screenshots/capture-report.json`: **tracked.** The surface → keys map the driver records; the coupler's input and the
  freshness check's source of truth.
- `screenshots/coverage-report.md`: **tracked**, regenerated by the coupler. Per area: how many keys are **direct** (own
  captured screenshot) vs **representative** (a stand-in, see below) vs **uncoupled** (no screenshot yet). Direct and
  representative are counted separately so a stand-in is never mistaken for a precise capture. Coverage is partial until
  the driver covers the full inventory.
- The `@key.screenshot` / `@key.screenshotNote` refs in `en/*.json`: **tracked.** Written line-surgically (only those
  two fields change; the coupler has a value-safety test covering both).

### Direct vs representative couplings

The coupler writes screenshots in two passes:

- **Direct**: a key that rendered on a captured surface gets that surface's screenshot (no note). It assigns each key
  its FIRST surface in the report's order (surfaces ordered narrow-to-broad, so the most-specific surface wins).
- **Representative**: for a key STILL uncoupled after the direct pass that matches a curated prefix in
  `REPRESENTATIVE_SCREENSHOTS` (in `scripts/couple-screenshots.js`), the coupler writes a STAND-IN screenshot (a real
  capture of the same panel/toast/dialog where the string appears), plus a `@key.screenshotNote` explaining the mapping.
  This sets translators up for success on families with no captured surface of their own: the dynamic `errors.*` tail
  (mapped to one captured error pane, `error-message-example.png`), AI/cloud states, SMB/MTP connection states, the
  crash-report dialog, and the shortcuts window. Honesty rules baked into the mechanism: direct ALWAYS wins (a stand-in
  never overwrites a precise capture), a key that later gains its own capture sheds its representative note, and a
  representative only points at an image the run actually produced. Add a mapping only where the layout/position
  genuinely matches; otherwise leave the cluster uncoupled (it shows in the coverage report).

The coupler is idempotent (a re-run with the same report is a byte-for-byte no-op). The warn-only
`message-screenshots-fresh` check runs the coupler's `--check` to flag report↔catalog drift without needing the PNGs; it
never fails the build (screenshots are optional). Re-run `pnpm i18n:shots` to clear a drift warning.

## Parity contract

`en` is the base/source locale. Every migrated string's base-locale rendered output must equal the pre-migration English
(this is readiness, not a copy change). The per-area parity test asserts this (the transfer pilot's is
`../../file-operations/transfer/transfer-complete-toast.test.ts`, pinned to en-US). The `pluralize-noun` Go check scans
source, not catalog JSON, so ICU plurals inside catalogs aren't covered by it: their correctness is the parity test's
job.

## Dead-key honesty + the orphan check

Two layers look for catalog keys never referenced in code:

- The codegen prints a non-fatal dead-key WARNING. Its usage scan only sees STATICALLY-written keys (`t('literal')`,
  `<Trans key="literal">`), so a dynamically-built key reads as dead there.
- `desktop-message-keys-unused` (Go check, error-level) is the enforced net: a catalog key whose literal appears in NO
  `apps/desktop/src/` file (test files included) and isn't covered by an allowlisted dynamic prefix is a hard failure,
  because an orphan key is dead translation work that costs money once human translators are involved. It credits any
  indirection that stores the literal (registry `*Key` fields, Record maps), so the only keys it flags are genuinely
  absent ones.

Keys assembled at runtime never appear verbatim, so they're carried by a closed, documented dynamic-prefix allowlist
(`unusedKeyDynamicPrefixes` in `scripts/check/checks/desktop-message-keys-unused.go`, the single source). Today that's
the four error-factory prefixes (`errors.git.`, `errors.listing.`, `errors.provider.`, `errors.write.`), each tied to a
runtime construction site in `lib/errors/` (and `lib/file-operations/transfer/`); a prefix with no matching catalog key
fails the check as a stale entry. Add a prefix ONLY for a real runtime-construction site; never to silence a genuine
orphan.

`common.downloadsFdaHint` originated as the `<Trans>` proof and now has its real call site (the Downloads FDA hint in
`FileSystemWatchingSection.svelte`).

## Principle 6 note (humans review human-facing copy)

The base `en` catalog is a parity-protected MOVE of already-human-authored copy, so it's fine under principle 6. But
future agent-translated locales DO meet human eyes, so that later pipeline must include human review: "agents translate,
scripted pipeline" is not a license to ship unreviewed machine copy.
