# Danish (da) translation style guide

Working notes for translating Cmdr into Danish. Read [`README.md`](../README.md) for how this fits the translation
process.

This is the language base (`da`), the universal Danish set. Danish is effectively a single standard (Rigsdansk); no
region variant is needed (see Decision points).

## Voice and tone

Friendly, concise, active, and never alarmist. Danish UI tone is naturally informal and warm, which fits Cmdr's voice
well. Match the English register: a calm, competent peer, not a corporate support desk. Error and crash copy stays
reassuring and factual; as in English, steer clear of "fejl"-heavy framing where a calmer phrasing works, and prefer
active voice.

## Formality

**Use "du"** (informal second person), settled from the sources, not a guess. macOS Danish addresses the user as `du`
throughout: across the mined Finder and AppKit strings the informal forms dominate overwhelmingly (`du` ~577, plus `dig`
/ `din` / `dine` / `dit`), with no use of the formal `De` / `Dem` / `Deres` capitalized polite form (verified in
`da/macOS/`, grep over Finder + AppKit, 2026-06-20). The Microsoft Danish style guide likewise prescribes `du`. Danish
dropped the formal `De` from everyday and software use decades ago; it now reads stiff or old-fashioned. So `du`
everywhere, no exceptions.

**Imperatives for UI actions** (buttons, menu items): use the imperative form, the Danish UI convention and what macOS
Finder uses: "Kopier", "Flyt", "Gem", "Slet", "Omdøb", "Annuller" (verified in `da/macOS/Finder/`, 2026-06-20). This is
the Danish imperative (often the bare verb stem), not an infinitive.

## Decision points

Formality is settled above (`du`). The remaining Danish-specific calls are minor.

- **Regional variant: none. One `da` base.** Danish is essentially a single written standard across Denmark, the Faroe
  Islands, and Greenland. Apple, Microsoft, Google, Spotify, and Netflix all ship exactly one Danish; there is no
  `da-DK` vs other split worth making. Recommendation: one base `da`. Confidence: confirmed.
- **Capitalization: sentence case, NOT English title case.** Danish capitalizes only the first word of a sentence/label
  and proper nouns; it does not capitalize each word of a heading or button the way English title case would. macOS and
  Microsoft Danish both follow this. This aligns with Cmdr's own sentence-case rule, so it's a reinforcement, not a
  conflict: never carry English title-casing into Danish. Confidence: confirmed.
- **Gender and inclusive language: low-friction.** Danish has common/neuter grammatical gender on nouns (en/et), but it
  does not gender the person addressed, and direct `du`-address sidesteps any "he/she" choice. No inclusive-form debate
  like Spanish's or German's exists for Danish UI. Recommendation: direct `du`-address, neutral nouns; nothing special
  needed. Confidence: high.
- **Compounding and length.** Danish writes compounds as one word ("filoverførsel" = file transfer, "netværksdrev" =
  network drive), which both lengthens individual tokens and is a correctness trap (splitting a Danish compound into two
  words, "fil overførsel", is a real and common error that changes meaning). Keep compounds joined. Length runs modestly
  longer than English. Confidence: confirmed (Danish orthography).

## Terminology and glossary

| English term | Danish          | Notes                                                                |
| ------------ | --------------- | -------------------------------------------------------------------- |
| trash        | Papirkurv       | macOS Danish term for the trash; matches Finder                      |
| Copy         | Kopier          | imperative, macOS Finder                                             |
| Move         | Flyt            | imperative, macOS Finder                                             |
| Delete       | Slet            | imperative                                                           |
| Rename       | Omdøb           | imperative, macOS Finder                                             |
| Cancel       | Annuller        | imperative, macOS                                                    |
| Save         | Gem             | imperative, macOS                                                    |
| Settings     | Indstillinger   | macOS app-settings term                                              |
| crash report | nedbrudsrapport | "nedbrud" is the standard Danish term for an app crash; non-alarmist |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

Danish CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('da')`, 2026-06-20; the GNOME nautilus
catalog confirms `nplurals=2; plural=(n != 1)`). Write both branches; cover the categories the message needs, not
English's. Danish, like English, treats only 1 as singular.

## Notes and decisions

- **Quotation marks**: Danish typographic quotes are `„…“` (low-high) traditionally, though `»…«` (guillemets pointing
  inward) is also standard and common in print. macOS Danish UI usage should be the tiebreaker once a `da` catalog
  exists; check `da/macOS/` for the form Apple uses and match it. Confidence: tentative (confirm against macOS usage).
- **Æ, Ø, Å**: these are full letters, not decorated a/o; never substitute "ae"/"oe"/"aa" (the "aa" form is archaic).
  Keep them in UI strings.
- **Numbers and dates come from the formatter layer** (comma decimal, period thousands). Never hardcode separators.
- **Ellipsis**: keep the source's three literal ASCII dots ("Sender...") to match the English catalog shape, per the
  catalog convention.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  the agent-handoff block in [`../guides/i18n-translation.md`](../../guides/i18n-translation.md).

## Decisions to confirm with David

- **Quotation-mark form** (`„…“` vs `»…«`): pick one once the `da` catalog exists, ideally matching macOS Danish.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/da/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
