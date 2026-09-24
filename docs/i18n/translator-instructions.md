# Translator instructions

The one source of the translator agent's standing instructions. `pnpm i18n:brief` embeds everything under
`## Instructions` in every brief, with `{{LANGUAGE}}` and `{{TAG}}` filled in, so a translator reads them there and
nowhere else. Edit them here; the process around them is `docs/guides/i18n-translation.md` § The translator-agent
context.

## Instructions

You are translating UI strings for Cmdr, a macOS file manager, from English into {{LANGUAGE}}. This brief holds
everything you need: the keys with their context, the style digest, the ruling for every term in play, the nearest
shipped translations, and the past decisions about these keys. Also read the full style guide
(docs/i18n/{{TAG}}/style.md) before writing. A term with a ruling below is settled: use it. Reopen a ruling only with
new evidence, and then edit it in place (the old form moves to "avoid" with its reason). Where a ruling's form and the
style guide disagree, the style guide's general rule wins unless the ruling says why it's an exception; flag it.

- **Where to write**: edit each value in place in apps/desktop/src/lib/intl/messages/{{TAG}}/<area>.json. When you
  translate or re-translate a key, set its `@key.sourceHash` to the hash of the current English value in the same edit.
- **ICU**: keep every {placeholder}, <tag>…</tag>, and plural/select structure exactly; translate only the text between
  them. Reorder placeholders only as grammar needs; the set stays identical. Write plural branches for YOUR language's
  CLDR categories, not English's. Double every apostrophe (' becomes '').
- **Raw families (no ICU)**: `errors.*` and the native `menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`. Use
  NORMAL apostrophes (a doubled '' renders as two on a real menu), keep {token} verbatim, treat <…> as literal text, and
  pass markdown through untouched.
- **Uncontrolled inserts**: {message}, {reason}, and a raw {path} carry text Cmdr doesn't control. Make the sentence
  read right whatever lands there; never assume its gender, number, case, or length.
- **Fragment keys**: a fragment assembled by a named `*Join` key must read naturally once assembled; the `*Join` key is
  where your word order goes.
- **Native menu labels** (`menu.*`): no screenshot exists, so the description is the whole aid (VERB or NOUN, which
  menu, the Finder counterpart). Mine the label from the localized OS menus (docs/i18n/reference-pile/how-to-mine.md §
  Menu-bar labels). Keep a trailing … (U+2026). Don't write & or _ mnemonics.
- **Aria labels**: a `*Aria` value must CONTAIN its visible label's words verbatim and in order (WCAG 2.5.3); case may
  differ. Pick the label's form to be the one the natural aria sentence uses, then cut the label from it. Say which
  substring satisfies containment, and record the pair in the term's "note".
- **Gender**: neutral RESTRUCTURING, never glyphs (no *innen, ·e·, -x, schwa). Name the object or action, not the
  person. If neutral phrasing would be stilted, flag the string instead.
- **Don't translate**: Cmdr, macOS, GitHub, SMB, MTP, and the {system_settings}-style tokens (the full list:
  BRAND_WORDS + SYSTEM_TOKENS in apps/desktop/scripts/i18n-catalog-lib.ts).
- **Deliberately identical**: when a value is correctly identical to English in your language (a brand, a unit, a
  placeholder-only string, a shared word), record a short, sourced `@key.sameAsSourceJustification` in your catalog
  instead of forcing a change.
- **Term choice**: (1) localize what Apple localizes, verified per locale in the pile's macOS folder (Quick Look is
  translated, Spotlight isn't); (2) prefer the macOS Finder term over the Windows one; (3) let brands inflect by
  PRONUNCIATION ("Cmdrben", "Cmdrs").
- **Reference pile (mandatory for "no ruling" terms)**: for every term the brief marks "no ruling" and every content
  word it lists under "No concept yet", mine the pile at the header's path (it lives only in the MAIN clone, never your
  worktree) for how macOS, Microsoft, Nautilus/Thunar/Dolphin, and Total/Double Commander render it; reuse and cite,
  never guess. Two-pane concepts (pane, file list, command line) come from the orthodox pair. Recipes:
  docs/i18n/reference-pile/how-to-mine.md.
- **Write back** what you settle: a ruling to docs/i18n/{{TAG}}/terms.json (chosen, sources, confidence, plus
  accept/forms/avoid), a new concept to docs/i18n/concepts.json, an English-sense exclusion to that concept's
  "notMatch", a language-specific boundary to the term's "exceptions" with the reason, rationale worth more than a line
  to docs/i18n/{{TAG}}/decisions.md under a heading citing the keys in backticks, and anything only a native reviewer
  can settle to docs/i18n/{{TAG}}/review-queue.md.
- **Check**: run the i18n checks in docs/guides/i18n-translation.md § Add a new language, step 5.
- **Report**: your output may ship without human review, so translate only what you're confident in and flag every
  string where the context was insufficient.
