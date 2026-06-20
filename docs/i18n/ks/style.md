# Kashmiri (ks) translation style guide

Working notes for translating Cmdr into Kashmiri (कॉशुर / کٲشُر). Read [`README.md`](../README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes
carry into Kashmiri.

Sourced: the pile has two MS style guides, one per script: Perso-Arabic (`_ignored/i18n/ks-Arab/`) and Devanagari
(`_ignored/i18n/ks-Deva/`); no macOS, no GNOME, no MS terminology, no Xfce. MS style guides are the only authority.
Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

- **Script: Perso-Arabic vs Devanagari, and it changes text DIRECTION (the single biggest decision, high).** Kashmiri
  is officially written in two scripts: the Perso-Arabic (Nastaʿliq) script, which is the script of the official
  language of Jammu & Kashmir and is **right-to-left (RTL)**; and Devanagari, **left-to-right (LTR)**, used by part of
  the community. Microsoft maintains a separate style guide for each (verified 2026-06-20). This is not a font choice:
  it flips layout direction, punctuation forms, and alignment. Recommendation: pick ONE script to ship first
  (Perso-Arabic `ks-Arab` is the official-status default; Devanagari `ks-Deva` is the alternative), and treat the other
  as a separate sibling locale if ever wanted, never a within-locale toggle. **David must choose the shipped script.**
- **RTL is a layout decision, not just a string decision (high).** If Perso-Arabic is chosen, the whole UI for this
  locale is RTL: pane order, alignment, icon mirroring, progress direction. Cmdr's frontend must support `dir="rtl"`
  for this locale, or the strings render in a broken LTR frame. Flag whether RTL layout is in scope before committing to
  `ks-Arab`. (Other RTL locales in the broader set, such as Arabic, Hebrew, and Urdu, share this need, so it may already be
  planned.)
- **Very low-resource: no terminology source at all.** No macOS, GNOME, MS terminology, or file-manager catalog. Every
  core file-manager term needs native coinage/review; the MS style guides give tone, punctuation, and grammar, not a
  term list. Expect the whole glossary to be `tentative` until a native reviewer fills it.

## Voice and tone

Friendly, concise, active, calm, never alarmist. Follow the Microsoft Kashmiri voice for the chosen script:
conversational and respectful. Error messages stay calm and actionable: name the problem and the next step, and avoid a
bare "error/failed" status label, consistent with Cmdr's English voice. A native reviewer carries the register since no
file-manager catalog exists to anchor phrasing.

## Formality

- **Polite second person.** Kashmiri distinguishes polite from familiar second-person address; software uses the polite
  form. Recommended default: polite register throughout. Confidence: medium; a native reviewer confirms the exact
  pronoun, which differs in feel between the two script communities.
- **Action labels (buttons, menu items): short imperative form, per the chosen script's conventions.** No catalog to
  anchor verb labels; a native reviewer sets them. Confidence: low-medium pending native input.

## Decision points

- **Script + direction: Perso-Arabic (RTL) vs Devanagari (LTR) (the key decision; see Decisions to confirm).** Pick one
  shipped script. This decides text direction, punctuation, and capitalization behavior. Confidence: high that the
  decision matters; David picks.
- **Capitalization:**
  - Perso-Arabic: **not applicable**, since the Arabic script has a single form per letter and no case (MS Kashmiri
    Perso-Arabic states capitalization does not apply, verified 2026-06-20).
  - Devanagari: also caseless. Either way, there is no title-case-vs-sentence-case choice. Confidence: confirmed.
- **Punctuation differs by script (high, Perso-Arabic).** In Perso-Arabic Kashmiri the punctuation marks themselves
  differ and are RTL-oriented: the full stop is `۔` (U+06D4), not the Latin `.`; MS warns that mixing Latin punctuation
  into the RTL text "corrupts the whole text" (verified 2026-06-20). Use the script's native punctuation throughout;
  don't paste Latin `.`/`,`/`?` into Perso-Arabic strings. Devanagari Kashmiri follows Devanagari/Hindi punctuation
  conventions (e.g. period after abbreviations), per MS (verified 2026-06-20). Confidence: high.
- **Regional variant: one community language, two scripts (not a region split).** The axis is script, not region. Don't
  build a `ks-IN` vs anything matrix; build at most `ks-Arab` and `ks-Deva` as separate locales. Confidence: high.
- **Gender / inclusive language (medium-high problem).** Kashmiri has grammatical gender and gendered verb agreement,
  which can expose the subject's gender in sentences addressed to the user. Where it would, **rewrite impersonally**. A
  native reviewer handles agreement. Confidence: medium-high.
- **Placeholder grammar (high).** Kashmiri uses case marking and (in Perso-Arabic) RTL embedding. A `{path}`/`{name}`
  placeholder carrying LTR text (a Latin path) embedded in RTL Kashmiri needs correct bidi handling so it doesn't
  scramble the line. Structure sentences to isolate the placeholder, and rely on Unicode bidi isolation at render time.
  Confidence: high; the subtlest concern, especially for `ks-Arab`.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. No macOS, GNOME, or MS terminology for Kashmiri, only MS style
guides (tone/grammar, not a term list). So the file-manager glossary starts essentially empty and **every term needs a
native reviewer**; mark all `tentative` until then. Write terms in the chosen shipped script. Where helpful, a native
reviewer may borrow established Urdu (for Perso-Arabic) or Hindi (for Devanagari) file-manager terms as a starting
point, then adapt to Kashmiri.

- **file, folder, trash, copy, open, cancel, delete, rename, eject, volume, pane, tab** · no source in the pile;
  native coinage/review required. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. In `ks-Arab` (RTL), a Latin brand name is an LTR run embedded in RTL text;
rely on bidi isolation so it renders correctly.

## Plurals

CLDR categories for `ks`: `one`, `other` (verified with `new Intl.PluralRules('ks')`, `'ks-Arab'`, and `'ks-Deva'` all
return `one, other`, 2026-06-20). Write both.

- **one**: integer 1. "1 file" (in the chosen script).
- **other**: everything else, including 0 and counts ≥ 2. A native reviewer settles whether and how the counted noun
  pluralizes (Kashmiri marks number and gender on nouns). The `desktop-i18n-plural` check requires both.

## Notes and decisions

- **Direction:** if `ks-Arab` is shipped, the locale is RTL end to end (see Decisions to confirm). Punctuation uses the
  Perso-Arabic marks (`۔` full stop, etc.), not Latin.
- **Quotation marks:** Perso-Arabic Kashmiri uses the Arabic-script conventions (often guillemets `«…»` or the
  script's quotation forms); Devanagari follows Devanagari conventions. A native reviewer settles each.
- **Numbers and dates come from the formatter layer.** Perso-Arabic Kashmiri may use Eastern Arabic-Indic digits;
  `formatNumber()`/`formatBytes()` follow the locale. Never hardcode separators or digit forms in a string.
- **Length and height.** Nastaʿliq (Perso-Arabic) renders with steep diagonal baselines and needs vertical room and a
  Nastaʿliq-capable font; overflow-check both axes against the pseudolocale (`en-XA`) and a real Kashmiri font.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/ks/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
