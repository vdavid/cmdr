# French (fr) translation style guide

Working notes for translating Cmdr into French. Read [`README.md`](README.md) for how this fits the translation process.

This is the language base (`fr`), the universal French set and the fallback for any future region variants (`fr-CA`,
`fr-CH`). Stick to standard metropolitan French here; push region-specific phrasing into a variant only when one is
added.

## Voice and tone

Friendly, concise, active, and never alarmist. Cmdr in French should sound like a calm, competent peer, not a corporate
support desk. Match the English register: warm and direct, never stiff. Error and crash copy stays reassuring and
factual; avoid dramatizing words. As in English, steer clear of "erreur" / "échec" framing in user-facing copy where a
calmer phrasing works, and prefer active voice ("Nous avons envoyé…" over "… a été envoyé").

## Formality

**Use "vous"** (the formal/polite second person) throughout. It's the safe, near-universal register for French software
UI: respectful without being cold, and it reads naturally to every French speaker. "Tu" would feel too familiar for a
file manager addressing an unknown adult user, and inconsistent tu/vous is jarring, so vous everywhere, no exceptions.

**Imperatives for UI actions** (buttons, menu items): use the infinitive, the French UI convention ("Envoyer",
"Annuler", "Copier", "Ignorer"), not the imperative mood. The infinitive is the neutral, label-style form Apple and
most French macOS software use for commands.

## Terminology and glossary

- **crash report** → rapport d'incident · "incident" is the standard, non-alarmist French term (matches Apple's "rapport d'incident"); avoid "rapport de plantage" which is more colloquial
- **crashed / quit unexpectedly** → s'est fermé(e) de façon inattendue · matches macOS French phrasing for an unexpected quit
- **Report ID** → identifiant du rapport
- **Settings** → Réglages · macOS French names the app preferences pane "Réglages"; keep consistent with how the in-app Settings section is named once an `fr` catalog exists
- **Updates** → Mises à jour · in-app navigation section; keep consistent across the catalog
- **Send** → Envoyer
- **Copy** → Copier
- **Dismiss** → Ignorer · "Ignorer" fits a non-destructive dismiss better than "Fermer" here
- **Always** → Toujours

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. The `{email}`-style
placeholder tokens are also verbatim.

## Plurals

French CLDR plural categories: `one`, `many`, `other` (run
`new Intl.PluralRules('fr').resolvedOptions().pluralCategories` to confirm). French treats 0 and 1 as singular ("one"),
and `many` covers large/compact-notation values. None of the crash-reporter strings use ICU plurals, so this is a note
for future strings: cover the categories the message needs, not English's.

## Notes and decisions

- Roster: base fr (France norm) ships first; Canadian French (fr-CA) is a deferred variant. See
  [`language-selection-decisions.md`](language-selection-decisions.md).
- **Punctuation spacing**: French uses a narrow no-break space before `: ; ! ?`. Apply it (e.g. "Identifiant du
  rapport :"). Use a real narrow no-break space (U+202F) where typographically correct, or accept a regular space if
  the rendering context doesn't support it; stay consistent within the catalog.
- **Quotation marks**: use French guillemets « … » with inner spacing when quoting, not English "…".
- **Apostrophes**: in ICU strings (everything outside `errors.*`), double every apostrophe (`d''incident`). In
  `errors.*` keys, use normal apostrophes. The crash-reporter strings are ICU, so they double.
- **Ellipsis**: keep the source's literal three dots ("Envoi...") rather than swapping to a single … character, to
  match the English catalog value.
