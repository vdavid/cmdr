# Translation learnings (shared, cross-language)

Cross-language know-how for whoever leads or audits a translation pass: failure shapes, evidence shortcuts, and
orchestration gotchas that aren't rules. Where the rest lives:

- Rules every translator follows: `translator-instructions.md` (mechanics) and `translation-principles.md` (judgment
  calls), both embedded in every brief. Proposals to add one go to `source-queue.md`.
- Process: `docs/guides/i18n-translation.md`. Termbase and tooling: `termbase.md`. Mining recipes and source traps:
  `reference-pile/how-to-mine.md`.
- Per-language findings: that language's `style.md`, `terms.json`, and `decisions.md`.

## Failure shapes the checks can't see

Parity, ICU, plural, stale, coverage, and don't-translate are structural, so these pass them all:

- **Mid-feature drift.** Keys an implementation agent adds in passing are fluent but drift from what the locale settled:
  the agent copies neighboring keys and can't see that the feature's Settings pane already ships another word. The drift
  is per FEATURE (one wrong head term everywhere) and a batch drifts against itself too (pt `alteração de nome` vs
  `renomeação`). `grep -oh '"<variant>"' <tag>/*.json | sort | uniq -c` shows the settled term at 15–60 hits and the
  drifted one at exactly the batch size.
- **Wrong regional variant.** A fully pt-PT string is structurally perfect. Pluricentric languages need a grep-list of
  variant tells in their style guide (vocabulary plus one grammatical construction); a recorded variant decision alone
  doesn't reach an implementation agent's defaults.
- **Formality regression.** One `tu` in an otherwise `vous` catalog. Spot-check new keys in any T/V language against
  `formal-informal-decisions.md`.
- **An elided verb in an uninflected language.** vi rendered "may or may not be covered" as "may have or not", still
  fluent. It bites on transparency surfaces, where the elided word was the point.
- **A stranded clause after editing a variant.** Languages front adverbials English keeps medial, so dropping a clause
  from a sibling copy can strand the rest (de "macOS hat **in den letzten X Stunden** einmal …"). Re-read the remainder
  as a standalone sentence.

## Copy shapes that drift

- **A warning badge is a state label, not an action.** "(overwrite!)" is noun/verb ambiguous in English; a language with
  a distinct imperative produces a badge telling the user to do what the row blocks. Keep a badge family one part of
  speech.
- **A doing/done pair needs a grammatical contract.** In pro-drop languages a bare finite past reads as "he/she did it"
  (es "Preparó"), so the done arm is impersonal, passive, or participial. In case-marking languages, state the contract
  in the style guide (de: "done = doing minus the auxiliary, subject in the nominative").
- **Composed one-line strings keep separators inside the branches.** In a fact list
  (`{label}{count, plural, =0 {} other { {countText} items}}… · {percentText}%`) each optional clause carries its own
  leading space and separator. Verify by assembling every present/absent combination. Moving the count out of the label
  into its own `·` fact is a legitimate restructure (de, nl, zh, and hu all did it, each for a grammatical reason).
- **A verb that doubles as a field label on the same screen takes its sense-verb.** "are what name this server", on a
  sheet with a `Name` field (`servers.sheet.identityLocked`), became IDENTIFY or DETERMINE in every locale.
- **A near-synonym pair only English keeps apart** (row/line, folder/directory, item/entry): drop the second noun and
  say WHERE ("the line continues directly below") before minting a new term per locale.
- **Don't fuse the subjects of "X did this, and so did N others"** when a `{reason}` describes only X: merging widens
  the reason to all N+1 files (`fileExplorer.rename.chainKeptOriginalNameAndOthers`). Keep the tail its own clause.

## Renaming a product noun every locale already translated

A rename isn't a copy edit, and `--restamp` is the wrong tool: the hash catches up while every catalog keeps the old,
narrower meaning.

- **Anchor on the locale's own catalog.** A sibling surface on the same head noun settles the term before mining (the
  queue window's rename took each locale's head noun from its shipped **Operation log** window). State that anchor in
  each brief, so every agent confirms it instead of rediscovering it.
- **A swapped noun drags grammar with it**: gender, articles, clitics, pronouns, and case suffixes around the old noun
  (de `die Übertragung` → `der Vorgang` changed four arias and two pronouns). The checks can't see it.
- **The old term is superseded, not retired**: it often still names the narrower concept one level down. Mark its
  glossary entry superseded in place; deleting it or leaving it prescriptive both undo the rename on the next pass.

## Quoting a macOS UI label

The decision is per LABEL (`docs/guides/i18n-translation.md` § Term-choice principles). Settle one by direct key match
in the shipped Finder bundle, with `en_GB.lproj` as the English side (`reference-pile/how-to-mine.md` § Menu-bar
labels):

```sh
cd "/System/Library/CoreServices/Finder.app/Contents/Resources"
plutil -convert json -o - en_GB.lproj/MenuBar.strings                   # 300801.title = "Get Info"
plutil -convert json -o - <tag>.lproj/InfoWindowGeneralView.strings     # 1073.title = the Locked checkbox
plutil -convert json -o - <tag>.lproj/InfoWindowPermissionsView.strings # 6.title = Sharing & Permissions
plutil -convert json -o - <tag>.lproj/LocalizableMerged.strings         # N30/N32/NE43/NE18 = running-text sentences
```

Prefer the running-text form (`LocalizableMerged`) when our string is a sentence and the widget form when it's a label:
Apple shortens headings (hu `Megosztási jogok:` in the panel vs `Megosztás és jogok` in prose). Nib IDs are
undocumented, so record the macOS version (verified on macOS 26.5.2, 2026-08-24).

⚠️ **These labels follow the SYSTEM language; our catalog follows the APP language.** A user running Cmdr in Spanish on
a Hungarian Mac reads a Spanish `Obtener información` their Finder calls `Infó megjelenítése`. Catalog translation is
right for the common case; the proper fix (sourcing from the OS) is in `apps/desktop/src-tauri/src/system_strings.rs` §
"Finder labels: why they're catalog strings, not OS-sourced".

## Evidence shortcuts

- **"Review pending changes before applying"**: AppKit's `Review Changes…` / `Review Unsaved` exist in every localized
  macOS. Microsoft's TBX first hit for "review" is the evaluation sense.
- **`remove` vs `delete` is a systematic trap.** macOS and Microsoft often use one verb for both, so Tier 1 pushes a
  "delete" verb onto a button that deletes nothing. Check the pair per locale (es `Quitar`, fr `Retirer`, vi `Gỡ`).
- **Finder's tag strings** are the `TG*` keys in `LocalizableMerged.strings` (`TG5` / `TG6` add/remove, `TG_COLOR_*`
  colors, `N169.37` bare "Tags"). `TG6` uses the DELETE verb in fr, es, and vi; those catalogs keep their un-list verb
  anyway, and their own quote typography. An exact Finder counterpart doesn't override a settled split (verified on
  macOS 27.0, build 26A428, 2026-09-16).
- **"…and N other items"**: Finder's `MR101_V2/_V3`, `MR201_V2/_V3`, and `PE106_V3/_V4` ship singular and plural of
  `“^1” and ^0 other item(s)`, showing how a language counts the followers and whether it repeats the verb.
- **SMB connection copy**: NetAuthAgent's `Localizable.loctable` keys `EINFO_NO_SHARE`, `EINFO_NO_ACCESS_GUEST`,
  `EINFO_UNSUPPORTED_VERSION`, `EMSG_INVALID_NAME_PWD`, and `EINFO_NO_SERVER` settle share, guest, and the pattern
  around a quoted server name (verified on macOS 26.6.2, build 25G83, 2026-09-11). Take the terms, not Apple's formal
  register.
- **A Linux "distribution" has no source.** Microsoft's TBX has only the logistics sense; take the community standard
  (`Distribution`, `disztribúció`, `发行版`) as `tentative`.

## Orchestration gotchas

- **Restamp centrally.** Translators write values only; the lead runs `sync-locale-keys.ts --restamp <key>` once for
  every locale (check first that no key carries `reviewed` / `sameAsSourceJustification`, which it drops). Then
  `git diff` on the catalogs is pure value lines, an exact scope audit. `--restamp` also SYNCS pending new keys as
  English skeletons; strip them if they belong to a later commit (a `JSON.stringify(…, null, 2) + '\n'` round-trip is
  byte-identical).
- **Give each parallel agent its own scratch directory.** Helper scripts in a shared `/tmp` overwrote each other and one
  agent got German answers to Dutch queries. Sanity-check that a mining helper answers in your language.
- **Hardcode a Workflow's batch spec; don't pass it via `args`.** `args` arrived as a JSON string, the loop iterated its
  characters, and ~848 agents spawned with `tag = undefined` (~36M tokens). Assert every tag is known and its locale dir
  exists before spawning anything.
