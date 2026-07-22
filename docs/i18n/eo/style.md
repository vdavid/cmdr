# Esperanto (eo) translation style guide

Working notes for translating Cmdr into Esperanto. Read `../README.md` for how this fits the translation process.

This is the language base (`eo`). Esperanto is a constructed auxiliary language with no regional or national variant (by
design), so there is exactly one `eo`.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register. Esperanto UI (GNOME) is plain and regular;
follow the Fundamento de Esperanto for grammar and prefer simple, widely understood roots, which the GNOME Esperanto
translation guidelines also recommend.

## Formality

**No T-V distinction to settle, use "vi".** Esperanto has a single second-person pronoun "vi" for both singular and
plural, formal and informal. The formality choice that dominates every other language's guide simply does not exist
here; "vi" is always correct. (The intimate "ci" exists in theory but is essentially never used and never in UI.)

**Imperatives for UI actions** (buttons, menu items): use the imperative, formed with the `-u` ending: "Kopiu" (copy),
"Forigu" (delete), "Movu" (move), "Nuligu" (cancel). The GNOME catalog uses these forms ("\_Forigi" infinitive in some
labels, "\_Nuligi", "\_Kopii ĉi tien"); pick one label convention (the `-i` infinitive vs the `-u` imperative) and apply
it consistently, GNOME Esperanto tends to use the `-i` infinitive for menu/button labels, which is the recommended
default here too. Confidence: high.

## Decision points

**Major-product coverage is essentially nil, this is the headline finding and a low-priority signal.** The reference
pile has only `eo/gnome-nautilus/` and `eo/xfce-thunar/` (Tier 3, verified 2026-06-20). No commercial major (Apple,
Microsoft, Google, Spotify, Netflix) ships a meaningful Esperanto product UI; Esperanto localization lives almost
entirely in volunteer open-source (GNOME, KDE, Mozilla). An Esperanto Cmdr user's OS is in some other language. Treat
`eo` as a community/goodwill locale: the GNOME/Xfce catalogs plus the Fundamento are the only anchors. Confidence:
confirmed.

- **Script and special letters: ĉ ĝ ĥ ĵ ŝ ŭ (the circumflex/breve letters).** These are real Esperanto letters and must
  be used in their correct Unicode form, never the "cx/gx/hx/jx/sx/ux" x-system or "ch/gh" h-system ASCII surrogates,
  which are input workarounds, not correct orthography. Keep the proper diacritics in every string. Confidence:
  confirmed.
- **Gender: a non-issue, with one note.** Esperanto nouns are not grammatically gendered. The third-person pronouns are
  "li" (he) / "ŝi" (she) / "ĝi" (it), plus the widely accepted gender-neutral "ri" (singular they) in modern usage. But
  Cmdr addresses the user directly as "vi", so third-person gendered pronouns rarely arise; when referring to an
  unspecified person, prefer a neutral noun or "ri". No agreement-with-gender traps. Confidence: high.
- **Regional variant: none, by design.** Confidence: confirmed.
- **Length: roughly comparable to English**, sometimes slightly longer due to agglutinative compounds and the consistent
  grammatical endings. Low overflow risk relative to German/Welsh. Confidence: high.

## Terminology and glossary

| English term | Esperanto | Notes                             |
| ------------ | --------- | --------------------------------- |
| Copy         | Kopii     | GNOME ("\_Kopii ĉi tien")         |
| Move         | Movi      | GNOME ("\_Movi ĉi tien")          |
| Delete       | Forigi    | GNOME ("\_Forigi")                |
| Cancel       | Nuligi    | GNOME ("\_Nuligi")                |
| file         | dosiero   | standard Esperanto computing term |
| folder       | dosierujo | "dosiero" + "-ujo" (container)    |
| trash        | rubujo    | confirm against GNOME             |
| Settings     | Agordoj   | standard Esperanto computing term |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

Esperanto CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('eo')`, 2026-06-20; GNOME nautilus
confirms `nplurals=2; plural=(n != 1)`). Esperanto pluralizes regularly by adding "-j" to the noun and any agreeing
adjective ("unu dosiero" / "{count} dosieroj"); write both branches and keep the adjective agreement.

## Notes and decisions

- **Quotation marks**: Esperanto has no single fixed convention; „…“ and «…» both appear. Pick one and stay consistent;
  GNOME Esperanto usage is a reasonable tiebreaker.
- **Numbers and dates come from the formatter layer.** Never hardcode separators.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  `docs/guides/i18n-translation.md`.

## Decisions to confirm with David

- **Priority: Esperanto has no commercial-major precedent and a tiny audience.** It's a goodwill locale; confirm it's
  worth the catalog maintenance before prioritizing it over anchored languages.
- **Label form: `-i` infinitive (recommended, matches GNOME) vs `-u` imperative for buttons.** Pick once, apply
  consistently.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/eo/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
