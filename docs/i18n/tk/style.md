# Turkmen (tk) translation style guide

Working notes for translating Cmdr into Turkmen. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Turkmen.

## Priority signal (read first)

Turkmen localization is very sparse. There is NO Apple Tier-1 source (Apple does not ship Turkmen as a macOS UI
language), no Microsoft style guide, and no Google, Spotify, or Netflix Turkmen UI found. The only references are
Microsoft terminology (a glossary, Latin, Turkmenistan-standard) and one old, partial GNOME Nautilus catalog (2004
to 2005, ~80% translated, Iranian-Turkmen flavored). There is essentially no full-UI major-vendor prior art to anchor
user expectations. Cmdr would be a near-first-mover. Flag for David: is shipping Turkmen worth it given near-zero
major-vendor precedent and a thin, partly dated reference base? Treat every term below as review-gated; native review
is needed before ship.

## Voice and tone

Friendly, concise, active, calm. Address the user politely (see Formality). Error messages stay calm and actionable,
and never use the words "error" or "failed".

## Formality

- Turkmen is a Turkic, agglutinative language with a T-V-like distinction: `sen` (informal) vs `siz` (polite/plural).
- Both reference sources phrase UI actions as bare imperatives (`Aç` open, `Ýap` close, `Göçür` copy, `Poz`/`Ýokla`
  delete), which is register-neutral and standard for buttons and menu items. Keep buttons and menu items as short bare
  imperatives.
- Where a full sentence addresses the user directly (confirmations, prompts), prefer the polite `siz` register and its
  polite second-person endings, consistent with Cmdr's respectful voice. Don't mix `sen` and `siz` within the app.
- Confidence: tentative. The bare-imperative button convention is well-attested; the sentence-level `siz` choice is a
  reasoned default, not sourced, so flag for David and native review.

## Decision points

Script, Latin (the #1 decision, high confidence):
- Options: Latin vs Cyrillic. Turkmen officially switched to a Latin alphabet after independence (adopted 1993,
  modeled on Turkish), and it is the standard script in Turkmenistan today. Cyrillic is legacy and older-generation.
- Both reference sources are Latin, confirmed from the actual strings (Turkmen Latin letters present: ä, ç, ž, ň, ö, ş,
  ü, ý). Base tag `tk` = Latin.
- The Turkmen Latin alphabet has 30 letters. Don't substitute Turkish look-alikes for the Turkmen-specific letters:
  `ä`, `ž`, `ň`, `ý` are Turkmen-specific (Turkish uses `e`, `j`, `n`, `y`/`ı` differently). Render all eight special
  letters correctly: ä ç ž ň ö ş ü ý (and their capitals Ä Ç Ž Ň Ö Ş Ü Ý).
- Recommendation: Latin only; don't build Cyrillic. Confidence: high.

Russian-loanword vs native term (high-confidence pattern, per-term call):
- Tech vocabulary splits between Russian loanwords and native Turkic terms. The old GNOME catalog leans on Russian
  loanwords (`program`, `kompýuter`, `dýalog`, `desktap`, `menýu`), reflecting its era and Soviet-legacy vocabulary.
  Microsoft terminology (the modern Turkmenistan-standard glossary) tends to native or established Turkmen forms.
- Recommendation: prefer the Microsoft-terminology form when it exists and sounds native; treat the GNOME catalog as
  weaker, dated evidence. Where only a Russian loanword is attested and is the everyday word (for example `faýl` for
  file, universally used), keep it. Decide per term and record the source. Confidence: high that the split exists; each
  individual term is review-gated.

Agglutination and placeholder-suffix agreement (engineering pitfall, high confidence):
- Turkmen is agglutinative with vowel harmony: case and possessive suffixes harmonize with the final vowel of the
  preceding word (front/back). A suffix glued to a variable `{name}` or `{path}` can't agree at translation time, and
  the inserted value's length, case, and characters are uncontrolled.
- How the sources cope: they attach suffixes to the fixed noun, not the variable (for example put the case suffix on
  the word for "file"/"folder", leaving the placeholder in nominative).
- Recommendation: never glue a grammatical suffix directly to a `{placeholder}`. Structure messages so suffixes land on
  fixed words, or keep the inserted value in nominative at the sentence end. The exact rewrite per string is a native
  review task. Confidence: high (grammar well-documented).

Gender, none (simplifies, high confidence):
- Turkmen is Turkic and has NO grammatical gender and no gendered third-person pronoun. No gender agreement to track.
  The Microsoft terminology marks `grammaticalGender = NotSelected` on essentially every entry, confirming this. Don't
  invent gendered phrasing. Confidence: high.

Number and date formatting (defer to the formatter, high confidence):
- Numbers and dates come from the `Intl` formatter layer for the locale; never hardcode separators or date order.
  Follow Cmdr's app-wide rule (ISO dates where the UI shows raw dates, thousands separators on user-facing counts via
  the formatter). Confidence: high (mechanism, not a Turkmen-specific call).

## Terminology and glossary

Format: `English → chosen · sources · confidence`. Sources are thin (MS terminology; GNOME old/partial), so confidence
is modest and native review is needed. A full glossary is not required yet; seed terms below.

- file → faýl · MS, GNOME · high (universal everyday loanword)
- folder → bukja · MS · high (GNOME predates the term)
- copy (verb) → göçür · MS, GNOME · high (note: MS also lists `synp` for the noun "a copy"/duplicate, NOT the verb)
- move → ýerini üýtget · MS · tentative (GNOME uses `göçir`, which collides with "copy"; prefer MS's distinct form)
- delete → poz · GNOME (MS: `ýokla`) · tentative, `poz` reads as the everyday "erase/delete"; confirm against `ýokla`
- rename → adyny üýtget · GNOME (`Adyny Ewez et`) · tentative, modernize `Ewez et` to `üýtget`; native review
- open → aç · GNOME, MS · high
- search → gözleg · MS, GNOME · high
- cancel (button) → ýatyr / goý bes et · tentative, GNOME uses `Ybtal` (Arabic-origin, dated); MS `bes etmek` is the
  "stop/abort" sense. Native review for the dialog-Cancel button term.
- trash → zibil · GNOME (MS: `Taşlandy Sebet` = recycle bin) · tentative, `zibil` is short and fits the file-manager
  domain; confirm whether `Sebet` (basket) reads better
- new → täze · GNOME, MS · high
- save → ýatda sakla · MS · high
- paste → ýelme · MS · high
- close → ýap · MS · high
- settings → sazlamalar · MS · high
- tab → tab · MS · tentative (loanword; confirm a native term isn't preferred)

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style and
`{email}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`).

## Plurals

CLDR categories: `one`, `other` (verified: `new Intl.PluralRules('tk').resolvedOptions().pluralCategories`). Author
both branches for every plural message. Note: the old GNOME catalog declared `nplurals=2; plural=(n != 1)` (gettext
convention), but Cmdr's intl layer is CLDR-based, so use the `one`/`other` buckets. Like other Turkic languages,
Turkmen keeps the counted noun singular after a numeral (no plural suffix after a number), so the two forms are often
the same word, but the framework still needs both buckets. The `desktop-i18n-plural` check requires every plural
message to cover the categories this language needs.

## Notes and decisions

- Latin with Turkmen-specific letters: Ä ä, Ç ç, Ž ž, Ň ň, Ö ö, Ş ş, Ü ü, Ý ý. Don't substitute Turkish or Russian
  look-alikes.
- Numbers and dates come from the formatter layer. Never hardcode separators.
- The GNOME catalog is old (2004 to 2005) and Iranian-Turkmen flavored; weight Microsoft terminology higher for the
  modern Turkmenistan standard. Record any conflict here when it comes up.
- Record case-by-case rulings here so they are not relitigated.

## Decisions to confirm with David

- Go/no-go on shipping Turkmen at all (near-zero major-vendor precedent, thin and partly dated reference base).
- Sentence-level formality register (`siz` polite is the proposed default; not sourced).
- Per-term calls flagged `tentative` above: move, delete (`poz` vs `ýokla`), rename, the Cancel-button term, and trash
  (`zibil` vs `Sebet`).
- Every placeholder-bearing string needs native review for suffix and vowel-harmony agreement.

## ICU mechanics

Catalog-level, language-agnostic: double every apostrophe in a value (`'` → `''`), and keep every `{placeholder}` and
`<tag>` verbatim. Full rules: the agent-handoff block in [`../guides/i18n-translation.md`](../../guides/i18n-translation.md)
and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/tk/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
