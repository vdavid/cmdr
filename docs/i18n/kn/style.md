# Kannada (kn) translation style guide

Working notes for translating Cmdr into Kannada (ಕನ್ನಡ). Read [`README.md`](../README.md) for how this fits the
translation process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes
carry into Kannada.

Sourced: the pile has MS terminology, MS style guide, and GNOME Nautilus (`_ignored/i18n/kn/`); no macOS folder for
Kannada, no Xfce. Evidence verified against the pile on 2026-06-20.

## Decisions to confirm with David

- **English-loanword vs native-coinage register (the key flag, high).** Indian-language software localization runs on a
  spectrum: heavy native (Sanskritized) coinages vs everyday speech that borrows common English tech words ("file",
  "folder", "copy") in Kannada script. The two read very differently to users. MS and GNOME lean native; many users
  speak the loanword register daily. Recommendation: lean native where a well-established Kannada term exists (trash,
  folder), but don't coin obscure words where the English loan is what users actually say. Flagging because this is a
  register call a native reviewer/David should set once, then apply consistently.
- **A few file-manager terms (tentative).** GNOME Kannada is partial; settle core terms against MS terminology with a
  native check.

## Voice and tone

Friendly, concise, active, calm, never alarmist. MS Kannada targets the conversational, everyday register over formal
technical language (verified 2026-06-20), which matches Cmdr's English voice. Kannada verbs don't conjugate for gender
in the relevant forms, easing one class of agreement (see Decision points). Error messages stay calm and actionable:
name the problem and the next step, and avoid a bare "ದೋಷ" (error) status label the way English avoids "error"/"failed".

## Formality

- **Polite second person, addressing the user directly.** Kannada distinguishes a familiar second person from a polite
  one (`ನೀವು` polite plural/honorific vs `ನೀನು` familiar). Software uses the polite `ನೀವು` form. Recommended default:
  **`ನೀವು`-register throughout.** Confidence: high.
- **Action labels (buttons, menu items): the established imperative form.** Follow GNOME/MS for action verbs ("ರದ್ದು
  ಮಾಡು" Cancel appears in GNOME). Keep labels short. Use full `ನೀವು`-form verbs in sentences to the user. Confidence:
  medium-high pending a fuller term pass.

## Decision points

- **Script: Kannada, no decision.** Kannada is written in its own abugida script. It is unicameral (no upper/lower
  case), so the title-case-vs-sentence-case question collapses to one letterform. Confidence: confirmed.
- **Regional variant: one, `kn` (`kn-IN`).** Kannada is the official language of Karnataka, India; no second national
  standard, no variant matrix. Confidence: high.
- **Gender / inclusive language: low problem.** Kannada nouns have gender (masculine/feminine/neuter) and third-person
  pronouns are gendered, but the polite `ನೀವು` address and the usual UI verb forms don't force the user's gender. Where
  a sentence would expose gender, rewrite impersonally or use the neuter/plural. No big inclusive-form workaround
  needed. Confidence: high.
- **Capitalization: not applicable.** Kannada script has no case. Don't capitalize labels. Confidence: confirmed.
- **Agglutination and case suffixes affect placeholder grammar (high).** Kannada is agglutinative: nouns take case
  suffixes (often with vowel/sandhi changes at the join). A `{path}` or `{name}` placeholder followed by a case suffix
  can't reliably attach the suffix to runtime text. Structure sentences so a placeholder lands where the grammar doesn't
  depend on the inserted value's ending. A native reviewer handles sandhi/case edges. Confidence: high; the subtlest
  translator-craft concern for Kannada.
- **Register: loanword vs native coinage (see Decisions to confirm).** Set the register once and apply consistently.

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Evidence verified against `_ignored/i18n/kn/` (MS terminology, MS
style guide, GNOME Nautilus) on 2026-06-20; no macOS for Kannada, so GNOME/MS are the highest available authorities for
file-manager terms. Sources decide the term; Cmdr writes its own value (MS copyrighted, GNOME GPL, never copied
verbatim).

Settled where GNOME/MS agree:

- **trash: `ಕಸಬುಟ್ಟಿ`** (waste basket) · GNOME ("Trash" → "ಕಸಬುಟ್ಟಿ"). `high`.
- **folder: `ಕಡತಕೋಶ`** (file-case) · GNOME ("Folder" → "ಕಡತಕೋಶ"). Native coinage; `high` for the native register.
- **cancel: `ರದ್ದು ಮಾಡು`** · GNOME ("Cancel" → "ರದ್ದು ಮಾಡು"). `high`.

To settle from MS terminology / GNOME with a native check:

- **file: `ಕಡತ`** (the likely native term; `ಫೈಲ್` is the loanword) · GNOME/MS; pick per the register decision.
  `tentative`.
- **copy, open, delete, rename, eject** · GNOME Kannada is partial here; triangulate with MS terminology. `tentative`.
- **volume / pane / tab** · no macOS anchor; check MS terminology, else native review. `tentative`.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`. Latin brand names stay in Latin script inside Kannada text.

## Plurals

CLDR categories for `kn`: `one`, `other` (verified with `new Intl.PluralRules('kn')`, 2026-06-20). Write both.

- **one**: integer 1 (and CLDR-mapped `one` cases). "1 ಕಡತ".
- **other**: everything else, including 0 and all counts ≥ 2: "5 ಕಡತಗಳು", "0 ಕಡತಗಳು". Kannada does mark plural on the
  noun (suffix `-ಗಳು`), so the `other` branch typically pluralizes the counted noun, unlike Khmer/Turkic. A native
  reviewer confirms the exact form. The `desktop-i18n-plural` check requires both.

## Notes and decisions

- **Quotation marks:** Kannada UI commonly uses English-style `"…"` (or `'…'`); a native reviewer settles house style.
- **Numbers and dates come from the formatter layer.** Kannada has its own digit glyphs but Arabic digits are standard
  in modern UI; `formatNumber()`/`formatBytes()` follow the locale. Never hardcode separators in a string.
- **Length and height.** Kannada renders with stacked vowel/consonant signs and can be taller; overflow-check both width
  and line-height against the pseudolocale (`en-XA`) and a Kannada font.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/kn/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
