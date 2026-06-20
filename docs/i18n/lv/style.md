# Latvian (lv) translation style guide

Working notes for translating Cmdr into Latvian. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Latvian.

## Voice and tone

Friendly, concise, active, calm. Latvian UI convention already drops the second-person pronoun and addresses the user
through the verb form, which suits Cmdr's direct voice. Microsoft also prefers suggestive "varat darīt" over a bare
imperative where it reads friendlier. Error messages stay calm and actionable; state the problem and a next step.

## Formality

**Verdict: informal `tu`, not the formal `jūs`.** Consumer brands (around 17 of 20 surveyed — Swedbank, Rimi, the
telcos, IKEA, Coca-Cola — use `tu`; `jūs` is reserved for legal copy) address Latvian users informally, which fits
Cmdr's friendly personal voice. Microsoft's style guide leans formal `jūs`, but Cmdr deliberately picks the warmer
consumer-brand register. Formality decision recorded in
[`formal-informal-decisions.md`](../formal-informal-decisions.md).

- **Labels and instructions:** the pronoun-free singular imperative reads clean (saglabā, atver) and aligns with the
  `tu` register; the pronoun-free style stays the default where it works.
- **Direct address: informal singular `tu` / `tev` / `tavs`.** Use it when a pronoun is unavoidable, not the formal
  `jūs`.

## Decision points

No macOS anchor (priority signal):

- Apple does NOT ship a Latvian macOS UI; Latvian isn't among Apple's display languages (keyboard/input/spellcheck
  only). So there's no Tier-1 Finder reference. Authority: Microsoft (Tier 2: terminology + 97-page style guide) and
  GNOME/Xfce (Tier 3), both complete. Recommendation: Microsoft for terms, GNOME/Xfce for file-manager parity, and
  native review before shipping. For any Mac-specific term (Trash, Finder-style menus), lean on GNOME parity plus a
  native reviewer. Confidence: high.

CLDR plurals, the `zero` category gotcha:

- lv categories: `zero`, `one`, `other` (three forms).
  - one: n%10=1 and n%100 != 11 (1, 21, 31; NOT 11)
  - zero: n%10=0, OR n%100 in 11..19 (so 0, 10, 11-19, 20, 30, …, 110-119), triggers genitive plural
  - other: everything else (2-9, 22-29)
- The `zero` form is NOT just the literal 0: it covers 10, 11-19, and any number ending in 0. A naive two-form
  (singular/plural) message is grammatically wrong for Latvian. Note: the gettext catalogs order the buckets one/other/
  zero (zero LAST in gettext index), which is easy to get wrong; in CLDR/ICU naming the same buckets are zero/one/other.
- Recommendation: author all three CLDR categories for every counted noun, and add a check/test so a translator can't
  ship only one+other. Confidence: high.

Case system vs placeholder insertion (structural risk):

- Latvian nouns inflect across 7 cases x 2 genders; a `{name}`/`{count}` doesn't auto-agree with the surrounding
  sentence. "Move to {folder}" needs {folder} in a case a nominative filename can't provide; "{count} files" needs the
  noun in genitive plural when {count} hits the `zero` category.
- How the majors cope: Microsoft's style guide tells translators to rewrite, use demonstratives ("šis dokuments")
  instead of bare placeholders, restructure so the placeholder lands in nominative, keep the variable where it doesn't
  force agreement. No runtime case engine.
- Recommendation: design lv strings so placeholders sit where nominative is grammatical (end of sentence, or after a
  colon), use the plural mechanism (above) for the count noun rather than templating it, and make per-string case fit a
  native-review checklist item. Confidence: high (general), tentative per-string.

"File" term, fails vs datne (David to settle):

- Real split: Microsoft and Xfce use `fails` (everyday, colloquial); GNOME uses `datne` (the purist standard-language
  term promoted by the State Language Centre). This recurs everywhere in a file manager, so the two can't be mixed.
- Recommendation: `fails` (Microsoft + familiarity, fits a friendly voice), but flag, a native reviewer may prefer
  `datne` for correctness. Confidence: tentative.

## Terminology and glossary

Format: `English → chosen · sources · confidence`. M = Microsoft, N = Nautilus, T = Thunar.

- file → fails · M, T (N: datne) · tentative, see Decision points
- folder → mape · M, N · high
- copy → kopēt · M, N · high
- move → pārvietot · M, N · high
- cancel → atcelt · M, N · high
- delete → dzēst · M · high
- send → sūtīt · M · high
- search → meklēt (verb) / meklēšana (noun) · N, M · high
- open → atvērt · N · high
- rename → pārdēvēt · N · high
- settings → iestatījumi · M · high
- version → versija · M · high
- trash → miskaste · N, T (M: atkritne for "recycle bin") · tentative, miskaste fits a macOS desktop app
- properties → īpašības · N · high
- cut → izgriezt · N · high
- paste → ielīmēt · N · high

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `zero`, `one`, `other` (see Decision points for exact rules and the gettext-ordering gotcha). Author
all three for any counted noun. The `desktop-i18n-plural` check requires every plural message to cover the categories
this language needs.

## Notes and decisions

- Diacritics: ā č ē ģ ī ķ ļ ņ š ū ž. Don't strip them. Overflow-check against the pseudolocale (`en-XA`).
- Sentence case fits Latvian (first word + proper nouns only).
- Numbers and dates come from the formatter layer. Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

- "File" → fails vs datne (pick one, never mix).
- Trash → miskaste vs atkritne (recommend miskaste for a macOS app).
- Whether to ship Latvian without a macOS anchor, and the native-review budget.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in
[`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/lv/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
