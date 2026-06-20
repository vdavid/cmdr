# Interlingua (ia) translation style guide

Working notes for translating Cmdr into Interlingua. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice.

## Priority: low (constructed auxiliary language)

Interlingua is a constructed international auxiliary language (IALA, 1951), built from the common Romance/Latin
vocabulary. It has NO native speakers and NO commercial product localization: no Apple macOS, no Microsoft, no Spotify,
no Netflix. The only reference is a partial GNOME Nautilus catalog (`ia/gnome-nautilus/`, ~245 translated strings). This
low-priority signal IS the finding: Interlingua is a community/hobby locale, not a market locale. Translate it only if
the community-goodwill value justifies it; it is far below the major locales.

## Voice and tone

Friendly, concise, active, calm. Interlingua reads like a simplified pan-Romance language (close to Italian/Spanish/
Latin), so a reader of any Romance language can largely follow it. Match Cmdr's English voice directly.

## Formality

**Interlingua has no T-V formality distinction.** One second-person pronoun ("tu" singular, "vos" plural); no
formal/informal split. Use imperative verbs for buttons ("Copiar", "Renominar", "Aperir", "Cancellar").

## Decision points

### Script and variants: none to decide

- Latin script, one standardized form. No regional variants, no orthography choice, no gender complications worth a
  decision block. This is by design (it is an auxiliary language built for clarity). Confidence: high.

### No grammatical gender

- Interlingua nouns and adjectives have no grammatical gender agreement. Inclusive language is a non-issue. Confidence:
  high.

### Verbs are invariant (translation is mechanical)

- Interlingua verbs do not conjugate by person; the infinitive doubles as the imperative ("Copiar" = copy / to copy).
  This makes UI-string translation unusually mechanical. Confidence: high.

## Terminology and glossary

Source: GNOME Nautilus (`ia/gnome-nautilus/`) only. `tentative` until reviewed. From the catalog: copy → Copiar, rename
→ Renominar, new folder → Nove dossier, open → Aperir, cancel → Cancellar, files → Files. Likely (Romance-transparent,
confirm against the catalog): move → Mover, delete → Deler/Eliminar, search → Cercar, trash → Corbe (a papiro), folder →
Dossier, file → File.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories: `one`, `other` (verified `new Intl.PluralRules('ia')`, 2026-06-20). Regular plural in `-s`/`-es`; write
both branches.

## Notes and decisions

- **Length:** comparable to Italian/Spanish (~10–20% longer than English). Overflow-check against `en-XA`.
- **Numbers/dates** come from the formatter layer.

## Decisions to confirm with David

- **Ship at all?** Interlingua is a constructed language with no commercial localization and a tiny community. Lowest
  priority; this is David's call, not a translation decision.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ia/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
