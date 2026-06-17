# i18n translation-readiness & maintenance

Make the i18n system safe to translate into several languages and keep maintained, ahead of adding the first non-English
locales soon. English-only ships today; everything here must be built AND tested NOW (using a generated pseudolocale as
the test fixture) so it's ready the moment a real locale lands. The migration + screenshots are done; this is the
maintenance/tooling layer on top.

Read first: `docs/guides/i18n.md`, `apps/desktop/src/lib/intl/{CLAUDE,DETAILS}.md`,
`apps/desktop/src/lib/intl/messages/{CLAUDE,DETAILS}.md`. Audit learnings that shape this plan: the catalog is already
translation-ready (100% `@key.description` coverage, placeholders mapped); the biggest residual blind-translation risk
is pass-through placeholders (`{message}`/`{reason}`/raw `{path}`); screenshots are a booster, not required.

## The lynchpin: the pseudolocale is the universal test fixture

We have no non-English locale yet, so every check below would otherwise be untestable. A **pseudolocale** — a generated
locale that takes each current English value and produces a deterministic, accented, ~+40%-longer string while
PRESERVING every `{placeholder}`, `<tag>`, and ICU `plural`/`select` structure, and recording each key's source hash —
is a valid, loadable locale that MUST pass every check. It's simultaneously: the overflow-testing tool (drive the app in
it, screenshot, find clipping), the test data for the stale check (its hashes match → not stale; corrupt one → stale),
and the test data for the parity/ICU/plural/key checks (preserves structure → passes; negative tests corrupt it → fail).
Build it early (M1) so M2–M3 test against it.

## Decisions

### Locale format (#5) — confirmed correct, document it

- The locale VALUE is the full BCP-47 tag from `getLocale()` (`en-US`, `de-DE`, `pt-BR`); the formatter layer already
  uses it for numbers/sizes/dates. Don't change.
- Catalogs are keyed by BCP-47, with the **language-base** as the universal fallback: `en` (base) + optional region
  variants (`en-GB`, `pt-BR`) holding only overrides or a full set. `en` is today's base and the final fallback.
- **Resolution** (build when the first locale lands; today it's hardcoded to `en`): OS locale `xx-YY` → try catalog
  `xx-YY` → try `xx` → fall back to `en`. British English = add an `en-GB` catalog; the resolver prefers it over `en`.
- No course-correction to the current structure. This plan documents the convention; the resolver itself is a small
  follow-on at first-locale time (out of scope here except to document + leave the seam clean).

### Source-of-truth hashing (#3)

- Each translated key in a non-`en` catalog records `@key.sourceHash` = a **7-char hex** hash (git-style, sufficient) of
  the exact English value it was translated from. A `desktop-i18n-stale` check flags any locale key whose stored
  `sourceHash` ≠ the current English value's hash as STALE. Deterministic, git-independent, survives rebases/reformats.
- A per-key per-locale `reviewed: true` flag (human sign-off, principle 6) that the stale check resets to absent/false
  whenever `sourceHash` changes. Likely lightly used, but build it now so it's in place. Stale = warn; unreviewed = warn
  (escalation to error at release is a later policy choice, not built now).

## Milestones

Mostly serial (each builds on the prior); the M3 checks may parallelize after the helper + pseudolocale exist, but
serial is fine and aids code reuse — reconcile `registry.go` centrally (minor, like the orphan check did). **Nothing is
"done" until it's tested AND documented.**

### M0 — Foundation (helper + conventions)

- A shared **catalog/ICU helper** the pseudolocale + all checks reuse. JS/Node (so it parses messages with the SAME
  `intl-messageformat` parser the runtime uses — the only way to catch exactly what breaks at runtime): load `en` + any
  locale catalogs, strip `@`-metadata, and parse each message to its ICU AST to extract placeholders, `<tag>`s, and
  `plural`/`select` categories. Single source for "what tokens/structure does this message have".
- The `sourceHash` helper (7-char hex of a string) — shared by the pseudolocale generator + the stale check.
- Document the locale-format convention + the future resolver in `i18n.md` (and the `@key` schema gains `sourceHash`
  - `reviewed` in `messages/DETAILS.md`).
- Tests: unit-test the helper (placeholder/tag/plural extraction on representative messages) + the hash helper.

### M1 — Pseudolocale generator (#2)

- A deterministic generator: for each `en` key, emit an accented, ~+40%-longer value that PRESERVES every
  `{placeholder}`, `<tag>`, and ICU `plural`/`select`/`#` structure (only the human text between them is transformed),
  and writes `@key.sourceHash`. Deterministic (same English in → same pseudo out; no RNG/time). Output to a real,
  loadable locale dir (pick a standard pseudo tag, e.g. `en-XA`), gitignored + regenerable (like screenshots), with a
  small committed fixture subset for the check unit-tests.
- A command (`pnpm i18n:pseudo` or similar) to (re)generate it. Optionally wire the screenshots driver to capture in the
  pseudolocale for overflow (reuse `i18n-capture`'s surface list; a `--locale` axis) — at least document the path.
- Tests: the generator preserves placeholders/ICU (parse both sides with the M0 helper, assert token-set equality) and
  is deterministic + valid ICU. This is also the fixture M2/M3 consume.

### M2 — Stale detection (#3)

- `desktop-i18n-stale` check: for every locale key, compare stored `@key.sourceHash` vs the current English value's
  hash; flag mismatches (and missing-hash on a present translation) as stale; reset/ignore `reviewed` when stale.
  Warn-only. Reuse the M0 hash helper. The pseudolocale (fresh hashes) passes; a negative test (mutate an English value,
  or corrupt a pseudo hash) flags exactly that key.
- Tests: Go/Node unit tests + a run against the pseudolocale (clean → no stale; corrupt → stale).

### M3 — Locale-validation checks (#4)

Each reuses the M0 helper AND the M2 locale-check scaffolding (`apps/desktop/scripts/i18n-locale-check-lib.js`:
`localesToCheck()` / `loadBaseCatalog()` / `newFindings` / `reportFindings` / `runLocaleCheck`, plus the
`EXIT_CLEAN`/`EXIT_ISSUES`/`EXIT_ERROR` contract the Go wrapper maps — see that file's header for the pattern an M3
check follows). A new M3 check is a thin Node script (en catalog + one locale catalog in → per-key findings out, via
`runLocaleCheck`) plus a `desktop-i18n-<name>` Go wrapper modeled on `desktop-i18n-stale.go` (run the script, map exit 1
→ WARN, other non-zero → ERROR). Each is wired into `registry.go` (+ `ci.yml`, or a `NotInCI` warn-only reason like
`desktop-i18n-stale`), and is tested against the committed pseudolocale fixture (`test/fixtures/i18n-pseudolocale/`;
passes clean, negative test corrupts a copy to fail). Priority order:

- **Placeholder/tag parity** (critical — a missing/renamed/extra `{x}` or `<tag>` is a runtime crash, not a typo): each
  locale message's token set must equal English's.
- **ICU validity per locale**: every locale message compiles via `intl-messageformat` (catches stray `'`/`{`).
- **Plural-category coverage**: each locale's `plural` messages cover that locale's required CLDR categories (can differ
  from English's two).
- **Key parity / untranslated visibility**: every English key exists per locale (missing = silent English fallback);
  surface the missing/identical set so a "100% translated" claim is honest (mirror the screenshot coverage report).
- **Don't-translate tokens**: brand/system tokens (Cmdr, macOS, `{system_settings}`, etc.) preserved per locale.
- **Error-pipeline loud note** (not a check): `errors.*` go through `getMessage()` (raw, no ICU) — translators must NOT
  add ICU syntax there. Loud note in `messages/DETAILS.md` (errors section) + the translator guide.

### M4 — Translator guide (#1)

First check whether `docs/guides/i18n.md` already covers these end to end (may be a partial no-op); fill the gaps as a
clear guide in `docs/guides/`:

- **"Add a new language"** process: pick the BCP-47 tag, generate the skeleton (keys + sourceHashes), the per-language
  style guide (tone/formality/glossary), translate with the agent context below, run the checks, human review, ship.
- **"New feature → add strings + translate to ALL languages in great quality"** process: add the `en` key + `@key`
  description (to the bar), then for each locale read its style guide + translate the new/changed keys (update
  `sourceHash`), run the checks (parity/ICU/stale/plural), review. This is the routine maintenance loop.
- **The translator-agent context/system prompt** to hand an agent: per-string `@key` context + screenshot/note + the
  per-language style guide + the ICU instruction (preserve placeholders/`plural`/`select`; target-language CLDR plural
  categories) + the audit's two must-says: (a) pass-through placeholders (`{message}`/`{reason}`/raw `{path}`) are
  uncontrolled runtime inserts — structure sentences to tolerate them; (b) fragment/concatenation keys assemble via a
  named `*Join` — respect word order. Plus principle 6: human reviews translated copy.
- Tighten the description bar in `messages/DETAILS.md`: fragment keys must name their assembler;
  pass-through-placeholder keys must state the inserted value is uncontrolled.

## Out of scope (note, don't build)

- RTL (Arabic/Hebrew) — a separate UI-direction effort.
- The runtime locale resolver + a language selector — land with the first real locale (this plan keeps the seam clean
  and documents the convention).
- Escalating stale/unreviewed to release-blocking errors — a later policy choice.

## Definition of done (per the user: tested AND documented, quality over speed)

- Pseudolocale generates deterministically, preserves placeholders/ICU, carries hashes, and is regenerable by one
  command; documented.
- `desktop-i18n-stale` + the M3 checks exist, are wired into `pnpm check` + CI, have unit tests, and pass against the
  pseudolocale (with negative tests proving they catch the failure they're for).
- The locale-format convention, the `sourceHash`/`reviewed` schema, and the error-pipeline note are documented.
- The translator guide (add-a-language + new-feature-strings + agent context) is in `docs/guides/`.
- `pnpm check -q` green (known pre-existing warns aside); `pnpm intl:keys` union unchanged.

## Unrelated copy bug to fix alongside

`updates.checkToast.errorPrefix` (`"Error: {message}"`) and `settings.updates.errorPrefix` (`"Error:"`) use "Error",
which the style guide forbids in user-facing copy. Propose `"Couldn't check for updates: {message}"` /
`"Couldn't check:"` (David to confirm wording).
