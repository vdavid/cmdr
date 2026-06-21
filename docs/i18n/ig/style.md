# Igbo (ig) translation style guide

Working notes for translating Cmdr into Igbo. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice.

## Voice and tone

Friendly, concise, active, calm. Error messages stay calm and actionable. Igbo localized software is sparse, so lean on
Cmdr's own voice plus the GNOME Nautilus Igbo catalog rather than a major-vendor house style.

## Formality

**Igbo has no European-style T-V formality distinction.** No formal/informal pronoun split to resolve. Use imperative
verbs for buttons and impersonal phrasing for prompts.

- Buttons and menu items: imperative ("Detuo"/"Depụta" copy, "Bagharịa aha" rename, "Hichaa"/"Hapụ" delete, "Mepee"
  open, "Wepụ" cancel) per the GNOME Igbo catalog.

## Decision points

### Tone marking and diacritics (the defining technical call)

- Igbo is a TONAL language and standard orthography uses diacritics and dotted-below letters: ị, ọ, ụ, ṅ, plus tone
  marks. These are distinct Unicode characters, not optional accents; dropping them changes meaning.
- Majors: where Igbo is localized (GNOME), the dotted letters (ụ, ị, ọ) are used ("Faịlụ" file, "Bagharịa aha" rename).
- Recommendation: preserve full Igbo orthography with dotted-below letters; ensure they survive the catalog round-trip
  and the chosen UI font renders them. Tone diacritics are usually omitted in running text (as in GNOME); follow that.
  Confidence: high.

### Sparse major-product localization (low-priority signal)

- No Apple macOS Igbo and no Microsoft terminology Igbo in the reference pile; only GNOME Nautilus
  (`ig/gnome-nautilus/`). Igbo has limited localized-software coverage overall.
- This is the finding: Igbo is lower priority than the major locales, with one Tier-3 source and no native macOS anchor.
- Recommendation: treat as lower priority; rely on GNOME Nautilus plus a native reviewer. Confidence: high.

### Heavy English borrowing in tech register

- Everyday Igbo tech speech borrows English freely ("file", "folder"); the GNOME catalog Igbo-izes spelling ("Faịlụ" for
  file). Decide whether to use native coinages, Igbo-ized loans, or plain English loans.
- Recommendation: follow the GNOME catalog's Igbo-ized loans where it has them; flag the loan-vs-native balance for a
  native reviewer. Confidence: tentative.

### No grammatical gender

- Igbo has no grammatical gender (one pronoun "ọ"/"ya" for he/she/it). Inclusive language is a non-issue. Confidence:
  high.

## Terminology and glossary

Source: GNOME Nautilus (`ig/gnome-nautilus/`) is the only reference (no macOS, no MS). All `tentative` until a native
reviewer confirms. From the catalog: file → Faịlụ, open → Mepee, delete → Hichaa/Hapụ, rename → Bagharịa aha, cancel →
Wepụ, trash → Ebemkpofuozi. Confirm copy, move, search, folder, settings, volume, server with a native speaker (the
mined catalog had thin coverage for these).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR category: `other` only (verified `new Intl.PluralRules('ig')`, 2026-06-20). Igbo marks no grammatical plural number
on the noun in the way English does, so every plural message needs ONLY the `other` branch. `desktop-i18n-plural`
requires just `other` for `ig`.

## Notes and decisions

- **Orthography integrity:** preserve dotted-below letters (ị ọ ụ ṅ); never substitute plain Latin (i o u n).
- **Length:** Igbo runs roughly comparable to English; some actions are multi-word ("Bagharịa aha"). Overflow-check
  against the pseudolocale (`en-XA`).
- **Numbers/dates** come from the formatter layer. Never hardcode.

## Decisions to confirm with David

- **Loan-vs-native balance** and **most glossary terms**: only one Tier-3 source, no macOS/MS. Need a native reviewer.
- **Priority:** sparse major-product localization, single source. Lower priority than the major locales; flag whether to
  ship it in an early round.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ig/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
