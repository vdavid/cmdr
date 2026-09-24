# Translation principles

The judgment calls every language shares. `pnpm i18n:brief` embeds everything under `## Principles` in every brief,
right after the translator instructions, so a translator reads them there. Edit them here, and keep them
language-neutral: a rule that only one language needs belongs in that language's `style.md`.

## Principles

- **Voice**: Cmdr is casual, friendly, and plain. When two forms are both correct, pick the everyday one over the formal
  or bookish one.
- **Names vs prose**: a term ruling locks the NAME of a thing: menu items, settings, commands, and anything the user
  also sees in macOS or Finder (app names, Finder tag colors). Names never drift. In running prose (descriptions,
  messages, explanations) you may use a natural everyday synonym instead of the ruled form when it reads more casual and
  is equally clear, but never for a name. That freedom is small on purpose: the termbase stays the default. A synonym
  you'd reuse goes in the ruling's `proseAccept`, which the checks accept in prose keys only.
- **No hedged grammar**: never write a parenthesized or slashed alternative to dodge grammar you can't know (the English
  "file(s)" and its cousins in every language: a plural, article, gender, or case ending in brackets, or an ending that
  depends on how an inserted value is pronounced). When Cmdr knows the variable, use ICU `plural` / `select`. When it
  doesn't, rephrase: a colon form ("Name: {name}"), the placeholder placed where no article or agreement touches it, or
  the object's type named first ("the folder {name}"). Never endorse a hedge in a style guide.
- **Native typography**: use your language's own quotation marks (primary and nested), its spacing rules (for example a
  non-breaking space before certain punctuation, where the language requires one), `…` (U+2026) for an ellipsis, never
  three dots, and its own list and number punctuation. Straight ASCII quotes (`"`) never appear in UI text. Your
  locale's `mechanics.json` declares which marks and spacing apply, and `i18n-mechanics` checks the catalog against it.
- **Escalate, don't work around**: a problem in the English source (inconsistent or ambiguous English, a weak or wrong
  `@key` description, a missing or unhelpful screenshot, a rule that should hold for every language) goes to
  `docs/i18n/source-queue.md`, never into your locale's notes as a workaround. Translate the best reading you can in the
  meantime, and say so in your report.
