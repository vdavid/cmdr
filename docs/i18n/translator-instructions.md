# Translator instructions

The one source of the translator agent's standing instructions. `pnpm i18n:brief` embeds everything under
`## Instructions` in every brief, with `{{LANGUAGE}}` and `{{TAG}}` filled in, so a translator reads them there and
nowhere else. The shared judgment calls follow them in the brief, from `translation-principles.md`. Edit them here; the
process around them is `docs/guides/i18n-translation.md` § The translator-agent context.

## Instructions

You are translating UI strings for Cmdr, a file manager for macOS, Windows, and Linux, from English into {{LANGUAGE}}.
This brief holds everything you need: the keys with their context, the style digest, the ruling for every term in play,
the nearest shipped translations, and the past decisions about these keys. The Principles section right after these
instructions holds the judgment calls every language shares (voice, names vs prose, no hedged grammar, native
typography, escalating source problems); they bind as much as these instructions do. Also read the full style guide
(docs/i18n/{{TAG}}/style.md) before writing. A term with a ruling below is settled: use it. Reopen a ruling only with
new evidence, and then edit it in place (the old form moves to "avoid" with its reason). Where a ruling's form and the
style guide disagree, the style guide's general rule wins unless the ruling says why it's an exception; flag it.

- **Where to write**: edit each value in place in apps/desktop/src/lib/intl/messages/{{TAG}}/<area>.json. Then stamp
  what you translated as current: in apps/desktop, run
  `node scripts/sync-locale-keys.ts {{TAG}} --restamp <key> --restamp <key> …` (one `--restamp` per key; it only moves a
  hash that's out of date). A key that arrived as a fresh English skeleton already carries the current hash, so
  restamping it changes nothing; it matters for a key whose English changed under an existing translation. It clears
  `reviewed` and `sameAsSourceJustification` on the keys it moves, so add a justification after restamping, never
  before.
- **Search keywords** (`settings.*.keywords`): a hidden, comma-separated list Settings search matches, never shown.
  Search already matches the setting's label and description, so write OTHER words a user of your language might type:
  synonyms, everyday words, the related idea, any count, not a 1:1 translation. Term rulings, avoid-lists, and brand
  names don't apply (brands, protocols, and file formats are matched in every language already), and there's nothing to
  mine the pile for. For Chinese, Japanese, and Korean, prefer short, common words: a CJK query only finds a keyword
  that contains it unbroken, with no typo tolerance.
- **ICU**: keep every {placeholder}, <tag>…</tag>, and plural/select structure exactly; translate only the text between
  them. Reorder placeholders only as grammar needs; the set stays identical. Write plural branches for YOUR language's
  CLDR categories, not English's. A CLDR `one` isn't "exactly one" (Russian `one` also covers 21 and 101): wording
  that's only right for a single item goes in an `=1` arm, and the displayed count stays in the ordinary `one` branch.
  Double every apostrophe (' becomes '').
- **Raw families (no ICU)**: `errors.*` and the native `menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`. Use
  NORMAL apostrophes (a doubled '' renders as two on a real menu), keep {token} verbatim, treat <…> as literal text, and
  pass markdown through untouched.
- **Per-OS values** (`{platform, select, macos {…} windows {…} other {…}}`): one branch per OS. `macos` is macOS,
  `windows` is Windows, `other` is Linux, plus whichever of the two has no branch of its own. Keep the frame and the
  branch names exactly. **In a platform select, each branch uses that OS's own localized term (Windows' own word for the
  Recycle Bin, File Explorer, Settings), sourced from that OS's localization, never the macOS term carried over.**
  - **The branch macOS reads, once in {{LANGUAGE}}, stays byte for byte**: that's `macos`, or `other` in a block with no
    `macos` branch (each key's note in the brief says which). A key that became per-OS kept its translation there, and
    `macos-copy-unchanged.test.ts` fails on any change. Your work is every branch still in English (all of them, for a
    new key); any other branch already in {{LANGUAGE}} whose English didn't change stays too.
  - **Where each OS's word lives** in the pile: `macos` mines `macOS/`; `windows` mines `microsoft-terminology/` (the
    TBX) and `microsoft-style-guides/`; `other` mines `gnome-nautilus/`, `kde-dolphin/`, and `xfce-thunar/`, taking the
    word they share, or the desktop-neutral one where they split (GNOME- or KDE-only words wait for per-desktop text).
    The TBX has no "This PC" or "user folder", and its "home folder" is a different sense (a folder an admin assigns on
    a server); where the pile lacks the OS's word, choose by the evidence you have and record it `tentative`.
  - **Two registers on Windows**, which the English branch already picks: Trash is "Delete" as an action's name (a
    button, menu row, or command) and "the Recycle Bin" in a sentence; Quit is "Exit" as an action and "close" when Cmdr
    is the subject ("Cmdr closes"). Use {{LANGUAGE}} Windows' own word for each, in the same register.
  - **Rulings**: a `terms.json` ruling may carry `windows` and `other` forms, and the brief prints them. When a branch's
    OS names a ruled concept differently from the base ruling, add that OS's form (docs/i18n/termbase.md, `terms.json`
    schema) with sources from that OS's folder.
  - **Done**: once every branch is in {{LANGUAGE}}, restamp the key as above; that's what takes it off the stale list.
  - **A plain value** (no select) shows on every OS. Use the word the OSes share in {{LANGUAGE}}; where they split, keep
    the macOS term and log the key in docs/i18n/source-queue.md as needing a per-OS value.
- **Uncontrolled inserts**: {message}, {reason}, and a raw {path} carry text Cmdr doesn't control. Make the sentence
  read right whatever lands there; never assume its gender, number, case, or length.
- **Fragment keys**: a fragment assembled by a named `*Join` key must read naturally once assembled; the `*Join` key is
  where your word order goes.
- **Native menu labels** (`menu.*`): no screenshot exists, so the description is the whole aid (VERB or NOUN, which
  menu, the Finder counterpart). Mine the label from the localized OS menus (docs/i18n/reference-pile/how-to-mine.md §
  Menu-bar labels). Keep a trailing ellipsis, written the way your mechanics file declares it. Don't write & or _
  mnemonics.
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
  translated, Spotlight isn't); (2) each OS's own term in its branch (Per-OS values above), and in a plain value the
  macOS Finder term where the OSes split; (3) let brands inflect by PRONUNCIATION ("Cmdrben", "Cmdrs").
- **Reference pile (mandatory for "no ruling" terms)**: for every term the brief marks "no ruling" and every content
  word it lists under "No concept yet", mine the pile at the header's path (it lives only in the MAIN clone, never your
  worktree) for how macOS, Microsoft, Nautilus/Thunar/Dolphin, and Total/Double Commander render it; reuse and cite,
  never guess. Two-pane concepts (pane, file list, command line) come from the orthodox pair. Recipes:
  docs/i18n/reference-pile/how-to-mine.md.
- **Write back** what you settle: a ruling to docs/i18n/{{TAG}}/terms.json (chosen, sources, confidence, plus
  accept/proseAccept/forms/avoid), a new concept to docs/i18n/concepts.json, an English-sense exclusion to that
  concept's "notMatch", a language-specific boundary to the term's "exceptions" with the reason, and anything only a
  native reviewer can settle to docs/i18n/{{TAG}}/review-queue.md.
- **decisions.md holds distilled rulings only**: "X over Y because Z", citing the keys in backticks in its heading, at
  most ~3 lines. When a ruling changes, edit or replace its entry and delete what it supersedes; never append a
  narrative, a dated story, or a second entry beside the old one. The file has a byte budget (`i18n-termbase` warns past
  it): distill first, and bump it when new decisions need room. Anything that applies beyond one language goes to
  docs/i18n/source-queue.md as a proposal to promote, never into this language's files.
- **Check**: run `pnpm i18n:check-locale {{TAG}}` in `apps/desktop` (several tags work too). It prints one line per
  clean check and the full report only for one that isn't; the batch is done when every line is a ✓. It's the one check
  you need: `pnpm check --fast` runs most of the same checks across every locale but skips the termbase.
- **Report**: your output may ship without human review, so translate only what you're confident in and flag every
  string where the context was insufficient. End with a **Source feedback** section: `@key` descriptions that were
  insufficient, screenshots that were missing or unhelpful, English that was ambiguous or inconsistent, and proposed
  cross-language rules (log each in docs/i18n/source-queue.md too), or "none".
