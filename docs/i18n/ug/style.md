# Uyghur (ug) translation style guide

Working notes for translating Cmdr into Uyghur. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Uyghur.

Uyghur (`ug`) is a Turkic, agglutinative language. The BCP-47 base tag `ug` resolves in CLDR to `ug-Arab-CN`: the
Perso-Arabic Uyghur Ereb Yéziqi (UEY), written right to left. That is the script Cmdr should target (see Decision
points).

## Priority signal: low, and politically sensitive

Be honest about where this sits. Uyghur software localization is sparse, and Uyghur is a minority language under
political pressure in its home region. The evidence base for Cmdr is thinner than for any European locale:

- **No macOS UI.** Apple does not ship Uyghur as a system language, so there is no Tier-1 Finder/AppKit evidence (the
  strongest source for every other locale). Verified: no `macOS/` dir in `_ignored/i18n/ug/`, 2026-06-20.
- **No Microsoft style guide.** There is no `microsoft-style-guides/` PDF for Uyghur, so there is no vendor guidance on
  formality or addressing the user.
- **What does exist:** a Microsoft terminology glossary (`microsoft-terminology/UYGHUR.tbx`, 12,223 entries, Arabic
  script), and two file-manager GNU gettext catalogs (GNOME Nautilus and Xfce Thunar). The GNOME catalog is maintained
  by the Uyghur Computer Science Association (UKIJ), in Arabic script. These are the term evidence; there is no
  authoritative tone/formality source, so the register calls below lean on Turkic-language norms plus the file-manager
  catalogs, not on a vendor style guide.

Treat this locale as a candidate that needs a native reviewer before shipping more than it needs review elsewhere. Many
calls below are tentative by necessity.

## Voice and tone

Friendly, concise, active, and calm, same as Cmdr's English voice. Uyghur is agglutinative, so a single inflected verb
often carries what English spreads across several words; prefer the natural single verb over a padded phrase.

Error messages stay calm and actionable and never read as a bare "error" or "failed" label: state what happened and the
next step. There is no macOS Uyghur precedent to copy here, so apply Cmdr's English error rule directly.

## Formality

Uyghur has a T/V-style second-person distinction: informal `sen` (سەن) versus polite/formal `siz` (سىز), with matching
verb agreement and the polite plural/honorific imperative suffix `-ng` / `-ingiz`.

- **Recommended default: polite `siz` register, but keep direct address light.** Software and formal writing lean
  polite; `sen` reads as familiar/intimate and risks sounding curt in UI. This matches how the GNOME/Xfce catalogs
  phrase prompts. Confidence: tentative (no vendor style guide exists for Uyghur; this is a Turkic-software-norm call,
  not a sourced one). Flag for David / a native reviewer.
- **Buttons and menu items: imperative.** The file-manager catalogs use bare imperative verbs for actions, for example
  Open = `ئاچ` (ach), Paste = `چاپلا` (chapla), Rename = `ئات ئۆزگەرت` (at özgert), Cancel = `ۋاز كەچ` (waz kech). Use
  the short imperative for action labels rather than a polite long form; reserve the polite register for sentences that
  address the user ("Do you want to …").
- **No grammatical gender.** Uyghur, like all Turkic languages, has no grammatical gender and no gendered third-person
  pronoun (`u` covers he/she/it). This removes a whole class of agreement problems present in the Romance/Germanic
  locales: no gendered agent nouns, no gender-star forms, nothing to flag. Confidence: confirmed.

## Decision points

### Script variant: target Arabic UEY (Ereb), the #1 decision

This is the dominant call for Uyghur and the one that shapes everything else.

- **Three scripts exist:** Arabic-based UEY / Ereb Yéziqi (RTL, the official standard in Xinjiang since 1982,
  Perso-Arabic, fully alphabetic with all vowels written), Latin-based ULY / Latin Yéziqi (LTR, a romanization used
  online and in academia), and Cyrillic USY / Siril Yéziqi (used by Uyghurs in Kazakhstan and Central Asia). They are
  not dialects: they are three orthographies for the same language, and a reader of one does not automatically read
  another.
- **How the majors handle it:** there is no Apple or Microsoft-product Uyghur UI to mirror. The two reference points
  that exist (Microsoft terminology, GNOME via UKIJ) are both Arabic-script UEY, and CLDR's `ug` resolves to
  `ug-Arab-CN`. So every authoritative artifact that does exist is Arabic-script.
- **Recommendation: ship Arabic UEY under the base tag `ug`.** It is the standard, the script of the home-region user
  base, and the only script the evidence covers. Confidence: high.
- **If a Latin or Cyrillic audience ever matters,** add them as explicit sibling tags `ug-Latn` (ULY) and `ug-Cyrl`
  (USY), never by silently swapping the base. They differ in script, direction (ULY/USY are LTR), and a full
  transliteration, so they are separate catalogs, not a variant of `ug`. Don't attempt them without that audience and a
  native reviewer. Confidence: high (that they must be separate); the question of whether to ever do them is a David
  call.

### RTL layout and bidirectional text

Arabic UEY is right to left, so this locale carries the same bidi surface as Arabic and Hebrew.

- **Mirror the layout.** Two-pane orientation, alignment, progress direction, chevrons/back-forward affordances, and
  reading order flip for RTL. This is a frontend layout concern (`dir="rtl"`), not something a translator encodes in
  strings, but the translator must write copy that reads correctly once mirrored.
- **Numbers and embedded LTR runs stay LTR inside RTL text.** File paths, brand names (Cmdr, macOS, SMB, MTP, GitHub),
  byte counts, and digits are left-to-right runs embedded in a right-to-left sentence. Without isolation they reorder
  visibly (a path's slashes and segments scramble, a "12 files" count detaches from its noun).
- **Use Unicode isolates/marks around uncontrolled LTR inserts.** Wrap a raw `{path}`, `{name}`, or a brand token in
  bidi isolation (LRM/RLM or, preferably, the isolate characters) so an arbitrary inserted value can't break sentence
  order. The English string can't carry these; the Uyghur translation must add them around the relevant placeholders.
  This is the single highest bidi risk for a file manager, whose strings are full of paths and counts. Confidence: high.
- **Punctuation:** Arabic-script Uyghur may use the Arabic comma (`،`) and Arabic question mark (`؟`) in running text;
  the GNOME catalog mixes ASCII and Arabic punctuation. Pick one convention per the reviewer; this is cosmetic, flag it
  but don't block on it. Confidence: tentative.

### Numerals: Western (European) digits 0-9, not Eastern Arabic-Indic

Counterintuitive but evidenced: Uyghur Arabic script uses Western digits.

- **CLDR default for `ug` is `latn`** (European 0-9), with `arabext` (extended Arabic-Indic ۰-۹) only as the native
  alternative, not the default. Verified on CLDR 44 locale summary for `ug`, 2026-06-20.
- **Recommendation: use Western digits 0-9** in counts, sizes, and dates, matching CLDR. Don't substitute Eastern
  Arabic-Indic digits. Note they remain LTR runs inside the RTL text (see RTL above). Confidence: high.
- Decimal/grouping separators: CLDR data is thin in the summary; default to the locale's CLDR separators rather than
  hardcoding. Flag for a reviewer if a user-facing number looks wrong. Confidence: tentative.

## Terminology and glossary

Not required for this pass. A few anchor terms verified across the Microsoft terminology glossary and the GNOME Nautilus
catalog (both Arabic UEY), recorded so the next translator inherits them:

| English term | Uyghur (UEY) | Notes                                                |
| ------------ | ------------ | ---------------------------------------------------- |
| Folder       | قىسقۇچ       | MS terminology + GNOME agree. Confidence: high.      |
| File         | ھۆججەت       | GNOME. Confidence: high.                             |
| Copy         | كۆچۈر        | GNOME. Confidence: high.                             |
| Open         | ئاچ          | GNOME, imperative. Confidence: high.                 |
| Paste        | چاپلا        | GNOME, imperative. Confidence: high.                 |
| Rename       | ئات ئۆزگەرت  | GNOME, imperative. Confidence: high.                 |
| Cancel       | ۋاز كەچ      | GNOME, imperative. Confidence: high.                 |
| Trash        | ئەخلەتخانا   | GNOME (literally "garbage place"). Confidence: high. |
| Properties   | خاسلىق       | GNOME. Confidence: high.                             |

Cmdr-specific terms (pane, tab, volume, listing, transfer, viewer) have no settled Uyghur evidence and need a native
reviewer; leave them for the translation pass and record each choice here as it is made.

## Brand and do-not-translate

Keep verbatim, same as every locale: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the system
placeholder tokens. In RTL text these are LTR runs and must be bidi-isolated (see RTL above). The list is enforced by
`desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

CLDR plural categories for `ug`: **one, other** (verified:
`new Intl.PluralRules('ug').resolvedOptions().pluralCategories`, 2026-06-20). Every plural message must cover both
branches.

- Like other Turkic languages, Uyghur does not inflect the counted noun for plurality after a number (the number itself
  carries plurality), so the `one` and `other` forms are often identical or differ only in the surrounding sentence.
  Write both branches anyway; the `desktop-i18n-plural` check requires `one` and `other` to be present, and authoring
  both keeps the message honest if the surrounding phrasing does differ.
- Evidence note: the GNOME Nautilus catalog declares `nplurals=1` (a single form), while Xfce Thunar declares
  `nplurals=2; plural=(n != 1)`. CLDR's two-category `one`/`other` is the authority for Cmdr; the GNOME single-form
  header reflects an older simplification, not a reason to drop a branch.

## Notes and decisions

### Decisions to confirm with David / a native reviewer

- **Formality register (`siz` vs `sen`).** Recommended `siz` (polite) by Turkic-software norm, but there is no vendor
  style guide for Uyghur to confirm it. Tentative; needs a native call.
- **Whether to ship Uyghur at all, given the low-priority and politically sensitive signal.** This is a product call,
  not a translation call.
- **Arabic vs ASCII punctuation** (`،`/`؟` vs `,`/`?`) in running text. Cosmetic; pick one with the reviewer.
- **Latin (`ug-Latn`) or Cyrillic (`ug-Cyrl`) editions:** only if a specific audience needs them, and only as separate
  sibling catalogs. Not part of `ug`.

### Layout

Uyghur word length is roughly comparable to or shorter than English for action labels (single agglutinated verbs), so
overflow is less of a risk than German. The real layout risk is RTL mirroring and bidi correctness, covered above, not
string length. Overflow-check against the pseudolocale as usual.

### ICU mechanics

Catalog-level, not Uyghur-specific, but easy to miss: double every apostrophe in a value (`'` becomes `''`; ICU treats a
lone `'` as an escape and silently swallows text), and keep every `{placeholder}` and `<tag>` verbatim. For Uyghur,
remember that bidi isolation marks go AROUND a placeholder, not inside its braces. Full rules: the agent-handoff block
in [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
`apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ug/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
