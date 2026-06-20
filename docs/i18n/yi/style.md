# Yiddish (yi) translation style guide

Working notes for translating Cmdr into Yiddish. Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`yi`), Yiddish written in the Hebrew script and laid out right-to-left. It's a low-resource,
RTL target: treat the layout work as part of the translation, not an afterthought.

## Voice and tone

Friendly, concise, active, and never alarmist, matching Cmdr's English voice. Yiddish has no large body of modern
software UI to anchor a "house tone", so aim for plain, warm, modern Yiddish (the register of contemporary Yiddish
press and digital media), not liturgical or heavily Germanic-archaic phrasing. Keep error and crash copy calm and
factual; avoid dramatizing words, same as English.

## Formality

Yiddish has a T/V distinction: informal **דו** (du) vs polite **איר** (ir). **Use polite איר (ir)** throughout, the
safe, respectful register for software addressing an unknown adult. Keep it consistent (no mixing du/ir).

For UI action labels (buttons, menu items), prefer the bare verb / imperative form, kept short. There's no established
Yiddish software convention to copy here, so this is a default to confirm, not a settled norm.

## Decision points

### Script and right-to-left layout (load-bearing)

- The choice: Yiddish is written in the Hebrew alphabet, laid out RTL. This is not optional and not a variant; it's the
  language.
- Majors: no macOS or Microsoft Yiddish UI exists in the reference pile (no `macOS/`, no MS terminology). Hebrew (`he`)
  is the closest shipped RTL reference for layout behavior; Yiddish is rendered RTL in Unicode-correct text the same way
  (verified: the GNOME `yi/nautilus.po` strings are Hebrew-script, e.g. `נײַע פּאַפּקע` for "New Folder", reference pile,
  2026-06-19).
- Recommendation: render RTL. Cmdr's UI must mirror correctly (pane layout, icons, progress direction, chevrons). This
  is a real engineering dependency, not just a string swap, and Cmdr has shipped no RTL locale yet, so the RTL plumbing
  (CSS logical properties / `dir="rtl"`, bidi isolation of `{path}` and `{name}` inserts) is a prerequisite. Flag to
  David: Yiddish can't ship correctly until the app supports RTL end to end; until then it's a layout-blocked locale.
- Confidence: high (script), but blocked on RTL app support.

### Hebrew-script orthography: YIVO vs traditional/Hasidic spelling

- The choice: which Yiddish spelling standard to follow. YIVO (standardized, used in academia and most digital
  resources) vs traditional/Hasidic spelling (still common in religious-community publishing, differing in vowel
  pointing and some letter choices).
- Majors: none in the pile to arbitrate. The GNOME catalog uses pointed, YIVO-leaning spelling (e.g. `נײַ`, `פּינטלעך`).
- Recommendation: follow YIVO orthography, the broadest, most documented standard and the one a translation tool and
  reviewer are most likely to share. Confirm with David if the target audience leans Hasidic.
- Confidence: tentative (audience-dependent).

### Bidirectional handling of inserted Latin-script values

- The choice: `{path}`, `{name}`, brand tokens (Cmdr, macOS, SMB), numbers, and error `{message}` strings are
  Latin-script LTR runs landing inside RTL sentences. Without bidi isolation they scramble visually (a path's slashes
  and segments reorder).
- Majors: standard RTL-locale practice (Apple/Microsoft Hebrew and Arabic UIs isolate embedded LTR runs).
- Recommendation: wrap uncontrolled LTR inserts in Unicode bidi isolates (FSI…PDI / `<bdi>`) at the rendering layer so
  any `{path}` or `{message}` displays correctly regardless of content. This is an app-side fix, tied to the RTL work
  above; the translator only needs to know inserts must not be assumed to sit LTR-cleanly in the sentence.
- Confidence: high.

### Region / variant

- The choice: Yiddish has no region variants worth splitting in the pile (no `_see-also.txt`). Keep a single `yi` base.
- Recommendation: ship one `yi` base; don't create region variants.
- Confidence: high.

## Terminology and glossary

Sparse: the only authoritative source is the GNOME catalog, and it's barely translated (61 of ~1,200 strings, reference
pile 2026-06-19), so most terms below are tentative and need native review.

| English term | Yiddish | Notes |
| ------------ | ------- | ----- |
| New | נײַ | from GNOME catalog (tentative) |
| New Folder | נײַע פּאַפּקע | "folder" = פּאַפּקע (papke) in GNOME catalog (tentative) |
| Name | נאָמען | GNOME (tentative) |
| Search / Find | געפֿין | GNOME "Find" (tentative) |
| Size | גרײס | GNOME (tentative) |
| Type | טיפּ | GNOME (tentative) |
| Unknown | אומבאַקאַנט | GNOME (tentative) |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. The `{email}`-style
placeholder tokens are also verbatim. In RTL text these stay LTR and need bidi isolation (see decision point above).

## Plurals

Yiddish CLDR plural categories: `one`, `other` (run `new Intl.PluralRules('yi').resolvedOptions().pluralCategories` to
confirm; matches the GNOME 2-form rule). Cover the categories a message needs.

## Notes and decisions

- **Apostrophes in ICU**: double every apostrophe in ICU strings (everything outside `errors.*`); normal apostrophes in
  `errors.*`. Hebrew-script text rarely needs the ASCII apostrophe, but the rule still applies to any that appear.
- **Punctuation**: Yiddish uses standard Latin punctuation visually mirrored by the RTL layout engine; don't hand-flip
  punctuation in the string.

## Decisions to confirm with David

- RTL is an app-level prerequisite: Yiddish is layout-blocked until Cmdr supports RTL end to end. Ship it after the RTL
  work, not before.
- Orthography standard (YIVO vs traditional/Hasidic), audience-dependent.
- Whole glossary is tentative (one thin source); needs a native Yiddish reviewer before shipping.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/yi/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
