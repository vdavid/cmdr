# Dutch (nl) translation style guide

Working notes for translating Cmdr into Dutch. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Dutch.

## Formality: `je`, settled

**Address the user as `je` / `jij` / `jou` / `jouw`** (informal second person) throughout. This is settled from the
sources, not a guess:

- macOS Dutch is fully informal. Across the mined Finder + AppKit strings, every second-person address uses `je` / `jij`
  / `jou` (520 `je` + 12 `jou` in Finder, 189 `je` in AppKit); there is not a single formal `u` / `uw` address (the one
  `uw` hit in AppKit is not a user address). Finder phrases prompts as "Weet je zeker dat je …", "Je kunt …", "Wil je
  opnieuw proberen …" (verified in `nl/macOS/`, grep over Finder + AppKit, 2026-06-19).
- GNOME Nautilus and Xfce Thunar Dutch use formal `u` ("Wanneer u een bestand via e-mail verstuurt …"). That is the
  open-source desktop convention (Tier 3), not ours; don't copy it.
- Cmdr is a macOS app with a friendly voice that even signs onboarding as David, so `je` is both the macOS-native choice
  and the right tonal fit. (No Microsoft `nl` style-guide PDF is in the pile, but macOS Tier 1 is decisive on its own.)

## Voice and tone

Friendly, concise, active, calm, warm. Dutch UI copy reads naturally with direct `je` address; keep it light and don't
force `je` into every line where macOS phrases neutrally ("Versturen…", not "Je verstuurt nu…").

Error messages stay calm and actionable and never use "fout" or "mislukt" as a bare label: state the problem and a next
step. Note: macOS itself does use "fout" freely ("een onverwachte fout"); Cmdr's voice rule is stricter than macOS here,
so don't copy that pattern.

## Formality mechanics

- **`je` / `jij`**, throughout (see Formality above). Use `je` (unstressed) by default; `jij` only for contrastive
  emphasis.
- **Buttons and menu items: bare-stem imperative**, matching macOS Finder, which uses the imperative stem, NOT the
  infinitive: "Verstuur" (not "Versturen"), "Annuleer" (not "Annuleren"), "Kopieer" (not "Kopiëren"), "Toon" (not
  "Tonen"). (verified in `nl/macOS/`, key cross-ref by value, 2026-06-19.)
  - Caveat: the GNOME/Xfce catalogs use the infinitive for buttons ("Verzenden"). macOS is Tier 1 and the imperative
    stem is the native-Mac feel, so prefer it for Cmdr's buttons.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order is macOS (highest, Tier 1) → Microsoft (Tier 2) →
GNOME/Xfce (Tier 3). Confidence is `confirmed` (human signed off), `high` (authoritative sources agree), or `tentative`
(sources conflict or none had it).

Straightforward (sources agree, `high`):

- send → versturen (verb) / Verstuur (button) · macOS Finder ("Send"→"Verstuur", "Sending…"→"Versturen…") · high
  - Microsoft uses "verzenden"; macOS is Tier 1 and "versturen" is the native-Mac form, so prefer it.
- cancel → Annuleer (button) / annuleren · macOS ("Cancel"→"Annuleer", consistent across AppKit + Finder) · high
- copy → Kopieer (button) / kopiëren · macOS ("Copy"→"Kopieer") · high
- copied → gekopieerd (past participle) · GNOME Nautilus ("Copied …"→"… gekopieerd") · high
- show details → Toon details · macOS AppKit ("Show Details"→"Toon details") · high
- settings → Instellingen · macOS ("Settings"→"Instellingen"), MS ("settings"→"instellingen") · high
- updates → Updates (kept; capitalized as a Settings-section name) · MS ("Updates"→"Updates", ProperNoun) · high
- version → versie · MS ("version"→"versie") · high
- report → rapport · MS ("report"→"rapport") · high
- crash report → crashrapport · macOS uses "Crashrapportage" for crash reporting; "crashrapport" is the natural Dutch
  compound for the report itself · high
- quit unexpectedly → onverwachts gestopt · macOS ("unexpectedly quit"→"onverwachts gestopt") · high
- dismiss → Sluit (button) / sluiten · MS ("dismiss"→"sluiten"); rendered as the bare-stem imperative per the button
  rule · high
- done → Gereed · macOS ("Done"→"Gereed") · high
- save → bewaren · macOS ("Save"→"Bewaar"); macOS uses "bewaren", NOT "opslaan", for save · high
- file → bestand (plural bestanden) · macOS, MS, Nautilus · high

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style tokens
and any `{email}`-style placeholders. Enforced by `desktop-i18n-dont-translate` (list in
`apps/desktop/scripts/i18n-catalog-lib.ts`). macOS UI names Cmdr opens into (System Settings panes, "Prullenmand")
should match a Dutch macOS.

## Plurals

CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('nl')`). Write both branches: "1 bestand" /
"{count} bestanden". Dutch is close to English here (singular vs everything-else), so plural handling is low-risk.

## Notes and decisions

- **Sentence case, not title case.** Dutch capitalizes only the first word and proper nouns, which fits the app's
  sentence-case rule directly. "Verstuur crashrapport?" not "Verstuur Crashrapport?".
- **Quotation marks:** macOS Dutch uses single curly quotes `‘…’` for quoted UI strings ("Klik op 'Ga door' …", and
  curly `‘%s’` in Nautilus). Prefer `‘…’`; avoid straight English `"…"`.
- **Length:** Dutch runs slightly longer than English (compounds like "crashrapport", "instellingen"), but far less than
  German. Overflow-check the layout against the pseudolocale (`en-XA`); watch buttons and toasts.
- **Compound nouns concatenate** ("crashrapport", "foutcode"). Correct Dutch; don't space-separate them.
- **Numbers and dates come from the formatter layer** (comma decimal, period thousands). Never hardcode separators.
- Record case-by-case rulings here.

## Decisions to confirm with David

The formality (`je`) and the send/cancel/copy terms are settled from macOS (Tier 1). Open subjective items:

- **send → versturen vs verzenden** (resolved to `versturen` from macOS, but Microsoft prefers `verzenden`): confirm
  "Verstuur rapport" reads better than "Verzend rapport" for the crash-report button. Low stakes; both are correct.
- **crash report → crashrapport** (high, but no exact macOS string for the noun): macOS has "Crashrapportage" (the
  reporting feature). Confirm "crashrapport" for the artifact reads natural in Cmdr's dialog.
- **Ask Cmdr tool-status doing/done pairs** (`askCmdr.tool.*`): no pile precedent for AI-assistant status lines, so the
  seven pairs are coined (present tense for "doing", past-participle-led for "done"; see glossary "Ask Cmdr pass" REVIEW
  FLAGS). Confirm the tone lands, and that seven distinct verbs read as a coherent family rather than ad hoc.
- **unarchive → "Uit archief halen"**: no single natural Dutch imperative verb for "unarchive" the way "Archiveer" works
  for "archive". Confirm this multi-word button reads fine next to its short siblings.
- **"Ask Cmdr model" → "Ask Cmdr-model"**: hyphenating after a two-word English brand name is a judgment call (no exact
  pile precedent for a multi-word brand + suffix). Confirm it doesn't read as awkward.
- **rename as a NOUN → "naamwijziging"** (with the compound "naamwijzigingsplan"), sourced from Microsoft's
  "naamwijzigingsvoorstellen"; the Tier-3-only "hernoeming" is out. A few keys outside the bulk-rename feature still say
  "hernoemen" (see the glossary's review flags); confirm a locale-wide sweep.
- **"Review file renames" → "Naamwijzigingen beoordelen"**: "beoordelen" (decide) over macOS's look-over "bekijken",
  because the modal is a per-row allow/deny gate. Confirm the register.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/nl/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
