# Karbi (mjw)

Working notes for translating Cmdr into this language. Read [`README.md`](../README.md) for how this fits the
translation process.

This is a very low-priority language: do NOT start a translation pass without a native Karbi speaker driving it. See
Decision points first; that's the load-bearing part of this guide.

## Voice and tone

Friendly, concise, active, calm. Match Cmdr's English register. No software-UI house style exists for Karbi, so the
translator effectively sets it; keep it plain and modern.

## Formality

Karbi is not documented as having a grammaticalized T/V (formal vs. informal "you") distinction the way European
languages do, and there's no UI precedent to copy. Default to plain, direct imperatives for buttons and menu items. Low
confidence; a native reviewer should confirm whether any politeness marking is expected in app chrome.

## Decision points

Settle these before any translation pass. The first two are the whole story for Karbi.

- **Viability / priority (the headline finding).** Karbi is a very-low-resource language for software localization. No
  Apple, Microsoft, Google, Spotify, or Netflix product ships in Karbi, and Google Translate does NOT support `mjw`
  (verified 2026-06-20 against Google Cloud Translation's language list, which carries 250+ languages and includes only
  Meitei/Manipuri from the region). So there's almost no inherited computing vocabulary and no major-vendor precedent.
  The one real precedent is a PARTIAL community GNOME Nautilus translation (`mjw`, by Jor Teron, 2019-20, ~27% of
  strings), the sole item in the reference pile. Recommendation: treat as low priority; ship only on a specific
  community request, and only with a native Karbi speaker as translator and reviewer. Confidence: high.
- **Script: Latin/Roman (the notable point for the region).** Unusually for Northeast India, Karbi is standardly written
  in the Latin alphabet (missionary origin), occasionally in Assamese script; a separate Arleng script (Sarthe Teron
  Milik, 1990s) exists but isn't used in software. Use Latin. The standard Latin orthography has no documented
  diacritics or tone marks, and the Nautilus reference data is plain ASCII letters, so expect no special characters
  beyond standard a-z (verified 2026-06-20 via Omniglot + the Nautilus `.po`). Left-to-right, no RTL concerns.
  Confidence: high.
- **Loan-word policy.** The Nautilus precedent borrows computing terms wholesale from English (Window, Codec, Album,
  Frame rate, Bit rate, Stereo) while coining native Karbi for everyday concepts (Kiri = search, Lun = audio, Apor =
  duration, amen = name/file). Follow that: keep technical/brand terms English, coin native words for plain concepts,
  and record every call in the glossary so it stays consistent. This is judgment-heavy and best done with a native
  reviewer. Confidence: medium.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Run `new Intl.PluralRules('mjw').resolvedOptions().pluralCategories` and cover what it reports. CLDR data for Karbi is
thin (the Nautilus `.po` declares the generic `nplurals=2; plural=(n != 1)`); verify what the runtime actually returns
before writing plural branches, and have a native speaker confirm whether Karbi marks number on the noun at all.

## Terminology and glossary

(Optional; fill as terms come up. Seed candidates from the Nautilus precedent: Kiri = search, Lun = audio, Apor =
duration, amen = name/file. Expect to coin most file-manager terms with a native reviewer, borrowing English for
technical terms per the loan-word policy above.)

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/mjw/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
