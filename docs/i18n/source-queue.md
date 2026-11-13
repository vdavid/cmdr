# Source queue

Translators log English-side problems here while translating: ambiguous or inconsistent English, a weak `@key`
description, a missing screenshot, or a rule every language should follow. One bullet per item, at most two lines: the
key(s), what's wrong, the suggested fix. The lead fixes the English, the description, or the shared docs (promoting a
rule into `translation-principles.md` or `translator-instructions.md`), then deletes the entry; git keeps the history.
Process feedback (a translator's homemade scripts and workarounds) goes under its own heading at the end.

## Open

- Screenshots: `mtp-browse.png` doesn't show the `mtp.readiness.connecting` row, and the AI suggestions dialog keys ship
  without any (the pills and fact lines need them most). Needs a `pnpm i18n:shots` run with the Mac left alone. (all)
- Counted nouns in the RAW `errors.*` family can't take an ICU plural, so Slavic locales still need a colon form:
  `errors.write.trashRefused.message.*`, `errors.write.deviceDisconnected.sided.*`,
  `errors.eject.unmountRefusedByProcesses`. Either word around the count, or give the raw path a plural. (uk)
- `archive` concept: the zip-noun keys (`fileExplorer.list.rowKind.archive`, `suggestedOps.facts.archive`) now leave
  `archive-chat`, but `archive`'s own `notMatch` drops any value that's exactly "Archive", so they match neither. Needs
  a key-scoped `match`. (all)
- `menu.window.zoom` and `menu.view.zoom` are both "Zoom" in English, so cs, fr, and uk keep a `zoom` exception; split
  the English or accept it. Needs David: it renames a menu item. (uk)
- The `remove` / `original` concepts match `operationLog.tree.statusRemoved`, `skipReason.locked`, and
  `skipReason.restoreTargetOccupied`, which their own descriptions exclude. (cs, de, es, fr, hu, nl, pt, ru, sv, uk, vi,
  zh, zh-Hant)
- `operationLog.dialog.selectedCount` drives its plural with the TOTAL, but "selected" agrees with the checked count
  (`3 sur 12 sélectionnés`), so fr falls back to a colon form. Pass the checked count as the plural driver: a code
  change plus a new key, since translations agree with the old driver. Likewise `operationLog.tree.backfillProgress` has
  no plural driver at all for "Read {doneText} … items". (cs, es, fr, pt, sv)
- `search.coverage.line.uncovered.*`: `{drive}` is either a name or the generic phrase from
  `search.coverage.unnamedDrive`, so case-marking languages read stiffly ("ezt: ez a meghajtó"), and in `.unavailable`
  it actually holds a SERVER name. Give the generic case its own variant key, or a server param of its own. (hu, ru)
- `look-inside` is defined as the AI reading a file, yet `search.coverage.line.denied` / `.declined` use it for a walk
  entering folders: widen or exclude. (cs, ru)
- "system" (the OS, `search.coverage.line.offLimits`) has no concept yet, and every locale coined its own word; add one
  sourced from Apple's localized macOS. (sv, zh, zh-Hant, uk)
- vi: `search.coverage.deniedFullDiskAccess` and `search.coverage.setUpFullDiskAccess` say `quyền truy cập toàn bộ đĩa`,
  but Apple's pane is `Quyền truy cập đầy đủ vào ổ đĩa` (vi `style.md`). Re-translate both. (vi)
- "Couldn't X" sentences naming Cmdr as the subject (`search.coverage.line.failedRoots`) collide with a style guide's
  impersonal pattern ("X ließ sich nicht öffnen") because `dont-translate` demands "Cmdr". Decide whether an actor-only
  "Cmdr" may drop in translation. Needs David. (de)
- `pnpm i18n:brief`'s regional-overlay section names vocabulary forks only; `es-419` also forks the compound perfect to
  the preterite ("no pudo"), which cost a failed check. Name tense forks there. (es)
- Briefs for brand-new keys: run `sync-locale-keys.ts` before `pnpm i18n:brief`, or the brief describes skeletons the
  catalog doesn't hold yet. (cs, de, es, fr, nl, pt, sv)
- `ask` concept matched "Your settings ask for" (`settings.ai.provider.localUnavailable`), the request sense; added
  `settings ask for` to its `notMatch`. A broader "ask for" exclusion needs a call, since prose "asks for your password"
  sits in the concept. (all)
- `settings.fileExplorer.typeToJump.mode.description`: the `other` branch names the PC's Backspace key, but no concept
  covers it (`delete-key` is the Mac ⌫ key), so each locale records its PC key name ad hoc (es put `Retroceso` in
  `delete-key`'s `other` form). Propose a `backspace-key` concept. (es)
- `recent` matches "recently closed tabs" (`settings.fileExplorer.tabs.closedTabHistorySize.description*`), which vi
  renders as "just closed" (`vừa đóng`), as its `commands.handler.noRecentlyClosedTabs` already does under an exception.
  Only vi deviates, and the concept's sense names recently closed tabs, so it stays a vi exception unless another locale
  does the same. (vi)
- The pile lacks the KDE Frameworks catalogs (`kxmlgui`, `kconfigwidgets`), where KStandardAction labels like "Full
  Screen Mode" live, so `i18n:pile` finds nothing for them and a Linux branch falls back to the base ruling. Add them to
  the pile beside Dolphin. (de, es, cs, hu, nl, vi, zh, zh-Hant)
- The pile has no Windows shell label "This PC" (the Microsoft TBX lacks it), so the `windows` branches naming it rest
  on judgment and stay `tentative`. Source it from a localized Windows, or a Microsoft support page per language. (es,
  hu, vi)
- `errors.listing.authRequiredEneedauth.suggestion`: the `other` branch serves macOS AND Linux but says "in Finder", so
  a Linux user is sent to Finder. Give it a `macos` branch and a generic `other` ("in your file manager"). (hu)
- Windows shell labels the pile can't confirm: "Associated Handles" (Resource Monitor), "Empty Recycle Bin", the
  "Read-only" attribute checkbox, "Tools > Check", "user folder", and Settings > System > "Storage" have no exact TBX
  hit, so every locale builds them from parts. Source them once from a localized Windows per language. (hu, de, cs, uk)
- `quoted-labels` pairs the Windows branch's “Read-only” (File Explorer's attribute checkbox) in
  `errors.write.permissionDenied.suggestion.delete` and `errors.write.fileLocked.suggestion` with Cmdr's own
  `errors.listing.readOnly.title`, so quoting Windows' real label (cs „Jen pro čtení“) fails the check against Cmdr's
  „Jen ke čtení“. Give the check a per-key opt-out, or skip pairs where the quote names another OS's control. (cs, uk:
  quoted Cmdr's «Тільки читання» to pass, where Windows likely says «Лише для читання»)
- `allow` matches "wasn’t allowed to" / "isn’t allowed to" (a file-system refusal, not granting consent) in
  `errors.write.trashRefused.message.notPermitted` and `…folderNotWritable`; hu carries two exceptions. Propose a
  concept `notMatch` for both phrases. (hu, de: `…noPermissionErrno.explanation`, `…permissionDenied.explanation` too)
- `select` on Windows means "choose a command" (`select **Properties**`), which collides with the file-selection
  `select` concept in every locale that splits the two; hu added a `windows` form. Consider a `notMatch` for "select **"
  or a `choose` match in Windows branches. (hu)
- No concept covers "system monitor app", "disk tool", "network adapter", "Resource Monitor", or "Start" (the Windows
  menu), so each locale coins them unrecorded. (hu, de)
- English senses the Windows and Linux branches add to macOS-shaped concepts: `ai-tool` hits "**Tools > Check**" (a
  Properties tab), `desktop` hits the Windows desktop and "your desktop’s disk tool" (a Linux desktop environment),
  `tab` hits Resource Monitor's "CPU tab", and `scan` hits "scan it for problems". Propose `notMatch`es or concepts.
  (de, uk: `i18n:apply-terms --concepts` refuses the `notMatch` because it would stale other locales' exceptions (cs,
  nl, ru, sv), so uk carries exceptions too; a lead-side `--drop-stale-exceptions` pass would settle it)
- `errors.listing.notEnoughMemory.suggestion`: the note keeps "Ctrl+Shift+Esc" as-is, but a German keyboard reads
  Strg+Umschalt+Esc (French and others differ too). Say whether key names may localize in a Windows branch. (de)
- The disk-check keys (`errors.listing.diskReadProblem.suggestion` and three siblings) call "GNOME Disks" a product
  name, but GNOME localizes the app's name (de "Laufwerke"). Say whether the Linux branch keeps the English name. (de)
- A new ruling for `file-explorer` flags `fileExplorer.pane.fileExplorerAriaLabel` ("File explorer", Cmdr's own pane
  area) in every locale; add that key to the concept's `notMatch`. (de, uk)
- `app-closes` matched "Cmdr closed the connection" (the four `*negotiationInvalid` error keys), so any locale with a
  ruling drifted there; added `closed the connection` to its `notMatch`. (hu)
- English-sense mismatches in per-OS branches: `path` hits "your PATH" (the environment variable,
  `settings.fileOperations.adbBinaryPath.description`), `unplug` hits "Nothing gets disconnected"
  (`fileExplorer.network.browser.removeHostConfirm`), and `system-settings` hits the generic "your system settings"
  (`settings.appearance.themeColorSwatchAria`). hu carries an exception and an `accept`; propose `notMatch`es. (hu)
- `settings.appearance.appColorHintPrefix` has one `other` branch for Windows and Linux, but its link
  (`appColorHintLink`) is a Settings path on Windows and a noun phrase on Linux, so the prefix can't end in the colon a
  path wants. Give the prefix a `windows` branch, or make it one sentence with a link tag. (hu)
- "System disk" (Linux) and "system drive" (Windows) have no concept: they ride on `startup-disk`, and no Linux catalog
  in the pile names a system disk. Also unconfirmed Windows labels: "Settings > Personalization > Colors", "Settings >
  Apps", and "Windows display language". (hu)
- `fileExplorer.network.browser.refreshHint`: Windows' input-neutral "select here" has no natural rendering where the
  target is the hint text itself; say what "here" is ("select this hint") in the description. (hu)
- `file` matches the app name "File Explorer", so a locale whose Windows name has no file word (sv `Utforskaren`) drifts
  on every Windows branch naming it; sv carries six exceptions. Add "File Explorer" to `file`'s `notMatch`. (sv, uk: a
  `windows` form on `file` accepting «Провідник»)
- `software`'s sense covers "the hardware they run on", so a first ruling (sv `programvara`) drifts on "hardware-level"
  keys (`errors.listing.deviceProblem.explanation`, `…diskReadProblem.explanation`) and on
  `settings.network.smbConcurrency.description`; split hardware into its own concept. (sv)
- `accent`'s sense is a diacritic, yet it matches "accent color" in `settings.appearance.appColor.description`,
  `.themeColorSwatchAria`, and `.appColorHintPrefix`; add "accent color" to its `notMatch` or give the Windows accent
  color its own concept. Likewise `prompt` (AI text) catches "system prompt" (a dialog asking the user), which sv words
  as `fråga`. (sv, uk)

## Process feedback

Translators log here every script they wrote and every workaround they needed (the report's Process feedback section,
`translator-instructions.md`): one bullet per item, the tool or step that was missing, and the locale. The lead reads it
after each wave. Two locales logging the same kind of script is a missing tool: build it or fix the brief, then delete
the entries (`docs/guides/multi-agent-refactors.md` § Fan-out sweeps).

- The brief for 88 per-OS keys was 445 KB (about 110k tokens): too big for one read, and the Keys section alone needed
  three. Each key repeats its English three times (the value, the word diff, and the English branches inside "now") and
  two identical boilerplate bullets ("Per-OS value", "Frozen macOS text"). Print those two once per brief, drop the diff
  when the only change is the per-OS wrap, and show only the branches to translate plus the frozen one. (cs batch 1: 88
  keys, 133k tokens, read in five chunks even with `allow_large`; hu, de: read in nine chunks, no script; hu batch 3:
  122 keys, 385 KB, about 161k tokens, read in four Keys chunks plus one `grep -A3` of the Terms section, no script; es
  batch 2: 105 keys, 2,397 lines, about 145k tokens, read in seven chunks, no script)
- Wrote a one-off `grep -v` / `gawk` filter to strip those lines from the brief so the keys fit in context. A
  `--compact` brief would replace it. (hu)
- The worktree guard refuses any compound shell command it can't verify (a heredoc, a `grep | sort | uniq` chain over
  the catalog), so patch files went through the Write tool and a catalog lookup was retried as a plain `grep`. Say so in
  the instructions, and offer a catalog search (`pnpm i18n:grep --lang hu <regex>`) for "how did we ship this word".
  (hu)
- Per-OS drift surfaced only at the check: 34 findings, nearly all from five causes (Windows `Properties`, `select`,
  `Storage`, `Settings`, plus three English-sense mismatches). The brief could list, per ruled concept, "no `windows`
  form yet" for concepts that hit a Windows branch, so the translator patches `terms.json` before applying. (hu, de: 17
  findings from six causes)
- `pnpm i18n:apply-terms --help` exits with "Unknown option", and the brief doesn't show the `avoid` entry shape
  (`{ form, why }`), so the patch needed a look at a neighbor ruling first. Add `--help` with the field shapes. (de)
- A "no ruling" concept that only a Windows or Linux branch uses (`file-explorer`, `task-manager`, `user-folder`) never
  fails the check, so skipping its ruling goes unnoticed; the check could warn when a translated branch hits one. (de)
- `decisions.md` sat exactly at its byte budget, so one new entry needed a hand-edit of
  `apps/desktop/scripts/i18n-termbase-baseline.json`; `i18n:apply-terms` could take a `--bump-decisions` flag. (hu)
- The brief's "Translation memory" section (66 KB) went unread: for a per-OS batch the frozen `macos` branch is the
  memory. Skip it when every key is a per-OS restale. (hu)
- A 30-key per-OS brief (1,144 lines, about 69k tokens) is over the Read tool's 25k-token cap, so `limit: 1200` alone is
  refused and the read needs `allow_large: true`; say so in the lead's prompt, or trim the brief. (sv) A 105-key brief
  (2,383 lines, about 141k tokens) doesn't fit even with `allow_large`, so the terms section took an `awk | grep`
  filter. (fr) An 88-key brief (2,025 lines, about 134k tokens) took six paged reads even with `allow_large`. (nl) uk
  batch 1 (2,034 lines, 131k tokens): two paged reads of the Keys plus a `sed | grep` of the Terms section's rulings.
  (uk)
- A windows/other termbase form replaces the base ruling's `accept` list instead of inheriting it, so each OS form needs
  its own `accept` copied in; the first nl check flagged 42 keys for that alone. Inherit the base accepts. (nl)
- The worktree guard refused `apply-terms && apply && check-locale` chained with a shell variable ("names git in a form
  too complex"); split into three calls. (nl, uk: a heredoc plus two `apply-terms` calls, redone as Writes and separate
  calls)
