# Armenian (hy) translation style guide

Working notes for translating Cmdr into Armenian (հայերեն). Read [`README.md`](../README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice.

**Tag note:** `hy` (ISO 639-1) and `hye` (ISO 639-3) are the SAME language, Armenian. The reference pile and the
team-lead's batch listed both base tags; they are not two languages. Use `hy` (the BCP-47 base tag the app catalog
should use); `hye` is a duplicate and needs no separate style guide. (Cross-language note for the maintainer: if the
batch process generated a `hye` slot, it should collapse into `hy`.)

## Voice and tone

Friendly, concise, active, calm. Error messages stay calm and actionable and avoid a bare "Սխալ" (error) label: state
the problem and a next step. Armenian Apple/Microsoft localization is thin (see Decision points), so lean on Cmdr's own
voice and the GNOME/file-manager sources rather than copying a house style.

## Formality

**Armenian has a T-V distinction: "դու" (du, informal singular) vs "Դուք" (Duk', formal/plural).** UI convention varies;
software localizations typically use the formal "Դուք" for respect, but a friendly consumer app can use informal "դու".
Like other languages, the cleaner path is to AVOID the pronoun: use imperative verbs for buttons and impersonal phrasing
for prompts, which sidesteps the choice.

- Buttons and menu items: imperative ("Պատճենել" copy, "Տեղափոխել" move, "Ջնջել" delete, "Բացել" open, "Չեղարկել"
  cancel).
- Recommendation: avoid the second-person pronoun; where unavoidable, lean informal "դու" to match Cmdr's friendly,
  David-signed voice. Confidence: tentative (no strong Apple/MS Armenian convention to anchor on); flag for David.

## Decision points

### Script: Armenian alphabet (single script, settled)

- Armenian uses its own unique alphabet (Ա-Ֆ), not Latin or Cyrillic. There is no script choice to make.
- Ensure the Armenian Unicode block survives the catalog round-trip; never transliterate to Latin.
- Confidence: high.

### Regional variant: Eastern vs Western Armenian (the real call)

- Two standardized literary forms: Eastern Armenian (Armenia, the larger user base, the de-facto computing standard) and
  Western Armenian (diaspora). They differ in orthography, verb conjugation, and some vocabulary.
- Majors: where Apple/Microsoft localize Armenian at all, they target EASTERN Armenian (hy / hy-AM, Republic of
  Armenia).
- Recommendation: target Eastern Armenian (hy-AM as the regional anchor). Confidence: high.

### Reformed vs classical (Mesropian) orthography

- Armenia uses the reformed (Soviet-era) orthography; the diaspora keeps classical Mesropian orthography. This tracks
  the Eastern/Western split above.
- Recommendation: reformed orthography (matches Eastern Armenian / hy-AM). Confidence: high.

### Sparse major-product localization (low-priority signal)

- The reference pile has GNOME Nautilus for Armenian but limited/absent Apple and Microsoft file-manager strings. Apple
  ships no full Armenian macOS UI; Microsoft Armenian is partial.
- This means fewer authoritative anchors and a smaller localized-software ecosystem. Lower priority than major locales.
- Recommendation: treat Armenian as lower priority; rely on GNOME Nautilus (`hy/gnome-nautilus/`) plus a native
  reviewer. Confidence: high (this is the priority finding itself).

### No grammatical gender

- Armenian has NO grammatical gender (nouns, pronouns, adjectives are genderless; one pronoun "նա" for he/she/it).
  Inclusive language is a non-issue. Confidence: high.

## Terminology and glossary

Source priority for Armenian: GNOME Nautilus (`hy/gnome-nautilus/`) is the main file-manager reference (no macOS,
partial MS). Triangulate against a native reviewer. Add rows as terms come up; mark most `tentative` until a native
speaker confirms, given the thin authoritative base. Core verbs to confirm: copy → Պատճենել, move → Տեղափոխել, delete →
Ջնջել, rename → Վերանվանել, open → Բացել, search → Որոնել, cancel → Չեղարկել, trash → Աղբարկղ, folder → Պանակ/Թղթապանակ,
file → Ֆայլ.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories: `one`, `other` (verified `new Intl.PluralRules('hy')`, 2026-06-20). Note Armenian's `one` includes 0
and 1 (numbers 0–1 are `one`), unlike English. Trust ICU's selection; write both branches. Armenian declines nouns by
case and number, so decline the counted noun correctly inside each branch.

## Notes and decisions

- **Script integrity:** Armenian letters and punctuation (the Armenian full stop `։`, question mark `՞`, emphasis `՜`)
  are distinct Unicode; preserve them, don't substitute Latin equivalents.
- **Length:** Armenian runs roughly comparable to or slightly longer than English. Overflow-check against the
  pseudolocale (`en-XA`).
- **Numbers/dates** come from the formatter layer. Never hardcode separators.

## Decisions to confirm with David

- **Formality** (informal դու vs formal Դուք) and **most glossary terms**: no strong Apple/MS Armenian convention to
  anchor on, so these are tentative. Confirm with a native Eastern Armenian reviewer.
- **Priority:** Armenian has sparse major-product localization and no native macOS. Lower priority than the major
  locales; flag whether Cmdr should ship it at all in an early round.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/hy/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
