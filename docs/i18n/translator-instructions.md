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

- **Where to write**: put your values in one JSON file (key → value) and write them with
  `pnpm i18n:apply --lang {{TAG}} --values <file.json>` in apps/desktop. It writes each value into its catalog file,
  stamps the key as translated from the current English, and refuses the whole batch (changing nothing) when a value
  breaks a placeholder, a tag, ICU, the per-OS block shape, the plural set, or the macOS text a Mac already reads; fix
  what it names and run it again. A per-OS key can be given as `{ "windows": "…", "other": "…" }`: each branch you leave
  out keeps the text that OS reads now, which is how the `macos` branch stays untouched. A deliberate identical value
  takes `"@<key>": { "sameAsSourceJustification": "…" }` in the same file. `--dry-run` checks without writing, and
  `--expect-keys <file>` (one key per line) lists the batch keys your file missed. A key the catalog doesn't have yet is
  refused with the `sync-locale-keys.ts` call that adds it.
- **Tools, not scripts**: the tools below cover writing and looking up. ❌ Never write your own script that writes a
  catalog or a termbase file, and never hand-edit one: the tools validate and keep the formatting, and a homemade writer
  does neither. When a tool can't do what you need, do the smallest manual step and say so under Process feedback
  (below).
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
  - **A macOS text that's wrong** (a mistranslation, an outdated term, or macOS vocabulary where the English is neutral,
    like `menu.window.minimize` as "Im Dock ablegen") is the one exception: fix it, and add an entry to
    `ACCEPTED_TRANSLATION_CHANGES` in `apps/desktop/scripts/i18n-macos-copy-baseline-lib.ts` with this catalog's tag,
    the key, the new macOS text exactly as the catalog spells it, and a one-line reason. That entry is what review
    reads, so the reason says what was wrong. ❌ Never edit or regenerate the baseline, and ❌ never fix a macOS text
    just to restyle it: a Mac user already reads it.
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
  - **Done**: once every branch is in {{LANGUAGE}}. `i18n:apply` writes a half-done key but leaves its stamp, so it
    stays on the stale list and the output names the branches still in English; a branch that's right as English needs a
    `sameAsSourceJustification` in your values file.
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
  word it lists under "No concept yet", mine the pile for how macOS, Microsoft, Nautilus/Thunar/Dolphin, and
  Total/Double Commander render it; reuse and cite, never guess. Two-pane concepts (pane, file list, command line) come
  from the orthodox pair. `pnpm i18n:pile --lang {{TAG}} --term "<English>"` (in apps/desktop) runs the lookups in one
  call from your worktree: macOS by key, every Microsoft sense with its definition, the Linux and orthodox catalogs;
  `--target "<word>"` looks up in reverse, `--key <key>` takes a key's English. The pile lives only in the MAIN clone,
  and the tool finds it, so never name that path in a shell loop. Recipes for what it doesn't cover (the live macOS
  bundles, the style-guide PDFs): docs/i18n/reference-pile/how-to-mine.md.
- **Term lookup**: `pnpm i18n:term <word>` prints every concept the word names, each sense's definition and boundaries,
  and every language's ruling; `pnpm i18n:term --key <key>` shows which concepts a key's English hits and which a
  `notMatch` keeps out (the first question when the termbase check flags a key you think it misreads).
- **Write back** what you settle with `pnpm i18n:apply-terms --lang {{TAG}} --patch <file.json>`: a JSON file of concept
  ID → the fields to change (chosen, sources, confidence, plus accept/proseAccept/forms/avoid/note). Each field you give
  replaces the stored one and `null` deletes it; `exceptions` merges key by key; a list may be
  `{ "add": […], "remove": […] }`. It refuses a patch that breaks the termbase schema and keeps the file's formatting. A
  new concept goes to docs/i18n/concepts.json (`--concepts`), an English-sense exclusion to that concept's "notMatch"
  (`--concepts`, also a key-scoped `{ "key", "why" }`), a language-specific boundary to the term's "exceptions" with the
  reason, and anything only a native reviewer can settle to docs/i18n/{{TAG}}/review-queue.md.
- **decisions.md holds distilled rulings only**: "X over Y because Z", citing the keys in backticks in its heading, at
  most ~3 lines. When a ruling changes, edit or replace its entry and delete what it supersedes; never append a
  narrative, a dated story, or a second entry beside the old one. The file has a byte budget (`i18n-termbase` warns past
  it): distill first, and bump it when new decisions need room. Anything that applies beyond one language goes to
  docs/i18n/source-queue.md as a proposal to promote, never into this language's files.
- **Check**: run `pnpm i18n:check-locale {{TAG}}` in `apps/desktop` (several tags work too). It prints one line per
  clean check and the full report only for one that isn't; the batch is done when every line is a ✓. It's the one check
  you need: `pnpm check --fast` runs most of the same checks across every locale but skips the termbase.
- **Report**: your output may ship without human review, so translate only what you're confident in and flag every
  string where the context was insufficient. End with two sections, each "none" when it's empty:
  - **Source feedback**: `@key` descriptions that were insufficient, screenshots that were missing or unhelpful, English
    that was ambiguous or inconsistent, and proposed cross-language rules. Log each in docs/i18n/source-queue.md too.
  - **Process feedback**: every script you wrote (even a one-off lookup) and why, every workaround (a command a guard
    refused, a manual step a tool couldn't take, an instruction that didn't fit), and what would have saved you the
    time. Log each under `## Process feedback` in docs/i18n/source-queue.md too. Two translators writing the same script
    is a missing tool, and only the lead can see that pattern, so leave nothing out because it felt routine.
