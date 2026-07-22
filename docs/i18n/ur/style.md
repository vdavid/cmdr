# Urdu (ur) translation style guide

Working notes for translating Cmdr into Urdu. Read `../README.md` for how this fits the translation process.

RTL, and the biggest workstream here is the app, not the words. Urdu is written right-to-left in the Perso-Arabic
script. The pile has Microsoft terminology and the Microsoft Urdu style guide (both useful for register and gender
guidance), plus a sparse Xfce Thunar catalog; there is NO macOS Finder reference, because Apple ships no Urdu macOS UI
(Apple's Urdu support is iOS/keyboard/web only, verified 2026-06-20 via Apple Community + Keyman). So the
highest-authority source (a real localized OS) is absent; treat terms as `tentative` until a native reviewer confirms.

## Voice and tone

Friendly, concise, active, never alarmist, matching Cmdr's English voice. The Microsoft Urdu guide pushes the same
register: conversational, warm, less formal, address the user as "you" not "the user", avoid the corporate "we". Error
messages stay calm and actionable and avoid the words "error" and "failed". With no macOS reference, prioritize plain,
clear Urdu over literary flourish.

## Formality

Use آپ (respectful "you") for the user throughout, with its honorific verb agreement, never the familiar تم/تو. This is
the standard software register and Microsoft's Urdu guide uses it. For UI actions (buttons, menu items), use the
imperative/command form. Confidence: high.

## Decision points

### RTL and bidi: the dominant concern

This is the single biggest issue, and it's a LAYOUT problem as much as a text one.

- The whole UI must mirror: the two panes swap sides, and cursor/selection logic, progress bars, chevrons, and
  back/forward navigation arrows all reverse. A right-pointing "forward" arrow is wrong in RTL.
- Cmdr is a two-pane file manager, so the left/right pane mental model itself mirrors under RTL. Confirm the app's
  layout engine flips correctly before shipping any RTL locale; this is an app-code question, not a translation one.
- Bidi is the headline hazard for a FILE MANAGER specifically: file paths, filenames, extensions (`.txt`), URLs, brand
  names (Cmdr, macOS, SMB), and (if Western digits are used) numbers are all LTR runs embedded in RTL Urdu text. Without
  proper Unicode bidi isolation, a `{path}` or `{name}` insert visually scrambles the surrounding sentence: the period
  ending an embedded LTR run lands on the wrong side, and a path like `/Users/x/file.txt` can reorder. Every
  uncontrolled insert (`{path}`, `{name}`, `{message}`) must sit in a bidi-isolated span; rely on isolation (the
  `dir="auto"` / `<bdi>` / Unicode isolate approach), not on manually sprinkling LRM/RLM marks. LRM/RLM are the
  last-resort manual fix for a stubborn edge; isolation is the default.
- Recommendation: do NOT ship Urdu until the app's RTL mirroring and bidi isolation are verified end to end. The
  translation is the smaller half of the work. Confidence: high that RTL is the gating issue.
- Flag for David: Urdu (with ps, sd-Arabic, ar, he, fa) is the trigger for a whole RTL-readiness workstream in the app,
  separate from translation. This is the headline finding, shared across all RTL locales.

### Script and font: Nastaliq vs Naskh

Urdu is Perso-Arabic, but the rendering tradition matters and differs from Arabic.

- Pakistani readers expect Nastaliq (the sloping, calligraphic style), not Naskh. Naskh reads as "foreign/Arabic" to an
  Urdu audience even though it's technically legible. Google ships Noto Nastaliq Urdu for exactly this; Apple/Microsoft
  systems fall back to whatever Nastaliq or Naskh face is installed.
- Nastaliq is expensive: it needs 2D glyph positioning, far more glyphs, and tall line-height (roughly 2.0–2.4) to keep
  the descending words from colliding. That collides with a dense file-list UI with tight rows.
- macOS renders Urdu text (the script isn't blocked) but ships no Urdu UI; the system Urdu font face is not guaranteed
  to be Nastaliq.
- Recommendation: respect the system font per Cmdr's principle, but verify Urdu actually renders in a Nastaliq-capable
  face and that row heights don't clip the tall Nastaliq baseline; if the system face is Naskh-only, consider bundling
  Noto Nastaliq Urdu. This is an app-rendering call, not a translation one. Confidence: high on "Nastaliq expected",
  tentative on the exact font remedy. Flag for David: font/row-height handling for Nastaliq.

### Regional variant: ur vs ur-PK vs ur-IN

- Target `ur` as the base, Pakistan-leaning (`ur-PK` register). The large majority of Urdu speakers and software demand
  are in Pakistan. `ur-IN` (India) differs in minor vocabulary and some loanword preferences; the written standard is
  broadly shared.
- Microsoft references `ur-PK`. The variant gap is minor for a file-manager UI.
- Recommendation: base `ur` = Pakistan register; treat `ur-IN` as a possible later, lower-priority variant only if
  Indian demand appears. Confidence: high.

### Numerals: Western (0123) vs Eastern Arabic-Indic (۰۱۲۳)

A file manager shows counts and file sizes constantly, so this matters.

- Urdu can use Eastern Arabic-Indic digits (the Persian/Urdu shapes ۰۱۲۳, Unicode U+06F0–U+06F9) or Western digits
  (0123). In SOFTWARE, Western digits are common and are what the majors' systems default to:
  `Intl.NumberFormat('ur').format(1234)` yields `1,234` (Western, verified 2026-06-20 with Node), not ۱۲۳۴.
- Western digits also sidestep a bidi problem: they're an LTR run, but a numeral-only run is far less likely to scramble
  than a mixed path; Eastern digits flow more naturally inside the RTL text but are heavier on rendering and less
  familiar in technical UI.
- Recommendation: let `Intl` with the `ur` locale shape numerals (it picks Western by default), rather than hardcoding
  either; confirm audience expectation with a native reviewer. Confidence: high that Western is the safe software
  default, tentative on whether the audience prefers Eastern.

### Gender and inclusive language

Urdu verbs and adjectives are grammatically gendered and agree with the subject, so a sentence about "you" (the user)
can leak a gender assumption.

- Microsoft's Urdu guide is explicit: use a respectful tone, avoid gender-specific forms, prefer common nouns over
  male/female pairs, don't use gendered pronouns (مرد/عورت, لڑکا/لڑکی) in generic references, and rewrite to second or
  third person (آپ / کوئی).
- Recommendation: structure UI copy around imperatives and nouns so the user is never the gendered subject of an
  agreeing verb. There's no established neutral-morphology trick; rephrasing is the tool. Confidence: high (Microsoft
  guidance is direct).

## Terminology and glossary

Defer; with no macOS source, every term is tentative and needs native review. Triangulate Microsoft `ur` terminology +
the (sparse) Xfce Thunar catalog only. The Thunar `ur` catalog is largely untranslated, which is itself a low-precedent
signal.

| English term | Urdu  | Notes                              |
| ------------ | ----- | ---------------------------------- |
| file         | فائل  | Thunar `ur`; confirm with reviewer |
| trash        | ردی   | Thunar `ur`; tentative             |
| folder       | فولڈر | tentative, needs native check      |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`. In RTL Urdu these LTR brand runs need bidi isolation (see the RTL decision point).

## Plurals

CLDR categories for `ur`: `one`, `other` (verified 2026-06-20 via `Intl.PluralRules('ur')`). Only two plural forms
needed. The `desktop-i18n-plural` check requires both branches on every plural message. Mind that the counted noun's
adjective/verb agreement is gendered, so write each branch to read naturally rather than slotting a count into a fixed
frame.

## Notes and decisions

- Urdu desktop localization is LOW priority and sparse in precedent: no Apple macOS UI, limited Apple system support, a
  mostly-untranslated Thunar catalog, and it forces the RTL app-readiness work. Major-product Urdu desktop localization
  is essentially absent (the action is on mobile/web). That low-priority signal IS a finding: recommend deprioritizing
  until an RTL workstream exists and a native reviewer is available.
- Punctuation: in bidi context the sentence-final period attaches to the RTL paragraph; let the bidi algorithm place it,
  don't force it with marks.

## Decisions to confirm with David

- Is the app's RTL layout mirroring + bidi isolation ready? Urdu can't ship without it. This gates ur, ps, sd-Arabic,
  and every RTL locale, and is the headline finding.
- Nastaliq rendering: rely on the system font, or bundle Noto Nastaliq Urdu, and how to handle tall Nastaliq line-height
  in dense rows?
- Numerals: Western (the `Intl` default and software norm) vs Eastern Arabic-Indic? Defaulting to `Intl` is safe; native
  reviewer settles audience preference.
- Is Urdu in scope at all for launch, given no macOS reference and the need for native review on nearly every term?

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ur/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
