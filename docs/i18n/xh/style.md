# Xhosa (xh) translation style guide

Working notes for translating Cmdr into Xhosa (isiXhosa). Read [`README.md`](../README.md) for how this fits the
translation process.

This is the language base (`xh`), isiXhosa in Latin script. The pile has a GNOME catalog (`xh`, ~86% translated) and
Microsoft terminology (`xh-ZA`); for Cmdr a single `xh` base covers it (see Region). isiXhosa is a Nguni language, so it
shares the noun-class concord challenge with isiZulu; see [`zu/style.md`](../zu/style.md) for the shared reasoning.

## Voice and tone

Friendly, concise, active, and never alarmist, matching Cmdr's English voice. No isiXhosa-specific vendor style guide is
in the pile, but the sibling isiZulu Microsoft guide calls for a warm, relaxed, conversational, less-formal voice, which
fits Cmdr and is a reasonable Nguni-family anchor. Keep error and crash copy calm and factual.

## Formality

isiXhosa has no European-style T/V pronoun split. Use a direct, respectful second-person register (as Microsoft
recommends for the sibling isiZulu: address the user as "you", avoid impersonal third-person "the user"), with plain
imperative verb labels for UI actions ("Vula" Open, "Cima" Delete/Cancel). Confirm with David / a native reviewer if a
more honorific register is wanted.

## Decision points

### Noun-class agreement and the placeholder problem (load-bearing)

- The choice: isiXhosa is a Bantu noun-class language; verbs, adjectives, and possessives take concord prefixes agreeing
  with the noun's class, and a `{name}`/`{path}` insert's class is unknown at write time. Same problem as isiZulu.
- Majors: intrinsic to all Nguni-language localization; vendors phrase around it with class-neutral framings.
- Recommendation: write templates so no agreement morpheme depends on an inserted value; keep inserts in
  agreement-neutral slots (label + colon + value, or a fixed carrier noun owning the concord). This is the top
  blind-translation risk for isiXhosa. Full reasoning in [`zu/style.md`](../zu/style.md) § noun-class. Flag to David:
  some English source templates may need rewording to translate cleanly.
- Confidence: high.

### Click consonants and orthography

- The choice: isiXhosa orthography uses the Latin letters c, q, x to spell its three click series (plus digraphs), so
  ordinary-looking ASCII letters carry click values; spelling must be exact (e.g. `Cima` Delete uses the dental click
  letter c). It also uses circumflex/diaeresis in a few words.
- Majors: GNOME and Microsoft both use standard Latin isiXhosa orthography (verified: `Cima` Delete, `ifayile` File,
  `ifolda` Folder, `kopa` Copy, reference pile, 2026-06-19).
- Recommendation: follow standard isiXhosa Latin orthography exactly; the click letters are plain ASCII so no special
  font work is needed, but a translator must not "correct" c/q/x spellings that look unusual to an English eye.
- Confidence: high.

### Loanword vs native term

- The choice: computing nouns are largely borrowed-respelled in isiXhosa (`ifayile` file, `ifolda` folder).
- Majors: GNOME and Microsoft agree on borrowed forms here (`ifayile`, `ifolda`, reference pile 2026-06-19), so unlike
  isiZulu there's no folder-term conflict.
- Recommendation: use the borrowed-respelled forms both sources share; they match user expectation. Record each in the
  glossary.
- Confidence: high.

### Region / variant

- The choice: the pile has `xh` (GNOME) and `xh-ZA` (Microsoft); isiXhosa is overwhelmingly South African.
- Recommendation: ship a single `xh` base; don't split a region variant.
- Confidence: high.

### Gender / inclusive language

- The choice: isiXhosa has no grammatical gender and no gendered personal pronouns, so the gendered-language inclusivity
  problem mostly doesn't arise.
- Recommendation: no special handling needed.
- Confidence: high.

## Terminology and glossary

| English term  | Xhosa          | Notes                                     |
| ------------- | -------------- | ----------------------------------------- |
| File          | ifayile        | borrowed-respelled; MS terminology (high) |
| Folder        | ifolda         | GNOME + MS agree (high)                   |
| Copy          | kopa           | MS terminology (high)                     |
| Delete        | cima           | MS terminology + GNOME agree (high)       |
| Open          | vula           | GNOME (high)                              |
| Rename        | thiya kwakhona | GNOME (high)                              |
| Cancel        | rhoxisa        | GNOME (high)                              |
| Move to Trash | yisa emgqomeni | GNOME (high)                              |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. The `{email}`-style
placeholder tokens are also verbatim.

## Plurals

isiXhosa CLDR plural categories: `one`, `other` (run `new Intl.PluralRules('xh').resolvedOptions().pluralCategories` to
confirm; matches the GNOME 2-form header). isiXhosa treats 0 as `one`. Plurals change the noun-class prefix (singular vs
plural class), not just a suffix, so write both branches as full native forms.

## Notes and decisions

- **Apostrophes in ICU**: double every apostrophe in ICU strings; normal apostrophes in `errors.*`.
- **Capitalization**: don't force English title-case onto isiXhosa nouns whose lowercase class prefix should stay
  lowercase.
- **Click letters**: c, q, x are correct as written; don't "fix" them.

## Decisions to confirm with David

- Whether any English sentence template needs rewording so its `{name}`/`{path}` insert avoids breaking noun-class
  concord (shared with isiZulu).
- Native isiXhosa review for the agreement-restructured sentences and any file-manager term not covered by GNOME/MS
  (pane, tab, transfer).

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/xh/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
