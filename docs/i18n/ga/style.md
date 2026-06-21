# Irish (ga) translation style guide

Working notes for translating Cmdr into Irish (Gaeilge). Read [`README.md`](../README.md) for how this fits the
translation process.

This is the language base (`ga`), the universal Irish set. Modern written Irish uses the official standard (An Caighdeán
Oifigiúil); the three dialects (Connacht, Munster, Ulster) differ in speech but the Caighdeán is the UI norm, so no
region variant is needed. The pile carries a `ga-IE` folder for Microsoft sources but it adds nothing over `ga`.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Microsoft's Irish
style guide aims for warm, conversational, direct address. Keep error and crash copy reassuring and factual.

## Formality

**Address the user directly with the singular "tú" (informal), recommended, high confidence.** Irish distinguishes
singular "tú" from plural/polite "sibh", but Irish software does not use a French-style polite-plural for a single user:
imperatives address one person. Microsoft's Irish style guide says to "address the user as you, directly... avoid
third-person references, such as 'user', as they sound formal and impersonal". So use the singular imperative for
actions and tú-address for direct sentences. Confidence: high (Microsoft style guide).

**Imperatives for UI actions** (buttons, menu items): use the imperative (the bare verb stem), matching the GNOME and
Microsoft catalogs: "Cóipeáil" (copy), "Scrios" (delete), "Cealaigh" (cancel). Irish forms the imperative this way; it
agrees with the singular address above.

## Decision points

**Coverage is good.** GNOME, Microsoft terminology, and the Microsoft style guide all exist for Irish (verified
2026-06-20); macOS is missing (Apple does not localize macOS into Irish, so the user's Finder chrome is in English).
Confidence: confirmed.

- **Script: Latin only, no decision.** Modern Irish is written in the Latin alphabet (the old Gaelic/Cló Gaelach script
  is historical only). Use standard Latin orthography. Confidence: high.
- **Initial mutations (séimhiú / urú) are the defining Irish difficulty.** Irish mutates word-initial consonants:
  lenition ("séimhiú", shown by an inserted h: "comhad" → "do chomhad") and eclipsis ("urú": "i gComhad"). The
  triggering word and grammatical context decide it, so a `{filename}`/`{name}` placeholder inserted after a
  mutation-triggering word should mutate, but the catalog can't mutate runtime text. Structure sentences so a
  placeholder sits where no mutation is required, or where leaving it unmutated still reads acceptably. Never assemble
  an Irish phrase by gluing fragments without checking the mutation at each join. Confidence: confirmed (Irish grammar):
  the single biggest blind-translation risk for this language.
- **Plurals: FIVE CLDR categories.** See Plurals below; flagged here because it's a genuine difficulty, not a footnote.
  Irish counted nouns also interact with mutation and the genitive, so the noun form per branch matters.
- **Gender: Irish has grammatical gender** (masculine/feminine), which triggers mutations and article forms, but direct
  tú-address does not gender the user. No inclusive-form debate exists. Recommendation: direct address, neutral nouns.
  Confidence: high.
- **Length: Irish runs longer than English** (often 20–30%+, partly from the verbal-noun constructions). Overflow-check
  tight buttons against the pseudolocale (`en-XA`). Confidence: high.

## Terminology and glossary

| English term | Irish          | Notes                            |
| ------------ | -------------- | -------------------------------- |
| Copy         | Cóipeáil       | Microsoft + GNOME ("\_Cóipeáil") |
| Move         | Bog            | GNOME (confirm against catalog)  |
| Delete       | Scrios         | Microsoft + GNOME ("\_Scrios")   |
| Cancel       | Cealaigh       | Microsoft + GNOME ("\_Cealaigh") |
| file         | comhad         | Microsoft + GNOME                |
| folder       | fillteán       | Microsoft + GNOME                |
| trash        | Bosca Bruscair | GNOME (lit. "rubbish box")       |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Irish CLDR categories: `one`, `two`, `few`, `many`, `other` (verified with `new Intl.PluralRules('ga')`, 2026-06-20;
GNOME nautilus uses a matching 5-form rule `nplurals=5; plural=n==1?0:n==2?1:n<7?2:n<11?3:4`). This is one of the
hardest plural languages in Cmdr's set: every counted ICU message must write all five branches, and Irish counted nouns
interact with mutation and number, so get the noun form right in each branch, not just the count. The
`desktop-i18n-plural` check enforces coverage. Confidence: confirmed.

## Notes and decisions

- **Diacritics**: Irish uses the acute accent (síneadh fada: á é í ó ú). It changes meaning and is mandatory; keep it.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Priority: Irish is a high-effort locale** (five plural forms, initial mutations). Better-anchored than the truly
  thin languages, but confirm it's worth the effort before lower-priority locales.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ga/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
