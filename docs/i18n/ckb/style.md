# Central Kurdish / Sorani (ckb) translation style guide

Working notes for translating Cmdr into Central Kurdish (Sorani, کوردیی ناوەندی). Read [`README.md`](../README.md) for
how this fits the translation process.

`ckb` is the language base, written in the Sorani Perso-Arabic script and laid out right-to-left (RTL). The reference
pile has GNOME nautilus and Microsoft terminology for `ckb`; no macOS (Apple does not ship a Sorani macOS UI).

## Voice and tone

Friendly, concise, active, calm, never alarmist. Match the English register where the language allows. Keep error and
crash copy reassuring and factual; never use the bare labels "error"/"failed". Lean on the GNOME Sorani catalog and
Microsoft Sorani terminology for established UI phrasing.

## Formality

**Use the polite/standard register, recommended, with native review.** Kurdish has informal and polite second-person
forms (the polite/plural "ئێوە" vs informal "تۆ"). Software UI conventionally uses the polite form. Recommendation:
polite register. Confidence: medium-high; the exact verb forms and pronoun choice need a native reviewer, since Sorani
software-localization norms are less codified than the majors'. Apply consistently.

## Decision points

The defining decisions are script direction (RTL) and the Perso-Arabic orthography. This is the only RTL language in
this batch, so the layout work matters more than usual.

- **Script: Sorani Perso-Arabic, RTL (`ckb` base), settled.** Sorani Central Kurdish is written in a modified
  Perso-Arabic alphabet, right-to-left. (Northern Kurdish/Kurmanji uses Latin and is a different language, `ku`/`kmr`,
  not this locale.) Microsoft and GNOME both localize Sorani in the Perso-Arabic RTL form (verified: the `ckb` MS
  terminology and GNOME catalogs are Perso-Arabic, 2026-06-20; e.g. "folder" → "فۆڵدەر"). Recommendation: Perso-Arabic
  RTL. Confidence: confirmed.
- **RTL layout is the single biggest technical risk, and it's a code concern, not just translation.** Before any Sorani
  ship, verify in the actual app:
  - The two-pane layout, panes, and reading order mirror correctly under RTL (the directory listing, the cursor, the
    active-pane highlight, breadcrumbs, and any left/right affordances).
  - Bidi handling where RTL Sorani text sits next to LTR runs: file paths, brand words (Cmdr, macOS, GitHub), numbers,
    and extensions stay legible and don't reorder confusingly. Wrap LTR tokens so they don't scramble the surrounding
    RTL sentence.
  - Icons and directional controls (back/forward, "move to other pane") read correctly mirrored. This is a prerequisite,
    not a translation step. Flag for David: does Cmdr's layout currently support RTL mirroring at all? If not, Sorani
    (and any future RTL locale) needs that engineering before it can ship. Confidence: confirmed that RTL support is
    required; unknown whether the app has it.
- **Regional/dialect variant: target standard Sorani, no split.** Sorani has sub-dialects, but the written standard
  (centered on the Sulaymaniyah/Erbil literary norm) is what's localized. No product-level region split. Confidence:
  high.
- **No grammatical gender in Sorani.** Central Kurdish has largely lost grammatical gender (unlike Kurmanji), so
  gender-agreement traps don't apply to the user address. Confidence: high.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Sources for `ckb`: Microsoft terminology (Tier 2) and GNOME
nautilus (Tier 3).

- folder → فۆڵدەر · MS terminology ("folder" → "فۆڵدەر") · high
- file → پەڕگە / فایل · GNOME / MS · tentative, native reviewer to pick the preferred form
- copy → ڕوونووسکردن / لەبەرگرتنەوە · GNOME · tentative
- delete → سڕینەوە · GNOME / MS · high
- search → گەڕان · GNOME / MS · high
- settings → ڕێکخستنەکان · GNOME / MS · high

Populate fully from `ckb/microsoft-terminology/` and `ckb/gnome-nautilus/` during translation; mark anything a single
source supports as `tentative` pending native review.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. These are
LTR Latin tokens inside RTL runs: wrap them so bidi doesn't reorder them, and verify they render correctly mid-sentence.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('ckb')`, 2026-06-20). Two branches. Phrase counted
strings to read correctly for one vs many; a native reviewer confirms number-noun agreement.

## Notes and decisions

- **Digits**: Sorani may use Eastern Arabic-Indic digits (۰۱۲…) or Western digits depending on context; let the
  formatter layer decide, don't hardcode.
- **Numbers and dates come from the formatter layer.** Never hardcode separators or direction.
- **Punctuation**: RTL Arabic-script punctuation (e.g. the Arabic comma ، and question mark ؟) may apply; a native
  reviewer handles this. Don't impose Latin punctuation.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim, and be
  careful that bidi reordering doesn't visually detach a placeholder from its sentence. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **RTL layout support is a prerequisite.** Confirm whether Cmdr's UI can mirror for RTL today. If not, Sorani can't
  ship until the app handles RTL layout, bidi, and mirrored controls. This is the gating call. Flag for David.
- **Register and several core terms** need a native Sorani reviewer; Sorani software norms are less codified than the
  majors', so confidence on phrasing is lower than for the well-resourced languages.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ckb/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
