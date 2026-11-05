# Translation principles

The judgment calls every language shares. `pnpm i18n:brief` embeds everything under `## Principles` in every brief,
right after the translator instructions, which own the mechanics (ICU, raw families, uncontrolled inserts, aria
containment); don't restate those here. Edit the principles here, most important first, one bullet per rule, and keep
them language-neutral: a rule only one language needs belongs in that language's `style.md`. New shared rules arrive
through `source-queue.md`.

## Principles

### Voice and meaning

- **Voice**: Cmdr is casual, friendly, and plain. When two forms are both correct, pick the everyday one over the formal
  or bookish one.
- **Add nothing English doesn't say**: a vague amount stays vague ("a few days" and "for a while" never become a
  number), an unknown count stays number-neutral, and a time English leaves open stays open.
- **Doing/done pairs share a verb**: a done label must read as finished. Where your language's past form of the English
  verb doesn't ("searched" with no natural finished participle), use a nearby verb ("looked up", "went through"), as
  long as the doing and done lines of the pair use the same one.
- **No "try again" where retrying can't help**: a deterministic refusal (not found, no permission) says what the user
  can do instead.
- **Escalate, don't work around**: a problem in the English source (inconsistent or ambiguous English, a weak or wrong
  `@key` description, a missing or unhelpful screenshot, a rule that should hold for every language) goes to
  `docs/i18n/source-queue.md`, never into your locale's notes as a workaround. Translate the best reading you can in the
  meantime, and say so in your report.

### Names and consistency

- **Names vs prose**: a term ruling locks the NAME of a thing: menu items, settings, commands, and anything the user
  also sees in macOS or Finder (app names, Finder tag colors). Names never drift. In running prose (descriptions,
  messages, explanations) you may use a natural everyday synonym instead of the ruled form when it reads more casual and
  is equally clear, but never for a name. A synonym you'd reuse goes in the ruling's `proseAccept`.
- **Quote a label exactly**: text that names a button, setting, menu command, dialog, or pane copies that label byte for
  byte: from your own catalog for a Cmdr surface, from the localized OS for a macOS one, and from the device's own UI
  for a third-party one (Android's AOSP strings count as Tier 1, like Apple's). A synonym sends the reader hunting for a
  control that doesn't exist. A quoted third-party label (a NAS's switch name) is exempt from your avoid-lists: quoting
  it exactly wins.
- **Command descriptions follow your macOS help tags**: the English palette descriptions speak in the third person
  (“Opens…”), like English macOS help tags; write yours in the form your language's macOS help tags use (Russian uses
  the infinitive).
- **Show-the-result labels**: a label that displays its own effect (“lowercase”, “UPPERCASE”, “Every Word Capitalized”)
  mirrors that casing and is exempt from sentence case. A script without letter case (Chinese, Japanese, Korean)
  describes the transform in words instead.
- **Same English, same translation**: identical English gets a byte-identical translation, and variants of one sentence
  share their frame character for character (edit the nearest sibling and change only what English changed). When one
  member of a visible family changes, realign the whole family, and re-check a label's `*Aria` sibling in the same edit.
- **Count before you coin**: before writing a new form, count how often your catalog already uses the settled one; for
  which of two correct forms this app uses, the catalog outranks the pile. One control keeps one name even where English
  uses two.

### Placeholders and counts

- **No hedged grammar**: never write a parenthesized or slashed alternative to dodge grammar you can't know (the English
  "file(s)" and its cousins in every language: a plural, article, gender, or case ending in brackets, or an ending that
  depends on how an inserted value is pronounced). When Cmdr knows the variable, use ICU `plural` / `select`; when its
  values are a closed set Cmdr supplies, agree with them. Otherwise rephrase: a colon form ("Name: {name}"), the
  placeholder placed where no article or agreement touches it, or the object's type named first ("the folder {name}").
  Never endorse a hedge in a style guide.
- **No pronoun for an insert**: never point a pronoun or possessive back at an inserted name, path, or app; repeat the
  noun or use a pronoun-free form. Never let a pronoun reach past a nearer noun of the same gender. The rule bites where
  the pronoun carries a gender or number the insert may not match (French « il »); a pronoun that carries neither
  (Traditional Chinese 它) is fine. Dropping a pronoun for a quoted name never forbids the object's type noun in front
  of it (“the phone “{device}”…”).
- **Agreement-bound inserts may move to a colon slot**: when a word before an insert must agree with it (an article that
  depends on pronunciation, a case ending), move the insert after a colon (“Up to version: {ceiling}”).
- **A pronoun-only count may go unused**: when `{count}` only picks a pronoun (“move {count, plural, one {it} other
  {them}} back”) and your language has no grammatical number, write one wording in every branch and leave the number
  out.
- **First-person labels may go impersonal**: a checkbox or disclosure the user “speaks” (“I’ve checked the key”) may use
  an impersonal form where the first person would gender the user (a Slavic past tense).
- **Counts**: no definite article before a formatted count ("the {countText} items" renders "the 1 item"); carry
  totality another way. A clause that must agree with the count goes inside the plural branches, and an `=0` / `=1` arm
  is only for different wording ("Skip" vs "Skip all").
- **Colon frames**: a value spliced after a frame's colon reads as a complete unit and doesn't repeat the frame's words,
  unless one value serves several frames.
- **Counted nouns without a plural**: the English passes a plural wherever ICU can take one; a counted noun with none is
  a source bug to queue. Only a native menu label (`menu.*`), which can't pluralize, takes a counted-label colon form
  (“Items: {count}”).
- **Search keywords cover every label**: when one setting shows different labels per context (a provider-specific name),
  its `*.keywords` carry every label the search label doesn't, so a search for the label on screen finds it.
- **Example addresses**: keep RFC 2606 `example.com`; only the local part may translate.

### Typography

- **Native typography, as your language's macOS writes it**: its quotation marks (primary and nested, also around text
  the user types), apostrophe, ellipsis glyph (never three dots; keep a trailing one wherever English has it,
  placeholder examples included), required spacing (such as a non-breaking space before certain punctuation or `%`), and
  list and number punctuation. Straight ASCII quotes (`"`) never appear in UI text. Your locale's `mechanics.json`
  declares these, and `i18n-mechanics` checks the catalog against it.
