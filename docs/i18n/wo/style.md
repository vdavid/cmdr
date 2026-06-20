# Wolof (wo) translation style guide

Working notes for translating Cmdr into Wolof. Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`wo`). The only reference in the pile is Microsoft terminology tagged `wo-SN` (Senegal);
there's no GNOME or macOS Wolof, so this is a thin, low-resource target.

## Voice and tone

Friendly, concise, active, and never alarmist, matching Cmdr's English voice. Wolof has very little modern software UI
to anchor a house tone; aim for plain, modern Wolof in the standardized Latin orthography. Keep error and crash copy
calm and factual.

## Formality

Wolof has no strong grammaticalized T/V politeness split like European languages; respect is carried lexically and by
honorific particles rather than a separate pronoun set. Use a plain, respectful register, prefer direct imperative verb
labels for UI actions. There's no shipped convention to copy, so confirm the register with David / a native reviewer.

## Decision points

### Script and orthography: standardized Latin vs Wolofal (Arabic-script)

- The choice: Wolof is written both in the official Latin-based orthography (the government/CLAD standard, used in
  education and digital text) and historically in Wolofal (an Arabic-derived script still used in religious contexts).
- Majors: Microsoft's Wolof terminology uses the Latin orthography (verified: `bara denc` File, `booleeb denc` Folder,
  `sotti` Copy, `far` Delete, reference pile `wo-SN/microsoft-terminology`, 2026-06-19). No vendor uses Wolofal for UI.
- Recommendation: use the standardized Latin orthography. It's the script of digital Wolof and the only one any vendor
  reference supports; Wolofal would be wrong for a software UI and would also drag in RTL/Arabic-script layout work for
  no audience benefit.
- Confidence: high.

### Special Latin characters and orthographic consistency

- The choice: standard Wolof Latin uses characters beyond ASCII (à, ë, ñ, ŋ, and doubled vowels marking length, e.g.
  `booleeb`). Spelling conventions (vowel length, prenasalized consonants) vary in practice between sources.
- Majors: Microsoft uses the doubled-vowel length convention (`booleeb denc`, `bara denc`).
- Recommendation: follow the CLAD/Microsoft Latin convention consistently, including ñ and ŋ and doubled long vowels;
  keep text in NFC so the special characters compare stably. Verify the app font renders ŋ (eng) and ñ without fallback
  boxes.
- Confidence: high (orthography), medium (which exact spelling when sources are silent: needs native review).

### Region / variant

- The choice: the pile has only `wo-SN` (Senegal). Wolof is also spoken in Gambia and Mauritania, but Senegal carries
  the standardization and the only reference.
- Recommendation: ship a single `wo` base targeting standardized (Senegal-anchored) Wolof; don't split a variant.
- Confidence: high.

### Loanwords from French

- The choice: Senegalese Wolof borrows heavily from French in everyday and technical registers; some computing terms may
  be more natural as French-derived borrowings than as coinages.
- Majors: Microsoft's terms are mostly native/descriptive (`bara denc` literally "container for keeping"), not French
  borrowings, but real-world digital Wolof code-switches with French freely.
- Recommendation: prefer Microsoft's descriptive native terms where they exist for consistency; allow a well-understood
  French-derived borrowing only where no natural Wolof term exists and the borrowing is genuinely the expected word.
  Record each call in the glossary.
- Confidence: tentative (depends on native-speaker expectation).

## Terminology and glossary

Only Microsoft terminology is available, so terms are MS-sourced and need native review for Cmdr's voice.

| English term | Wolof        | Notes                                                           |
| ------------ | ------------ | --------------------------------------------------------------- |
| File         | bara denc    | MS terminology (high; literally a thing that holds saved items) |
| Folder       | booleeb denc | MS terminology (high)                                           |
| Copy         | sotti        | MS terminology (high)                                           |
| Delete       | far          | MS terminology (high)                                           |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. The `{email}`-style
placeholder tokens are also verbatim.

## Plurals

Wolof CLDR plural categories: `other` only (run `new Intl.PluralRules('wo').resolvedOptions().pluralCategories` to
confirm). Wolof doesn't inflect the noun for number the way English does, so one branch covers all counts; write it to
read naturally with the count placeholder.

## Notes and decisions

- **Apostrophes in ICU**: double every apostrophe in ICU strings; normal apostrophes in `errors.*`.
- **Unicode normalization**: keep ñ, ŋ, à, ë and long vowels in NFC.

## Decisions to confirm with David

- Register / address form (no shipped convention to anchor it).
- Whole glossary is from a single source (Microsoft terminology); needs a native Wolof reviewer before shipping, and
  GNOME-level file-manager terms (pane, tab, trash, transfer) aren't in any pile source, so they'll be coined fresh.
- French-borrowing vs native-coinage policy for terms Microsoft doesn't cover.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/wo/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
