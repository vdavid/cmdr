# Estonian (et) translation style guide

Working notes for translating Cmdr into Estonian. Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`et`), the universal Estonian set. Estonian is a single standard; no region variant is
needed.

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: a calm, competent peer. Estonian UI tone is
typically direct and unfussy, which fits Cmdr's voice. Keep error and crash copy reassuring and factual; prefer active
phrasing.

## Formality

**Use "sina" (informal singular)**, with confidence high but review-gated, because the top-tier source is absent (see
the macOS gap below).

- Estonian has a T-V distinction: informal singular "sina" (verb 2nd-person singular) vs polite plural "teie" (verb
  2nd-person plural). Unlike German or Greek, Estonian software has trended toward informal "sina" for consumer UI, and
  much Estonian open-source (GNOME, the file-manager catalogs) addresses the user informally or uses impersonal /
  imperative phrasings that sidestep the choice.
- Cmdr is a friendly app that signs onboarding as David, so "sina" matches its personality. Confidence: high, but flag
  for David because there's no macOS Estonian to anchor against (Microsoft Estonian and GNOME are the authorities here).
- Whatever is chosen, apply it consistently. Estonian UI very often avoids direct address entirely by using the
  imperative or impersonal voice ("Kustuta fail", "Faili ei leitud"), which is the cleanest way to dodge the sina/teie
  question for most strings.

**Imperatives for UI actions** (buttons, menu items): use the imperative, the Estonian UI convention and what the
file-manager catalogs use: "Kopeeri" (copy), "Kustuta" (delete), "Loobu" (cancel), "Liiguta" (move) (verified in
`et/gnome-nautilus/`, 2026-06-20).

## Decision points

- **macOS coverage gap: Apple does NOT localize macOS into Estonian.** The reference pile has no `et/macOS/` folder;
  only `microsoft-style-guides/`, `microsoft-terminology/`, `gnome-nautilus/`, and `xfce-thunar/` exist (verified
  2026-06-20). Implication: the usual highest-authority anchor (what a user sees in Finder) is missing, so an Estonian
  Cmdr user is NOT coming from an Estonian-localized macOS, their OS chrome is in English (or Russian/Finnish). Lean on
  Microsoft Estonian (Tier 2) and the GNOME/Xfce file-manager catalogs (Tier 3) instead, and weight terminology toward
  what Estonian speakers actually encounter. Confidence: confirmed.
- **No grammatical gender, a real simplification.** Estonian has no grammatical gender and no gendered pronouns ("tema"
  is gender-neutral for he/she/it). The gender/inclusive-language problem that bites German, Spanish, and Greek simply
  does not exist here: no gendered agent nouns, no inclusive-ending debate, no agreement-with-user's-gender traps.
  Confidence: confirmed.
- **Heavy case system is the real difficulty.** Estonian has 14 grammatical cases, and a noun's ending changes with its
  role and with counts. A counted noun typically sits in the partitive singular ("3 faili", not "3 failid"), and a
  placeholder dropped into a case slot can't be inflected by the catalog. Structure sentences so a `{name}` / `{path}`
  placeholder stays in a form that reads correctly, or carries its own surrounding words; prefer the partitive-singular
  pattern for counts. Confidence: high (Estonian grammar).
- **Regional variant: none. One `et` base.** Estonian is a single standard; all majors that ship Estonian ship one.
  Confidence: confirmed.
- **Letters õ ä ö ü (and š ž in loanwords).** These are full letters; never substitute ASCII. "õ" in particular is
  distinct from "o" and changes meaning. Keep them. Confidence: confirmed.

## Terminology and glossary

| English term | Estonian                  | Notes                                                                |
| ------------ | ------------------------- | -------------------------------------------------------------------- |
| Copy         | Kopeeri                   | imperative, GNOME                                                    |
| Move         | Liiguta                   | imperative, GNOME                                                    |
| Delete       | Kustuta                   | imperative                                                           |
| Cancel       | Loobu                     | imperative, GNOME ("\_Loobu")                                        |
| trash        | Prügikast                 | standard Estonian term for the trash; confirm against MS terminology |
| folder       | kaust                     |                                                                      |
| file         | fail                      | partitive "faili" in counted phrases                                 |
| Settings     | Sätted                    | confirm against MS Estonian terminology                              |
| crash report | krahhiaruanne / vearaport | confirm the non-alarmist fit against MS terminology                  |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Estonian CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('et')`, 2026-06-20; GNOME nautilus
confirms `nplurals=2; plural=(n!=1)`). Write both branches. Crucially, the counted noun takes the partitive case and the
ending differs from the dictionary form ("1 fail" vs "{count} faili"); get the partitive right inside each branch, not
just the number.

## Notes and decisions

- **Quotation marks**: Estonian uses „…“ (low-high), matching German/Nordic conventions. Confirm against MS Estonian
  style guide.
- **Numbers and dates come from the formatter layer** (comma decimal, space thousands). Never hardcode separators.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Formality: sina (informal, recommended) vs teie (polite).** No macOS anchor exists; recommendation is sina to match
  Cmdr's warm voice and Estonian consumer-UI norms, but it's a softer call than for languages with a macOS reference.
  Consistent application required.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/et/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
