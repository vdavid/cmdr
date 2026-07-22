# Interlingue / Occidental (ie) translation style guide

Working notes for translating Cmdr into Interlingue (historically Occidental). Read `../README.md` for how this fits the
translation process, and the app-wide `docs/style-guide.md` for the English voice.

## Priority: low (constructed auxiliary language)

Interlingue (Occidental, Edgar de Wahl, 1922) is a constructed international auxiliary language with NO native speakers
and NO commercial product localization (no Apple, Microsoft, Spotify, Netflix). References are partial GNOME Nautilus
(~894 strings) and Xfce Thunar catalogs only (`ie/gnome-nautilus/`, `ie/xfce-thunar/`). This low-priority signal IS the
finding: a hobby/community locale, far below the major locales. Translate only for community goodwill.

## Voice and tone

Friendly, concise, active, calm. Interlingue is a naturalistic pan-European auxiliary language (Romance-leaning, some
Germanic); it reads close to Interlingua but with its own forms. Match Cmdr's English voice directly.

## Formality

**No T-V formality distinction.** Use imperative verbs for buttons ("Copiar", "Renominar", "Aperter", "Anullar").

## Decision points

### Script and variants: none to decide

- Latin script, one standardized form. No regional variants, no orthography or gender decision. Confidence: high.

### No grammatical gender

- No grammatical gender agreement; inclusive language is a non-issue. Confidence: high.

### Distinct from Interlingua (ia), do not mix

- Interlingue (ie) and Interlingua (ia) are SEPARATE constructed languages with different vocabulary and spelling ("New
  Folder" → "Nov fólder" in ie vs "Nove dossier" in ia; "Open" → "Aperter" vs "Aperir"). Use the ie catalog, never copy
  ia strings. Confidence: high.

## Terminology and glossary

Source: GNOME Nautilus + Xfce Thunar (`ie/`) only. `tentative` until reviewed. From the catalogs: copy → Copiar, rename
→ Renominar, trash → Paper-corb, new folder → Nov fólder, open → Aperter, cancel → Anullar, search → Serchar, folder →
Fólder. Confirm move, delete, file, settings, volume, server against the catalogs.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus `{system_settings}`-style tokens.
Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories: `one`, `other` (verified `new Intl.PluralRules('ie')`, 2026-06-20). Regular plural; write both
branches.

## Notes and decisions

- **Diacritics:** Interlingue uses accented forms ("fólder"); preserve them.
- **Length:** comparable to other Romance-leaning forms (~10–20% longer than English). Overflow-check against `en-XA`.
- **Numbers/dates** come from the formatter layer.

## Decisions to confirm with David

- **Ship at all?** Constructed language, no commercial localization, tiny community. Lowest priority; David's call.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ie/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
