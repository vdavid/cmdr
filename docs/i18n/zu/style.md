# Zulu (zu) translation style guide

Working notes for translating Cmdr into Zulu (isiZulu). Read [`README.md`](../README.md) for how this fits the
translation process.

This is the language base (`zu`), isiZulu in Latin script. The pile has a GNOME catalog (`zu`, ~88% translated) and
Microsoft terminology plus a Microsoft style guide (`zu-ZA`); for Cmdr a single `zu` base covers it (see Region).

## Voice and tone

Friendly, concise, active, and never alarmist, matching Cmdr's English voice. Microsoft's isiZulu style guide explicitly
calls for warm, relaxed, conversational, less-formal voice (verified: `zu-ZA/microsoft-style-guides/StyleGuide.pdf`,
2026-06-19), which lines up well with Cmdr. Keep error and crash copy calm and factual.

## Formality

isiZulu has no European-style T/V pronoun split. Microsoft's guide says to address the user directly as "you" (`wena` /
second-person verb forms) and to avoid third-person, formal phrasings like "the user", which sound impersonal (verified,
same source). Use that direct, respectful second-person register, with plain imperative verb labels for UI actions
("Vula" Open, "Sula" Delete). Confirm with David / a native reviewer only if a more honorific register is wanted.

## Decision points

### Noun-class agreement and the placeholder problem (load-bearing)

- The choice: isiZulu is a Bantu noun-class language. Verbs, adjectives, and possessives take concord prefixes agreeing
  with the noun's class. A sentence template with a `{name}` or `{path}` insert can't pick the right concord, because
  the inserted noun's class is unknown at write time.
- Majors: this is intrinsic to all Nguni-language localization; vendors handle it by phrasing around the problem (using
  class-neutral framings, or putting the variable noun in a position that doesn't drive agreement).
- Recommendation: write sentence templates so no agreement morpheme depends on an inserted `{name}`/`{path}`/`{count}`.
  Prefer phrasings where the insert sits in an agreement-neutral slot (e.g. label + colon + value, or a fixed carrier
  noun like "ifayela" that owns the concord). This is the single biggest blind-translation risk for isiZulu and the
  translator must restructure rather than translate word-for-word. Flag to David: some English templates may need a
  reworded English source to be cleanly translatable here.
- Confidence: high.

### Loanword vs native term

- The choice: computing nouns split between borrowed-respelled (`ifayela` file, `ifolda` folder per GNOME) and native
  descriptive words (`isikhwama`, literally "bag/pouch", which Microsoft uses for folder).
- Majors: GNOME uses borrowings (`ifayela`, `ifolda`); Microsoft prefers a native term for folder (`isikhwama`)
  (verified, reference pile 2026-06-19). The two sources disagree on "folder".
- Recommendation: prefer the borrowed-respelled forms that match the dominant open-source file-manager catalog
  (`ifayela`, `ifolda`) for immediate user recognition, unless a native reviewer says the Microsoft native term reads
  better. Record the resolution; this is a genuine source conflict.
- Confidence: tentative (sources conflict on folder).

### Region / variant

- The choice: the pile has `zu` (GNOME) and `zu-ZA` (Microsoft); isiZulu is overwhelmingly South African.
- Recommendation: ship a single `zu` base; don't split a region variant.
- Confidence: high.

### Gender / inclusive language

- The choice: isiZulu has no grammatical gender and class-1 personal references aren't gendered, so the
  gendered-language inclusivity problem mostly doesn't arise.
- Recommendation: no special handling needed.
- Confidence: high.

## Terminology and glossary

| English term  | Zulu               | Notes                                                                      |
| ------------- | ------------------ | -------------------------------------------------------------------------- |
| File          | ifayela            | borrowed-respelled; GNOME (high)                                           |
| Folder        | ifolda / isikhwama | GNOME `ifolda` vs MS `isikhwama`; conflict, see decision point (tentative) |
| Copy          | kopisha            | MS terminology (high)                                                      |
| Delete        | sula               | MS terminology + GNOME agree (high)                                        |
| Open          | vula               | GNOME (high)                                                               |
| Rename        | qamba futhi        | GNOME (high)                                                               |
| Cancel        | khansela / cima    | GNOME shows `cima`; verify against MS (tentative)                          |
| Move to Trash | hambisa kudoti     | GNOME (high)                                                               |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. The `{email}`-style
placeholder tokens are also verbatim.

## Plurals

isiZulu CLDR plural categories: `one`, `other` (run `new Intl.PluralRules('zu').resolvedOptions().pluralCategories` to
confirm; matches the GNOME 2-form header). Note: isiZulu treats 0 as `one`. Plurals interact with noun-class prefixes
(singular vs plural class), so the plural branch may change the noun's prefix, not just append an "s"; write both
branches as full native forms.

## Notes and decisions

- **Apostrophes in ICU**: double every apostrophe in ICU strings; normal apostrophes in `errors.*`.
- **Capitalization**: isiZulu nouns carry lowercase class prefixes; don't force English-style title-case on a noun whose
  prefix should stay lowercase mid-label.

## Decisions to confirm with David

- "Folder" term: borrowed `ifolda` (GNOME) vs native `isikhwama` (Microsoft).
- Whether any English sentence template needs rewording so its `{name}`/`{path}` insert avoids breaking noun-class
  concord.
- Native isiZulu review for the conflict terms and the agreement-restructured sentences.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/zu/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
