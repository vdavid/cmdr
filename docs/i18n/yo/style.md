# Yoruba (yo) translation style guide

Working notes for translating Cmdr into Yoruba. Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`yo`), standard Yoruba in Latin script with full tone-marking. The MS terminology in the
reference pile is tagged `yo-NG` (Nigeria); for Cmdr's purposes a single `yo` base covers it (see Region below).

## Voice and tone

Friendly, concise, active, and never alarmist, matching Cmdr's English voice. Aim for plain, modern Yoruba (the register
of contemporary Nigerian Yoruba media and the localized software that exists), not proverb-heavy or archaic phrasing.
Keep error and crash copy calm and factual.

## Formality

Yoruba's deep honorific system (the plural/respectful **ẹ** vs singular **o**, and respectful verb forms) is mostly a
spoken-address concern. For UI, use a neutral, respectful register: prefer plain imperative verb labels for actions
("Ṣẹ̀dà" Copy, "Paá rẹ́" Delete) and the respectful **ẹ** form if a sentence directly addresses the user. Keep it
consistent. There's no large shipped Yoruba software corpus to lock a convention, so confirm the address form with David
/ a native reviewer.

## Decision points

### Tone marks and diacritics (load-bearing, highest-risk)

- The choice: Yoruba spelling is incomplete and can change meaning without its diacritics. It uses tone marks (acute =
  high, grave = low, mid = unmarked) AND sub-dot vowels/consonants (ẹ, ọ, ṣ), and these stack (e.g. `ṣẹ̀dà` Copy,
  `ìfẹnukò` from MS terminology, `Parẹ́̀` Cancel from GNOME). Dropping or flattening a mark produces a different or
  nonsense word.
- Majors: Microsoft's Yoruba terminology fully tone-marks (verified: `fáìlì` File, `fódà` Folder, `ṣẹ̀dà` Copy, `àjọlò`,
  reference pile `yo-NG/microsoft-terminology`, 2026-06-19). The GNOME Yoruba catalog also tone-marks (`Parẹ́̀`, `Fódà`).
- Recommendation: preserve every tone mark and sub-dot exactly; treat them as load-bearing, never optional. Two concrete
  guardrails: (1) the translation must use proper precomposed/combining Unicode (NFC), not ASCII-fallback spelling; (2)
  the UI font must render stacked combining marks (acute over a sub-dot vowel). Verify the chosen app font shows `ọ́`/`ẹ̀`
  without clipping or tofu before shipping (Cmdr respects the system font, so this is a real rendering risk to
  overflow-check).
- Confidence: high (that marks are load-bearing); the font-rendering check is the open action.

### Loanword vs coined term (anglicisms)

- The choice: many computing terms are borrowed-and-respelled in Yoruba (`fáìlì` from "file", `fódà` from "folder")
  rather than freshly coined. Whether to follow Microsoft's borrow-respell convention or prefer descriptive native
  phrases.
- Majors: Microsoft borrows-and-respells heavily (`fáìlì`, `fódà`); GNOME mixes borrowing with native words.
- Recommendation: follow the borrow-and-respell convention for common computing nouns (file, folder), since it matches
  user expectation from the dominant localized software. Use native words where they're well established and natural.
  Record each choice in the glossary with its source.
- Confidence: high.

### Region / variant

- The choice: the pile has only `yo` (GNOME) and `yo-NG` (MS terminology). Yoruba is also spoken in Benin and Togo, but
  the localization weight and the only vendor reference are Nigerian.
- Recommendation: ship a single `yo` base targeting standard (Nigerian-anchored) Yoruba; don't split a region variant.
- Confidence: high.

### Gender / inclusive language

- The choice: Yoruba is grammatically genderless (no gendered pronouns: `ó` covers he/she/it), so the inclusive-language
  problem that dogs gendered languages mostly doesn't arise.
- Recommendation: no special handling needed; the language is neutral by default.
- Confidence: high.

## Terminology and glossary

| English term  | Yoruba    | Notes                                                     |
| ------------- | --------- | --------------------------------------------------------- |
| File          | fáìlì     | borrowed-respelled; MS terminology (high)                 |
| Folder        | fódà      | borrowed-respelled; MS terminology + GNOME agree (high)   |
| Copy          | ṣẹ̀dà      | MS terminology (high); note the stacked marks             |
| Delete        | paá rẹ́    | MS terminology (high)                                     |
| Move to Trash | (confirm) | GNOME Yoruba is partly untranslated; needs review         |
| Cancel        | parẹ́      | GNOME "Cancel" = `Parẹ́̀` (tentative; verify mark stacking) |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. The `{email}`-style
placeholder tokens are also verbatim.

## Plurals

Yoruba CLDR plural categories: `other` only (run `new Intl.PluralRules('yo').resolvedOptions().pluralCategories` to
confirm; matches the GNOME header). Yoruba doesn't grammatically inflect nouns for number, so one form covers all
counts. A plural message still needs a natural-reading single branch (often the count plus an unchanged noun).

## Notes and decisions

- **Apostrophes in ICU**: double every apostrophe in ICU strings; normal apostrophes in `errors.*`.
- **Unicode normalization**: keep all tone-marked text in NFC; a translator pasting from mixed sources can produce
  inconsistent combining sequences that look identical but compare unequal.

## Decisions to confirm with David

- Address form (respectful `ẹ` vs neutral) for sentences that speak to the user directly.
- App-font rendering of stacked tone-mark + sub-dot combinations (overflow/clip check before shipping).
- GNOME Yoruba is ~30% untranslated, so several common terms have only the borrow-leaning MS source; native review
  recommended.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/yo/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
