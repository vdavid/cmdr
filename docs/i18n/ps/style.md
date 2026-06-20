# Pashto (ps) translation style guide

Working notes for translating Cmdr into Pashto. Read [`README.md`](../README.md) for how this fits the translation
process.

Low resourcing, and RTL. The pile has only GNOME Nautilus (`ps`) and Microsoft terminology (`ps-AF`); NO macOS Finder
(Apple does not ship a Pashto macOS UI). So the highest-authority source (a real localized OS) is absent. Treat every
term as `tentative` until a native reviewer confirms, and expect more flags-for-David than for a major language.

## Voice and tone

Friendly, concise, active, never alarmist, matching Cmdr's English voice. With no macOS reference and limited software
precedent, prioritize clarity and plain Pashto over clever phrasing. Error messages stay calm and actionable.

## Formality

Pashto distinguishes familiar (تاسو/ته) and polite address; software convention leans polite/neutral. Use the
imperative/command verb form for actions, addressing the user politely where running text is unavoidable. Microsoft's
ps-AF terminology is the only software-register reference in the pile; follow its register. Confidence: tentative (thin
evidence).

## Decision points

### RTL: this is the dominant concern

Pashto is written right-to-left in an extended Perso-Arabic script. This is the single biggest issue and it's a LAYOUT
concern as much as a text one:

- The whole UI must mirror: panes swap sides, the cursor/selection logic, progress bars, chevrons, and "back/forward"
  navigation arrows all reverse. A right-pointing "forward" arrow is wrong in RTL.
- Cmdr is a two-pane file manager, the left/right pane mental model itself mirrors under RTL. Confirm the app's layout
  engine flips correctly before shipping any RTL locale; this is an app-code question, not just a translation one.
- Bidi hazard: file paths, URLs, brand names (Cmdr, macOS), and numbers are LTR runs embedded in RTL text. Without
  proper Unicode bidi isolation, a `{path}` insert can visually scramble the surrounding sentence. Ensure inserts are
  bidi-isolated.
- Recommendation: do NOT ship Pashto until the app's RTL layout mirroring is verified end to end. The translation is the
  smaller half of the work. Confidence: high that RTL is the gating issue.
- Flag for David: Pashto (with ps, sd, and any Arabic/Hebrew/Urdu) is the trigger for a whole RTL-readiness workstream
  in the app, separate from translation. This is the headline finding.

### Script and regional variant

Pashto is Perso-Arabic only (no script split like Sindhi). Regional variants exist (Afghanistan `ps-AF` vs Pakistan),
differing mainly in some vocabulary and pronunciation, but the written standard is broadly shared. Microsoft uses
`ps-AF` (Afghanistan) as its reference. Recommendation: target the Afghanistan standard (`ps-AF` register) as base `ps`;
the variant gap is minor for UI. Confidence: tentative.

### Numerals: Eastern Arabic vs Western digits

Pashto can use Eastern Arabic-Indic numerals (۰۱۲۳…) or Western digits (0123). File sizes and counts appear constantly
in a file manager. Recommendation: let `Intl` with the `ps` locale decide numeral shaping rather than hardcoding either;
confirm which the target audience expects with a native reviewer. Confidence: tentative.

### Gender and inclusive language

Pashto is grammatically gendered with verb agreement. As with other gendered languages, avoid making the user the
gendered subject; structure around imperatives and nouns. No established neutral-morphology convention in Pashto UI.
Recommendation: rephrase to avoid user-gender agreement. Confidence: tentative.

## Terminology and glossary

Defer; with no macOS source, every term is tentative and needs native review. Triangulate Microsoft ps-AF terminology +
GNOME Nautilus only.

| English term | Pashto       | Notes                         |
| ------------ | ------------ | ----------------------------- |
| file         | فایل         | confirm with reviewer         |
| folder       | پوښه / فولډر | tentative                     |
| trash        | کثافت دانی   | tentative, needs native check |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`. Note: in RTL text these LTR brand runs need bidi isolation (see RTL decision point).

## Plurals

CLDR categories for `ps`: `one`, `other`. Only two plural forms needed.

## Notes and decisions

- Pashto is LOW priority for launch absent specific demand. The honest signal: thin localization precedent, no Apple OS
  support, and it forces the RTL app-readiness work. Recommend deprioritizing until an RTL workstream exists and a
  native reviewer is available.

## Decisions to confirm with David

- Is the app's RTL layout mirroring ready? Pashto can't ship without it. (This gates ps, sd-Arabic, and any RTL locale.)
- Is Pashto in scope at all for launch, given no macOS reference and the need for native review on nearly every term?

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ps/`; recipes in `_ignored/i18n/how-to-mine.md`).
Never guess a term.
