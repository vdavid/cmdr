# Welsh (cy) translation style guide

Working notes for translating Cmdr into Welsh. Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`cy`), the universal Welsh set. Welsh is a single written standard for UI purposes; no region
variant is needed (the pile carries a `cy-GB` folder, but it adds nothing over `cy`).

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Welsh UI in the wild
(Welsh Government services, GNOME) is warm and plain. Keep error and crash copy reassuring and factual.

## Formality

**Use "chi" (the polite second person), recommended, flag for David.** Welsh has a T-V distinction: informal/singular
"ti" vs polite/plural "chi". For software addressing an unknown adult, "chi" is the safe, near-universal Welsh UI
register (it parallels French "vous"): respectful without being cold. "ti" would feel over-familiar for a file manager.
Welsh Government bilingual guidance and most public-facing Welsh software default to "chi". Confidence: high; flag for
David only because Cmdr's warm, David-signed voice is the one argument for "ti". Whatever is chosen, apply it
consistently, and note that ti/chi affects more than pronouns: verb forms and possessive constructions change with it.

**Imperatives for UI actions** (buttons, menu items): use the imperative, matching the GNOME Welsh catalog ("Copïo",
"Dileu", "Diddymu", "Symud"). The imperative form itself agrees with the ti/chi choice (the chi-imperative vs the
ti-imperative differ), so pick the formality first.

## Decision points

**Major-product coverage is thin, this is itself the headline finding.** The reference pile has only
`cy/gnome-nautilus/` (Tier 3); there is no macOS, no Microsoft terminology, and no Microsoft style guide for Welsh
(verified 2026-06-20). Apple does not localize macOS into Welsh; Microsoft's Welsh support is limited; Google offers
Welsh in a few products. So a Welsh Cmdr user's OS chrome is in English. The authoritative anchors are the GNOME/Xfce
file-manager catalogs plus Welsh Government bilingual-style conventions (worth a web check when translating). Treat
Welsh as a lower-priority, less-anchored locale and lean on the GNOME catalog for file-manager terms. Confidence:
confirmed.

- **Initial consonant mutation is the defining Welsh difficulty.** Welsh mutates word-initial consonants (soft / nasal /
  aspirate) depending on the preceding word and grammatical context: "Caerdydd" → "yng Nghaerdydd", "tad" → "fy nhad".
  This breaks naive string concatenation: a `{name}` / `{filename}` placeholder inserted after a word that triggers
  mutation should mutate, but the catalog can't mutate runtime text. Structure sentences so a placeholder sits where no
  mutation is required, or where leaving it unmutated still reads acceptably. Never assemble a Welsh phrase by gluing
  fragments without checking the mutation at each join. Confidence: confirmed (Welsh grammar) - the single biggest
  blind-translation risk for this language.
- **Plurals are the most complex in this whole language set: SIX CLDR categories.** See Plurals below; flagged here
  because it's a genuine decision-shaped difficulty, not a footnote.
- **Gender: Welsh has grammatical gender (masculine/feminine) on nouns**, which triggers some mutations and adjective
  forms, but it does not gender the person addressed, so direct chi/ti-address avoids gendering the user. No
  inclusive-ending debate like Spanish's exists. Recommendation: direct address, neutral nouns. Confidence: high.
- **Length: Welsh runs noticeably longer than English** (often 20–30%+). Overflow-check tight buttons against the
  pseudolocale (`en-XA`). Confidence: high.

## Terminology and glossary

| English term | Welsh     | Notes                                                      |
| ------------ | --------- | ---------------------------------------------------------- |
| Copy         | Copïo     | GNOME ("\_Copïo Yma" = Copy Here); note the diaeresis on ï |
| Move         | Symud     | GNOME ("\_Symud Yma")                                      |
| Delete       | Dileu     | GNOME                                                      |
| Cancel       | Diddymu   | GNOME                                                      |
| file         | ffeil     | GNOME ("Copïo ffeiliau")                                   |
| folder       | ffolder   | confirm against GNOME                                      |
| trash        | (confirm) | check GNOME nautilus for the Welsh trash term              |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Welsh CLDR categories: `zero`, `one`, `two`, `few`, `many`, `other`, all six (verified with
`new Intl.PluralRules('cy')`, 2026-06-20; GNOME nautilus uses a 4-form rule
`nplurals=4; plural=(n==1)?1:(n==2)?2: (n==8||n==11)?3:0`). This is the hardest plural language in Cmdr's set: every
counted ICU message must write all six branches the language needs, and "few"/"many" trigger different noun forms (the
noun stays singular after a numeral in Welsh: "5 ffeil", not "5 ffeiliau"). The `desktop-i18n-plural` check enforces
coverage; get the noun form right in each branch, not just the count. Confidence: confirmed.

## Notes and decisions

- **Diacritics**: Welsh uses the circumflex (â ê î ô û ŵ ŷ) and occasionally the diaeresis (ï). These are meaningful;
  keep them.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values (Welsh uses apostrophes in contractions like "i'r", "a'r", so
  this matters a lot here); keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Formality: chi (polite, recommended) vs ti (informal).** chi is the safe public-software default; ti only if David
  wants maximum warmth. Choice cascades into verb and imperative forms, so settle it first.
- **Priority: Welsh is a thin-coverage, high-effort locale** (six plural forms, mutation). Confirm it's worth doing
  before the other, better-anchored languages.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/cy/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
