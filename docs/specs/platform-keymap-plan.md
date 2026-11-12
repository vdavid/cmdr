# Per-platform default keymap: implementation plan (Windows port task 26)

Issues: [#1102](https://github.com/getcmdr/cmdr/issues/1102) (the keymap),
[#1110](https://github.com/getcmdr/cmdr/issues/1110) (Quit and the phantom macOS reservations), and
[#1114](https://github.com/getcmdr/cmdr/issues/1114) (full screen). Plan entry: `docs/specs/windows-port-plan.md`
task 26. Written against `5135d6d238` (2026-11-12). Out of scope, already filed:
[#1115](https://github.com/getcmdr/cmdr/issues/1115) (WebView2's browser keys).

**Goal**: macOS, Windows, and Linux each have their own default keymap, as three first-class platforms. Windows and
Linux follow Total Commander first and their native file managers second; macOS keeps every key it has today and gains
`⌥←` / `⌥→` for back and forward. The off-macOS keyboard bugs found on the way are fixed in the same work.

## Loud rules (read before every milestone)

1. **macOS keys don't move.** Every command's macOS default list stays byte-identical, in the same order, except
   `nav.back`, `nav.forward`, `viewer.goBack`, and `viewer.goForward`, which gain `⌥←` / `⌥→` as their LAST entry. The
   keymap snapshot (milestone 1) is the proof; never edit the macOS snapshot to make a test pass. Outside the keymap,
   macOS behavior changes only where § "Invariants" item 18 lists it; a milestone that finds another stops and reports.
2. **The keys aren't yours to decide.** `docs/notes/windows-port/keymap-decisions.md` is the verified per-command table
   and wins over the #1102 comment, `keymap-proposal.json`, `keymap-survey.md`, and this plan's examples. Where it
   leaves a command out, the key stays what it is today. Where it looks wrong, stop and report; don't pick a key.
3. **Three platforms, none the leftover.** Nothing in the keymap, its tests, or the menu bar says "non-Mac", "other", or
   "else Linux". A value is written for `macos`, `windows`, and `linux`, or by a helper that names its rule.
4. **❌ No mechanical conversion at read time.** After milestone 2, no reader turns a Mac combo into another platform's.
   Each platform's defaults are stored in that platform's own canonical form (`formatKeyCombo`'s output).
5. **❌ Never translate.** New strings are English plus a good `@key` description. A change to an EXISTING string is an
   edit of its `en` value (a per-OS string is one `{platform, select, …}` block), with its `@key` description updated
   (`apps/desktop/src/lib/intl/DETAILS.md` § "Platform variants"). The one-shot `apply-platform-copy.ts` script is gone;
   `copy-final.json` and `copy-decisions.md` are history, not a gate.
6. **❌ Nothing hand-loosened.** No allowlist, snapshot, or baseline is edited to get green. A snapshot changes only
   when the milestone says which lines move, and you review the diff line by line.
7. **Ask the registry, never the event flags.** Every key fix here goes through `eventMatchesCommand` /
   `resolveKeyCombo` or a named per-platform table; `cmdr/no-raw-key-match` stays clean without new opt-outs.
8. **Windows can't run here.** Cmdr doesn't compile for Windows yet. Everything Windows-shaped is proven by unit tests
   under `withPlatform('windows')` and by `cmdr-menu`'s tests (the crate builds on every host). § "What can't be tested
   here" lists the rest; don't claim a Windows behavior verified.
9. **Foreground commands only**, and the gates in § "Gates" before every commit. One commit per milestone, more if the
   milestone says so. Don't push; the lead lands.
10. **Strip milestone tags** from code, comments, test names, and docs before each commit
    (`docs/guides/multi-agent-refactors.md`). This plan keeps its own.

## Inputs

- `docs/notes/windows-port/keymap-decisions.md`: the keys, per command and platform, with what each desktop reserves,
  the native text keys per OS, and the unverified list. This plan names keys only as examples; the file is the source.
  Its § "Needs David" items are settled: the viewer's search toggles take the proposed keys (milestone 7); the search
  dialog's scope skips `Main window` (milestone 7); the Multi-rename access-key question waits for task 63.
- **Linux menu mnemonics vs `Alt+<letter>` (settled by a spike in the Linux E2E Docker lane, real X key events,
  2026-11-12)**: `src-tauri/src/menu/mnemonics.rs` gives every GTK bar title and item an underlined letter from the
  translated labels. GTK takes `Alt+<letter>` for a bar TITLE mnemonic before WebKit sees it (English titles take F, E,
  S, V, G, T, H; other languages differ); item mnemonics are inert while the menu is closed; `preventDefault` can't
  help. So the search dialog's `Alt+F` (Filename mode), `Alt+S` (Size popover), and `Alt+V` (scope) open a menu on Linux
  TODAY, and the 11 Multi-rename keys would too. The fix, verified in the same lane: while any main-window dialog,
  overlay, or the palette is up, set the main window's `GtkWindow` mnemonic modifier
  (`gtk_window_set_mnemonic_modifier`) to an unreachable mask, and restore Alt when the last one closes, the way a
  native modal blocks its parent's menu bar. It lands in milestone 5 (its own commit). The viewer's bar has no
  mnemonics, so its `Alt+R` / `Alt+C` are safe. On Windows the same question stays with task 63.
- #1102's comment of 2026-11-12 (David's decisions), #1110, #1114.
- `docs/notes/windows-port/keymap-survey.md` and `keymap-proposal.json`: the survey. Stale in places (milestone 10
  corrects them); useful for the `reserved` block and the Total Commander mapping.
- Out of scope, the lead files it: the browser demo's menu BAR following the visitor's OS (it's always the macOS bar
  today). The demo's keymap does become per-platform here (milestone 6).
- Local only, on David's laptop: `_ignored/keymap-research/` in the main clone (`code-report.md`, `os-report.md`,
  `tc-report.md`, `handoff-from-1103.md`). Total Commander's own key list can't be redistributed, so it isn't in the
  repo: it's David's vault note `projects/Cmdr/research/Total Commander keyboard reference.md`.
- Docs to read once: `apps/desktop/src/lib/shortcuts/CLAUDE.md` + `DETAILS.md`, `lib/commands/` C+D, `lib/platform/`
  C+D, `crates/cmdr-menu/` C+D, `apps/desktop/src-tauri/src/menu/` C+D, `docs/testing.md`, `docs/design-principles.md` §
  "Platform-native, not generic".

## The code today (grep of `5135d6d238`; re-grep before you edit)

All paths under `apps/desktop/src/` unless they start with `crates/` or `src-tauri/`.

- **Defaults**: `lib/commands/sources/*.ts` (11 files), `shortcuts: string[]` in macOS form. Four module-load ternaries:
  `sources/main-window.ts:322,334,393,401` (`pane.focusControls`, `app.focusNotifications`, `pane.moveDividerLeft` /
  `Right`). `command-registry.ts:186` blanks the keys of a command whose `needsCapability` is off, also at load.
- **The converter**: `toPlatformShortcut`, `lib/shortcuts/key-capture.ts:378-424`, with the `⌃`→Shift hack inside.
  Callers: `shortcuts-store.ts:458,468` (`getEffectiveShortcuts`, `getDefaultShortcuts`, the only app readers of
  `command.shortcuts` besides `command-registry.ts:186`), `file-explorer/pane/FunctionKeyBar.svelte:236` (the EMPTY
  slot's `⇧F<n>` chip, which has no command behind it), `routes/viewer/ViewerStatusBar.svelte:62` (the viewer's search
  key, which isn't a registry command: "Search isn't a registry key"; `viewer-keyboard.ts` `handleModifierShortcut`
  opens search on `f` with either ⌘ or Ctrl, and `handleSearchToggleKey` takes `(⌘ or Ctrl)+Alt+R/C`), and two tests
  (`routes/viewer/viewer-keys.remap.test.ts`, `lib/ask-cmdr/ask-cmdr-trigger.test.ts:714`). Rust mirrors the hack:
  `crates/cmdr-menu/src/accelerators.rs:184`.
- **Readers of the sources as TEXT**: `test/e2e-playwright/marketing-clips-keys.test.ts` regex-parses `shortcuts: ['…'`
  out of `lib/commands/sources/*.ts` (it can't import the registry), and checks the clips' `COMMAND_CAPTIONS` against
  it. Tests reading `command.shortcuts` directly: `routes/(main)/global-keydown.test.ts:469`,
  `lib/demo/browser-shortcuts.test.ts:40,70`, `conflict-detector.test.ts`, `shortcut-dispatch.test.ts`,
  `command-registry.test.ts`, `command-reachability.test.ts`, `shortcut-vocabulary.test.ts`.
- **The browser demo**: `lib/demo/browser-shortcuts.ts` seeds customizations in macOS spelling (`⌃T`, `⌘F3`) for every
  visitor, so off macOS they never match a keypress (the matcher formats `Ctrl+T`). The demo's menu bar is always the
  macOS one (`crates/cmdr-listing-wasm/src/menu.rs:30`, `PLATFORM = Platform::MacOs`) while its keymap follows the
  visitor's OS (`lib/demo/platform.ts`).
- **A raw key table the registry doesn't see**: `file-explorer/navigation/keyboard-shortcuts.ts`
  (`handleNavigationShortcut`, used by both list views, `cursor-nav-keys.ts`, `PlacesBrowser`, `ServersHub`) hardcodes
  `⌥↑` / `⌥↓` / `Home` / `End` / `PageUp` / `PageDown` by `event.key` and modifier flags. Their registry commands are
  `nav.home`, `nav.end`, `nav.pageUp`, `nav.pageDown` (`sources/file-list.ts:53-80`; `nav.firstInFull` /
  `nav.lastInFull` are Full mode's `←` / `→`, a different pair).
- **Linux menu mnemonics**: `src-tauri/src/menu/mnemonics.rs` gives every GTK bar title and item an underlined letter,
  allocated per submenu from the translated labels (none on macOS). GTK takes a title's `Alt+<letter>` before the
  webview, as it takes the F-keys (which the bar already leaves out); § "Inputs" has the spike and the fix. The
  main-window dialog gate (`routes/(main)/menu-dialog-gate.svelte.ts` → `set_commands_refused_over_dialog`,
  `src-tauri/src/commands/menu_state.rs:319`) already knows when a dialog, an explorer overlay (inline rename included),
  or the palette is up. Linux menus are per window, but the main bar is installed with `app.set_menu`
  (`menu/install.rs:64`); which other windows (Settings, the queue, Debug) carry it on Linux is unverified.
- **The search dialog's other hardcoded keys**: besides the three milestone 7 makes commands, `query-shortcuts.ts`
  routes `⌥A/F/R` (modes), `⌥⏎` (primary action), and swallows `⌘⏎` / `⇧⏎` as no-ops; `FilterChips.svelte:188` routes
  `⌥S` / `⌥M` / `⌥I` (popovers) and the scope keys `⌥C` / `⌥V`. Their chips are literals: `ModeChips.svelte:47,54,67`,
  `ScopeFilterPopover.svelte:146,159`, `QueryBar.svelte:195,216-217` (`⌘H`, `⏎`), `search/SearchDialog.svelte:371,378`
  (`⌥⏎`, `⏎`), `EmptyState.svelte:91-93` (`⌘N`, `⌘H`, `⌘Enter`). Off macOS `Ctrl+Enter` isn't swallowed and falls
  through to bare Enter's action (`QueryDialog.svelte:554`). And `tipAi` says `⌘Enter` runs an AI search while the
  router swallows `⌘⏎` (a macOS copy bug today).
- **The matcher**: `key-capture.ts` `formatKeyCombo`, `physicalKeyCombo`, `latinKeyCombo`, `typedCharacterCombo`,
  `keyComboCandidates`; `shortcut-dispatch.ts` `resolveKeyCombo`, `eventMatchesCommand`, `isBoundByAnyCommand`.
- **Mac-only reads in handlers**: `routes/(main)/global-keydown.ts:60-75` (`isDebugWindowShortcut`,
  `shouldSuppressKey`), `:87,91` (the `⌘C` and `⌘←`/`⌘→` carve-outs); `file-explorer/pane/pane-key-router.ts:153`;
  `file-explorer/views/full-list-mouse.ts:86`, `BriefList.svelte:602` (click-to-rename), `BriefList.svelte:629-656` and
  `FullList.svelte:690-712` (`metaKey` into `SelectPayload`), `pane/pane-pointer.ts:125`;
  `routes/viewer/viewer-keyboard.ts` `verticalMotion`; `file-explorer/pane/type-to-jump-keys.ts:24,42,160`;
  `settings/sections/KeyboardShortcutsSection.controller.svelte.ts:371` (`Win`).
- **Search dialog**: `lib/query-ui/query-shortcuts.ts` (`matchKey`, `routeModeNumberShortcut`, `routeModifierShortcut`),
  literal chips in `query-ui/EmptyState.svelte:91-93`, `QueryBar.svelte:195`, `ModeChips.svelte:47,54,67`, and
  `queryUi.empty.tip` / `tipAi` in `intl/messages/en/queryUi.json`.
- **Multi-rename option keys**: `lib/multi-rename/option-keys.ts` (11 `fixedKey` commands in `sources/multi-rename.ts`).
- **Native commands**: `NATIVE_SHORTCUT_COMMAND_IDS` (`command-registry.ts:43`), the four entries in
  `sources/app.ts:71-106`, `DispatchExemptId` family 1 (`routes/(main)/command-handlers/types.ts:27-34`),
  `isNativeShortcutCommand` (`shortcuts-store.ts:442`), the banner (`settings/sections/keyboard-shortcuts-banner.ts`).
- **Quit and full screen**: the quit gate is `quit::request_quit` (`src-tauri/src/app_lifecycle.rs:70,153`). Closing the
  main window is the `WindowEvent::CloseRequested` branch at `app_lifecycle.rs:67-76`: ask the gate, hold the close if
  it's held, else `stop_background_services()` and `exit(0)`; the four native commands' `whileDialogOpen` is
  `NATIVE_MENU_BAR_ITEM` (`sources/app.ts:77-104`). `full_screen` is `true` only in
  `src-tauri/src/platform/macos.rs:28`; the saved state lives in `src-tauri/src/window_state/mod.rs`;
  `routes/(main)/escape-key.ts` leaves full screen with `getCurrentWindow().setFullscreen(false)`, on macOS too, so the
  webview's window API already drives the same native full screen AppKit's View menu item enters. The demo answers
  `fullScreen: true` for a Mac visitor (`lib/demo/platform.ts:47`) and `NO_CAPABILITIES` otherwise.
- **Rust bar**: `crates/cmdr-menu/src/spec.rs:37-72` (`Platform { MacOs, Linux }`, `PerPlatform { macos, linux }`,
  `both`, `cmd_or_ctrl!`, `macos`, `split`, `macos_only`, `linux_only`), `bar.rs` (61 accelerator sites), snapshots in
  `bar_test.rs` and `bar_states_test.rs`, muda parse tests in `src-tauri/src/menu/accelerators.rs`. `Platform` is also
  matched in `crates/cmdr-menu/src/context/popups.rs:158-167`, `context/file.rs`, `click.rs`, `accelerators.rs`.
- **Tests that pin a platform**: `lib/shortcuts/registry-conflicts.test.ts` (macOS, hoisted),
  `conflict-detector-registry.test.ts` (implicit Linux, subtracts `Ctrl+D`), `shortcut-vocabulary.test.ts` (macOS). Both
  conflict tests run with NO capability, so every capability-gated command's keys are blanked there today.
  `withEveryCapability()` (`lib/platform/test-platform.ts`) grants them all, which is wrong for a per-OS test (it puts
  macOS-only commands on Linux). The per-OS truth is the three Rust arms (`src-tauri/src/platform/macos.rs`,
  `windows.rs`, `linux/mod.rs`); a field that isn't a literal `false` there (`true`, `cfg!(…)`, `x11_session()`) CAN be
  true on that OS. Also: `lib/commands/command-reachability.test.ts`, `command-registry.test.ts`,
  `rust-command-id-drift.test.ts`.
- **Vitest's default platform is `'linux'`** (`lib/platform/index.ts:20`), so many unrelated tests assume today's Linux
  keys without saying so.
- **Copy rows waiting on this work**: `fileExplorer.clipboard.empty`, `viewer.copyDialog.confirmBody` / `refuseBody`,
  `queryUi.empty.tip` / `tipAi`, `settings.fileOperations.pasteClipboardAsFile.description`,
  `settings.fileExplorer.tabs.closedTabHistorySize.description`,
  `settings.fileExplorer.suppressQuickLookHint.description`,
  `settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.description`, and
  `shortcuts.conflict.reservedByMacOs`, `shortcuts.section.macOsTooltip`, `shortcuts.section.macOsBadge`. Check the list
  against `docs/notes/windows-port/copy-decisions.md` and `copy-final.json`, which own it.
- **Linux isn't shipped** (`README.md:58`), so an off-macOS `shortcuts.json` exists only on dev and test machines. No
  migration of saved overrides is planned. If a Linux build ships before this lands, stop and tell the lead.

## The mechanism

**A command's `shortcuts` is three lists, one per platform, each in that platform's own canonical form.**

```ts
type PlatformShortcuts = Readonly<Record<Platform, readonly string[]>> // macos | windows | linux

// The literal: each arm in its own platform's spelling.
shortcuts: {
  macos: ['Backspace', '⌘↑'],
  windows: ['Backspace', 'Alt+↑', 'Ctrl+PageUp'],
  linux: ['Backspace', 'Alt+↑', 'Ctrl+PageUp'],
}
// The helpers (in `lib/commands/platform-shortcuts.ts`), each naming the rule it applies:
shortcuts: sameOnAll('F5', '⇧F6') // the same physical chord everywhere; refuses ⌘ and ⌃
shortcuts: cmdOrCtrl('⌘⇧P') // ⌘ on macOS, the same chord on Ctrl elsewhere; needs ⌘, refuses ⌃
shortcuts: macOnly('⌘H') // a key on macOS, none elsewhere
// …and one that composes them, arm by arm, in argument order:
shortcuts: concatShortcuts(cmdOrCtrl('⌘F'), sameOnAll('⌥F7'))
```

- The helpers are constructors: they return all three arms at module load, so the registry data is always the full
  three-way table and no reader converts anything. They take macOS glyph spelling (so the macOS string is the literal in
  the source) and write the other arms in canonical `Ctrl+Alt+Shift+Super` order.
- `cmdOrCtrl` refusing `⌃` and `sameOnAll` refusing `⌘` / `⌃` makes the old converter's two failures unrepresentable:
  `⌘D` and `⌃D` can no longer land on one key by accident, and the `⌃`→Shift hack has nowhere to live. A command that
  needs either writes the literal. Refuse at the type level where a template-literal type can, and by throwing at load
  otherwise (a registry load that throws fails every test).
- The store resolves through one function, `defaultShortcuts(commandId, platform = getPlatform())`, over a map built
  once per platform (lazily, cached). It also owns the "no default key where the capability is off" rule that
  `resolveCommand` applies at load today, so sources and the registry stop reading the platform and capabilities at
  module load. The cache holds only the platform's table; the capability check runs on every call, outside it, or a test
  that switches capabilities reads a stale answer. `Command.shortcuts` changes type, so every stale TypeScript reader
  fails to compile; the text reader (`marketing-clips-keys.test.ts`) doesn't, and needs its parser changed.
- Lists that mix rules (`['Backspace', '⌘↑']`: a bare key plus a ⌘ chord) fit no single helper, so
  `concatShortcuts(...parts)` joins `PlatformShortcuts` values arm by arm, in argument order, and throws on a combo
  repeated within an arm. On `5135d6d238` that covers nine entries (`search.open`, `askCmdr.findNext` / `findPrevious`,
  `nav.open`, `nav.parent`, `file.delete`, `file.deletePermanently`, `file.openTerminalHere`, `share.back`). Only
  entries holding `⌃` or keys that truly differ per platform are literals: today `⌃D`, `⌃S`, `⌃M`, `⌃Enter`, `tab.next`
  / `tab.prev` (`⌃Tab`), the four ternaries, and the two `⌘⌃⇧` tab moves. An empty list needs a spelling too
  (`sameOnAll()`).
- The same three words exist on the Rust side already (`both`, `cmd_or_ctrl!`, `split`, `macos`), so the menu bar and
  the registry read alike, and the parity test (milestone 3) compares like with like.

**Why this and not the alternatives** (`code-report.md` § 9):

- **Mac list plus optional `windows` / `linux` overrides, missing arm = converted** (the report's pick). Smallest diff,
  but it keeps macOS as the base and the other two as derived, and keeps the silent rule that a new `⌘X` becomes
  `Ctrl+X` with nobody deciding. That's the "mechanical translation" the design principle rules out. Here the same rule
  survives only as `cmdOrCtrl`, a word the author types on purpose.
- **Three full keymap files keyed by command id.** Each platform would be one standalone table, easy to diff against
  Total Commander. But it splits a command's keys from the command (against "organized by feature"), makes every new
  command a four-file change, and puts two near-identical 228-row tables (Windows, Linux) side by side to drift. The
  per-platform snapshot from milestone 1 gives the same one-table-per-platform view, generated.
- **Rust owns the defaults, the frontend reads them.** One source for the menu and the registry, but it moves registry
  data across IPC and away from its commands, and the capability-restricted windows would need a new read path. The
  parity test gives the same guarantee without it.
- **Generate the bar's accelerators from the registry.** Removes the second copy, but `cmdr-menu` builds standalone (the
  wasm demo consumes it), the bar deliberately leaves some keys out per platform (GTK takes the F-keys before the
  webview), and its snapshot is a reviewed artifact. A parity test with a short, reasoned exception list keeps both.

## Gates

Per `AGENTS.md` § "Checker script": `pnpm check --fast && pnpm check svelte-tests-related` before each commit; plus
`pnpm check clippy` and the touched Rust tests (`cargo nextest run <name>`) when Rust changed. Extra lanes, named (each
is slow; run it once, at the milestone's end, foreground):

- `pnpm check desktop-e2e-linux` on milestones 2 (every keypress's lookup is rewired), 5, 6, 7, and 8 (they change what
  a Linux key does). Not on 3, 4, or 9: their surfaces are unit- and snapshot-tested, and the lane's US layout can't
  press AltGr.
- `pnpm check listing-wasm` on milestone 3 (`cmdr-listing-wasm` matches `Platform`).
- `pnpm check clippy-linux` on milestones 3, 5, and 8: host clippy doesn't compile the `cfg(target_os = "linux")` code
  they touch (Linux-only `Platform` matches, the GTK mnemonic mask, the Linux capability arm). Nothing compiles
  `windows.rs`; review it by eye and say so in the report.
- `cargo deny check` on milestone 5 (the direct `gtk` dependency).

Flake policy: `docs/guides/multi-agent-refactors.md` § "The loop" step 4. ❌ Never a full `pnpm check`.

**Test fixtures, used from milestone 1 on.** Per-OS tests run with a per-OS capability fixture, never "every capability
on every OS": for each platform, exactly the capabilities that CAN be true there (§ "The code today", the Rust arms).
Two kinds of test, two inputs:

- **What a user on that OS gets** (the keymap snapshot, conflicts, reachability, the parity TSV): the per-OS fixture. A
  command whose `needsCapability` the OS can't offer isn't there.
- **What the data says** (vocabulary, canonical order, the invariants test's no-`Ctrl+Alt+<letter>` and no-`Super`
  rules): every command's raw arm for that platform, capability ignored. Otherwise a gated command's arm
  (`file.openTerminalHere` holds `Ctrl+Alt+T` off macOS today) escapes every check until its capability flips.

## Milestones

Sequential, one agent each, in this worktree. Each ends green and committed.

### Milestone 1: pin today's keymap

- **Scope**: one new test, `lib/commands/default-keymap.test.ts`, and its three snapshot files; the per-OS capability
  fixture in `lib/platform/test-platform.ts` and a test pinning it to the Rust arms; `scripts/check/checks/inputs.go`
  and `.github/workflows/ci.yml` (the cache inputs below).
- **Intentions**:
  - **The fixture.** `withCapabilitiesPossibleOn(platform)` (or a better name; same forms as `withCapabilities`) grants
    exactly the capabilities that can be true on that OS. A pin test reads the three Rust arm files as text (the way
    `rust-command-id-drift.test.ts` reads `ids.rs`), maps each `snake_case` field to its `Capability`, and fails if the
    fixture disagrees with "not a literal `false`", or if a Rust field has no `Capability` or the other way round.
    Milestone 8's new capability and the `full_screen` flip then move the fixture by failing this test.
  - **The snapshot.** Make "macOS didn't move" and "exactly these Linux keys moved" provable. The test renders each
    platform's keymap under that platform's fixture as stable lines (`<command id>\t<combo>, <combo>`, registry order, a
    command with no key included as an empty list, a command the OS can't offer left out) and compares with
    `toMatchFileSnapshot` against `lib/commands/__snapshots__/default-keymap.{macos,windows,linux}.txt`. It reads
    through the public `getDefaultShortcuts`, so milestone 2 can swap the internals under it. Set the fixture before the
    dynamic import: the registry blanks gated keys at load today.
  - **Cache inputs.** The pin test reads `apps/desktop/src-tauri/src/platform/{macos.rs,windows.rs,linux/mod.rs}`, which
    `svelteInputs` doesn't cover, so an arm edit would cache-hit green. Add the three paths there and to the matching
    `svelte` filter in `ci.yml` (precedent: the `*_token_vectors.json` line).
- **Landmines**: sources read the platform at load today, so each platform needs its own module load
  (`vi.resetModules()` plus a dynamic import after `withPlatform`, or three hoisted files; the forms are in
  `lib/platform/test-platform.ts`). Windows equals Linux today; pin it anyway.
- **Test plan**: the snapshots are the deliverable. Prove both tests bite: change one default locally, see the snapshot
  fail; flip one Rust arm field locally, see the pin test fail; revert both.
- **Gates**: § "Gates", plus `pnpm check go-tests` (`inputs.go` changed).
- **DONE**: three snapshot files, the fixture and its pin test, and the input lines committed; no source or other test
  changed.

### Milestone 2: the three-way table, with no key changed

- **Scope**: `lib/commands/types.ts`, the new `lib/commands/platform-shortcuts.ts` and its test, all 11 `sources/*.ts`,
  `command-registry.ts`, `shortcuts-store.ts`, `key-capture.ts` (delete `toPlatformShortcut`), its two other callers,
  the keymap's own tests, every test listed under § "The code today" that reads `command.shortcuts` or calls
  `toPlatformShortcut`, and `test/e2e-playwright/marketing-clips-keys.test.ts`'s source parser (read the macOS arm; that
  suite can't import the registry).
- **Intentions**: land § "The mechanism" as a pure refactor. Convert every entry to `sameOnAll`, `cmdOrCtrl`, `macOnly`,
  a `concatShortcuts` of them, or the literal, choosing whichever reproduces TODAY's key on all three platforms. The
  four ternaries and the two tab-move commands (the hack's only users) become literals holding today's values.
  `FunctionKeyBar` and `ViewerStatusBar` stop converting. Neither has a command behind it: the F-key bar's EMPTY slot
  shows a bare `⇧F<n>` (the filled slots already read `getFirstShiftShortcutReactive`), and the viewer's search key is
  its own fixed `⌘F` / `Ctrl+F` (`viewer-keyboard.ts` accepts either modifier). Each writes the chord in the platform's
  spelling with the same token writer `formatKeyCombo` uses, not by converting a Mac string. (The viewer's key becomes a
  registry command in milestone 7; this is the interim.)
- **The three tests go per platform**, each as `describe.each(['macos', 'windows', 'linux'])` with no hoisting, on the
  inputs § "Gates" › "Test fixtures" names:
  - Conflicts: fold `conflict-detector-registry.test.ts` into `registry-conflicts.test.ts`. `allowedConflicts` is per
    platform. The two real `Ctrl+D` clashes on Windows and Linux (Duplicate vs Show favorites, the error screen's
    details toggle vs Show favorites) go in with the reason "moves in the keymap milestone"; the stale-entry test then
    forces milestone 6 to delete them. `PLATFORM_COLLAPSED_COMBOS` is gone.
  - Vocabulary: `shortcut-vocabulary.test.ts` rebuilds a keydown from every command's raw arm per platform (it needs the
    `Ctrl+Alt+Shift+Super` token parser) and demands `formatKeyCombo` reproduce it.
  - Reachability: `command-reachability.test.ts` per platform, on the per-OS fixture.
  - Conflicts and reachability on the per-OS fixture; vocabulary on the raw arms.
- **Landmines**:
  - Today's conflict tests grant NO capability, so the per-OS fixture may surface conflicts nobody has seen (macOS gains
    every gated command). Report each; an allowlist entry needs the lead.
  - The old converter wrote modifiers in the AUTHORED order, so `app.hideOthers` is `Alt+Ctrl+H` off macOS today. The
    helpers write canonical order, so that one snapshot line changes to `Ctrl+Alt+H`. It's the only allowed snapshot
    diff in this milestone; any other means a conversion slipped.
  - `getEffectiveShortcuts` returns a fresh array; keep that (callers mutate).
  - `isBoundByAnyCommand` calls `getEffectiveShortcuts` for every command per multi-candidate keypress, and each call
    does `commands.find`. With the per-platform map this becomes a map read; don't leave the `find`.
  - The registry's `needsCapability` blanking moves into `defaultShortcuts`, read at call time. `listedCommands()`
    already reads at call time.
  - The browser demo injects its own platform (`lib/demo/platform.ts`, which answers `windows` for a Windows visitor).
    Nothing to change here; Windows visitors get the Windows keymap from milestone 6 on, which also gives the demo's
    seeded map a per-platform shape.
  - The viewer and other restricted windows import the registry too; nothing here may add a store read at load.
- **Test plan**: milestone 1's snapshots are unchanged except the one line above. `platform-shortcuts.test.ts` covers
  each helper, the refusals, and canonical order. `key-capture.test.ts` loses the `toPlatformShortcut` cases.
- **DONE**: no `isMacOS()` or `getPlatform()` in `lib/commands/sources/`; `toPlatformShortcut` and the hack gone;
  `rg -n "toPlatformShortcut|PLATFORM_COLLAPSED" apps/desktop/src` is empty; conflict, vocabulary, and reachability run
  for three platforms.

### Milestone 3: the Rust bar knows three platforms, and a test ties it to the registry

- **Scope**: `crates/cmdr-menu/` (`spec.rs`, `bar.rs`, `accelerators.rs`, `context/`, `click.rs`, the tests),
  `apps/desktop/src-tauri/src/menu/` (`accelerators.rs` tests, any `Platform` match), `crates/cmdr-listing-wasm`'s
  `Platform` uses, the parity TSV writer in `default-keymap.test.ts`, `scripts/check/checks/inputs.go` and `ci.yml` (the
  cache lines below), both crates' docs.
- **Intentions**:
  - `Platform` gains `Windows`; `Platform::current()` answers it on Windows. `PerPlatform` gains a `windows` arm, and
    every helper takes or derives three values on purpose (`split(macos, windows, linux)`; `cmd_or_ctrl!` gives Ctrl on
    both; `linux_only` rows become rows that name the platforms they're on). The Windows bar has the Linux bar's rows
    until task 63 decides the Windows menu model; say so in `crates/cmdr-menu/DETAILS.md`.
  - Accelerators stay as they are in this milestone (Windows = Linux = today's).
  - Retire the Rust copy of the hack (`accelerators.rs:184`): off macOS the frontend only ever sends that platform's own
    spelling.
  - **The parity test.** The frontend test from milestone 1 also writes `crates/cmdr-menu/src/registry_default_keys.tsv`
    (`platform\tcommand id\tfirst default combo`, one line per command in `menuCommands`, under each platform's
    capability fixture, by `toMatchFileSnapshot`, so a stale file fails the frontend test). A `cmdr-menu` test reads it
    with `include_str!` and, for every bar row with a command (`ids.rs`'s menu id → command map) on each platform,
    requires the row's accelerator to equal `to_accelerator(combo, platform)`: the same conversion a rebind takes.
    Compare as modifier set plus key, since `Alt+Cmd+O` and `Cmd+Alt+O` are one chord. A row that differs on purpose
    goes in a list in the test with its reason (the keys GTK takes before the webview, which the Linux bar leaves out; a
    display-only accelerator), and a stale reason fails.
- **Landmines**:
  - The first run will surface real drift between the registry and the bar. Fix the bar where it's a plain mistake;
    where it's deliberate, write the reason. Report each one.
  - No `cfg` in the data (`crates/cmdr-menu/CLAUDE.md`): the Windows bar must exist on every host so a Mac pins it.
  - Every accelerator must parse in muda; the three muda tests in `src-tauri/src/menu/accelerators.rs` iterate all three
    platforms after this.
  - `ids.rs` keeps its `pub const NAME: &str = "…";` shape (`rust-command-id-drift.test.ts` parses it).
  - `bar_test.rs` gets a third snapshot, `WINDOWS_MENU_BAR`; `bar_states_test.rs` likewise.
  - A TSV needs no new dependency in the crate. Don't add one.
  - Check-runner caching, both sides: the TSV sits under `crates/cmdr-menu/**`, so the Rust lanes already hash it. The
    Svelte lane doesn't: `svelteInputs` (`scripts/check/checks/inputs.go`) covers `apps/desktop/src/**` and friends
    only, so a hand-edited TSV would cache-hit green. Add the TSV there and to the matching `svelte` filter in
    `.github/workflows/ci.yml`. Close the same gap for the two Rust files vitest already reads
    (`rust-command-id-drift.test.ts`): `crates/cmdr-menu/src/ids.rs` and `apps/desktop/src-tauri/src/dock/menu/rows.rs`
    are in `rustEmbeddedInputs` but not in `svelteInputs`, so an `ids.rs` edit never re-runs the drift test. Then
    `pnpm check go-tests`.
  - The parity test skips `predefined` rows (macOS's Hide, Quit, Services: AppKit owns their keys).
  - `src-tauri/src/menu/display_accelerators.rs` matches `Platform::MacOs` 11 times; read it with the rest.
  - The browser demo's bar stays macOS (`cmdr-listing-wasm/src/menu.rs:30`) whatever the visitor's OS (making it follow
    the OS is out of scope); with the hack gone, check what `update_menu_accelerator` does there with a `Ctrl+…` combo
    from a Windows visitor's keymap, and pin the answer in a test.
- **Test plan**: the parity test red first (break one accelerator, see it name the row), the three bar snapshots, the
  muda tests on three platforms, `rust-command-id-drift.test.ts` still green.
- **DONE**: every `Platform::Linux` match under `crates` and `apps/desktop/src-tauri/src` sits beside a
  `Platform::Windows` arm or names why Linux alone (`rg -n "Platform::Linux"`, read each); the parity test runs for
  three platforms with a reasoned exception list; the TSV, `ids.rs`, and `rows.rs` are in `svelteInputs` and the
  `svelte` CI filter.

### Milestone 4: the matcher

- **Scope**: `lib/shortcuts/key-capture.ts`, `shortcut-dispatch.ts` if needed,
  `file-explorer/pane/type-to-jump-keys.ts`, `KeyboardShortcutsSection.controller.svelte.ts`, their tests.
- **Intentions**:
  - **AltGr guard.** Off macOS, `event.getModifierState?.('AltGraph') === true` means the keypress is typing. Then
    `physicalKeyCombo` and `latinKeyCombo` return `null`, and `typedCharacterCombo` treats AltGr as it treats ⌥ on a Mac
    (it may run although `ctrlKey` is set), so an AltGr-typed `*`, `+`, or `-` reaches its binding. The exact combo
    stays first in `keyComboCandidates`.
  - **Type-to-jump takes AltGr letters**: a keypress with `AltGraph` set and a printable `event.key` is typing, even
    though Windows reports Ctrl+Alt with it.
  - **`Ctrl+<` by position.** Go to root's `Ctrl+<` (milestone 6) is the `IntlBackslash` key, which types `<` on German
    and Nordic layouts and `í` on Hungarian. Give `IntlBackslash` a name in the physical-key table (`<`) and let the
    physical candidate run for it with Ctrl or ⌘ alone, so the binding works wherever that key exists. A US keyboard has
    no such key; its `Ctrl+Shift+,` stays its own combo.
  - **`Super`, not `Win`**, in the Settings live preview (`formatModifiers`), so the preview and the stored combo agree.
    What a Windows user READS for that key is a display question: `toDisplayShortcut` may show `Win` on Windows if
    `keymap-decisions.md` or the style guide's platform vocabulary says so; the stored token stays `Super`.
- **Landmines**:
  - Linux AltGr is `ISO_Level3_Shift` with `ctrlKey` and `altKey` false. WebKitGTK may still report `AltGraph`; then the
    guard nulls two candidates that wouldn't have fired anyway (`physicalKeyCombo` needs `altKey`, and a bare
    `latinKeyCombo` letter is typing), so it's harmless there.
  - The guard is off on macOS by construction, not by luck: whether WebKit on macOS reports `AltGraph` for ⌥ is
    unverified, and if it does, a platform-blind guard would kill `⌘⌥R`, `⌥⇧=`, and every other ⌥ physical match.
    Synthetic test events can't catch that (they carry no `getModifierState` unless the test adds one), so the macOS
    cases must feed `AltGraph: true` and prove nothing changes. Keep the existing ⌥ behavior on macOS byte-identical
    (`key-capture.test.ts` pins it).
  - AltGr plus a dead key: `normalizeKeyName` turns `Dead` into the key's position, so the EXACT combo (first candidate,
    untouched by the guard) becomes `Ctrl+Alt+<that key>`. Decide in the test whether an `AltGraph` dead key resolves to
    nothing; the invariant below covers letters only.
  - `IntlBackslash` on macOS: Mac ISO keyboards are widely reported to swap `IntlBackslash` and `Backquote` in
    `event.code` (unverified here). Keep the new physical rule off macOS, where no default needs it.
  - Whether WebView2 sets `AltGraph` for LeftCtrl+LeftAlt on an AltGr layout is unverified. That's why milestone 6 also
    removes every `Ctrl+Alt+<letter>` default: the guard protects typing, the keymap avoids the question.
  - `capturedKeyCombo` then records `Ctrl+Alt+@` for AltGr+Q. Honest; leave it.
  - `physicalKeyCombo` is deliberately narrow (read its doc comment). Widen it only for `IntlBackslash`.
- **Test plan**: `key-capture.test.ts` cases feeding `getModifierState` (German AltGr+Q, Hungarian AltGr+F and AltGr+V,
  Polish AltGr+A, a US layout unchanged), the `IntlBackslash` cases on three layouts, type-to-jump with an AltGr letter,
  the "keys from any keyboard and layout" block in `routes/(main)/global-keydown.test.ts`.
- **DONE**: the AltGr cases in `key-capture.test.ts` were red before the guard and are green after (say so in the
  report); on macOS every case fed `AltGraph: true` yields the same candidates as without it; the `IntlBackslash` cases
  pass on three layouts.

### Milestone 5: the off-macOS input bugs

- **Scope**: `routes/(main)/global-keydown.ts`, a new per-platform table beside it, the file list's row and mouse
  handlers, `pane-key-router.ts`, `file-explorer/navigation/keyboard-shortcuts.ts`, `routes/viewer/viewer-keyboard.ts`,
  their tests. First commit, on its own: the Linux mnemonic mask (`menu-dialog-gate.svelte.ts`, `commands/menu_state.rs`
  or `src-tauri/src/menu/`, `src-tauri/Cargo.toml`, the Linux E2E lane's image and one spec, the menu module's C+D).
- **The mnemonic mask (first commit)**:
  - What § "Inputs" describes. A Linux-only direct `gtk = "0.18"` dependency (0.18.2 is in the lock already; follow
    `docs/guides/adding-a-crate.md`, `cargo deny check`). It lives in the menu module beside the other GTK menu code,
    fed by the dialog gate; ❌ not a second frontend writer.
  - Drive it from the gate's own "something is open" bit, sent explicitly (a `bool` on the same command, or its own
    command), ❌ not from "the refused list is non-empty": that's the same thing today only because
    `REFUSED_OVER_DIALOGS` happens to be non-empty.
  - Stacked dialogs: the mask stays until the LAST dialog, overlay, or the palette closes (the gate's bit already works
    that way; pin it). A webview reload or HMR restarts the gate with `lastSent = null`, so its first push restores the
    real state; pin that too.
  - Inline rename and the palette count as dialogs here, so `Alt+F` doesn't open the File menu mid-rename; intended
    (it's modal input), say so in the menu module's `DETAILS.md`.
  - **Other windows**: find out (from the code, then in the lane) which windows carry the main bar on Linux. If the
    Settings window does, its shortcut recorder can't capture `Alt+F`: mask that window while the recorder listens, or
    give it no bar; report which. The viewer's own bar has no mnemonics (spike).
  - **The proof**: a real-X-event spec in the Linux lane (the lane's `pressKey` is a synthetic DOM event GTK never
    sees). Make the helper part of the lane: prefer the distro's packaged `xdotool` (it wraps the same `libxdo` the
    spike's helper used) over a compiled helper, and keep it to the one spec. Why not a unit test plus a manual check:
    the failure is silent (a menu opens and the keypress is gone), it depends on GTK's behavior and on every dialog
    registering with the gate, and a manual check nobody repeats won't catch a regression. The spec, in English: with
    the search dialog open, a real `Alt+F` switches to Filename mode and no menu opens; after it closes, a real `Alt+F`
    opens the File menu (Escape closes it). Milestone 6 adds the Multi-rename keys to it.
  - Language: the mask doesn't read labels, so one language proves it. Keep the decision (bit → mask or Alt) a pure
    function with a unit test; only the GTK call needs the lane.
- **Intentions**:
  - **Native text keys as one per-platform table.** The `⌘C` and `⌘←` / `⌘→` carve-outs become
    `nativeTextKeys(platform)`: exactly the sets in `keymap-decisions.md` § "Native text keys", for all three OSes,
    including the combos no default holds until milestone 6 (the table is keyed on combos, so it lands complete now),
    the copy-with-a-selection condition, and the "worth a bail" entry, kept apart from the native ones with its reason.
    Keyed on the KEY, not on the command it happens to run today: what collides with typing is the combo, whatever a
    user has bound to it.
  - **One toggle-modifier predicate** for rows: ⌘ on macOS, Ctrl on Windows and Linux. It feeds `SelectPayload` (rename
    the field from `metaKey` to what it means, `toggle`) at the eight row sites and the two click-to-rename guards. ❌
    Not `metaKey || ctrlKey`: Ctrl+click is the context menu on macOS.
  - **`shouldSuppressKey`** suppresses the platform's own twins (select all outside the file list, the dev tools chord
    in production), and `isDebugWindowShortcut` reads the platform's primary modifier.
  - **`pane-key-router.ts:153`**: the "nothing under the cursor" swallow asks whether the event is `nav.open`'s combo
    with a modifier, through the registry, in place of `e.metaKey`.
  - **Viewer document start and end**: `Ctrl+Home` / `Ctrl+End` (with Shift to extend) on Windows and Linux; ⌘+arrow
    stays on macOS. The viewer's `handleModifierShortcut` (copy, select all; search until milestone 7) takes the
    platform's primary modifier only, through the same predicate as the rows: today either ⌘ or Ctrl runs it, so
    `Super+C` copies on Linux and `⌃C` on macOS.
  - **`handleNavigationShortcut`** (`file-explorer/navigation/keyboard-shortcuts.ts`) stops hardcoding `⌥↑` / `⌥↓` /
    `Home` / `End` / `PageUp` / `PageDown` and asks `comboMatchesCommand` for `nav.home`, `nav.end`, `nav.pageUp`, and
    `nav.pageDown`. Without this, milestone 6's `Alt+↑` for Go to parent on Windows and Linux would ALSO jump the cursor
    to the top in the lists, the places browser, and the servers hub. The places browser and servers hub then follow a
    user's rebind of those commands too; that's intended (one meaning per key), say so in the doc.
  - **The rest of `metaKey`.** `rg -n "metaKey" apps/desktop/src -g '!*.test.ts'` has about 95 hits. Most are "no
    modifier held" guards or macOS-only code, which are fine. Classify every hit; fix the ones in this milestone's scope
    that read `metaKey` (alone, or `metaKey || ctrlKey` as "either") to mean the primary modifier; list the others
    outside scope in the report, by file and line, for the lead to file.
- **Landmines**:
  - This lands BEFORE the keymap milestone because macOS's new `⌥←` / `⌥→` would otherwise run back and forward from
    inside every text input (`isTypingKeyCombo` lets ⌥ combos through).
  - `InlineRenameEditor.svelte:103-109` answers `⌥←` / `⌥→` itself; check it still gets them first.
  - The row payload crosses `FilePane.svelte:2280`, `pane-commands.ts:290`, and `DualPaneExplorer.svelte:1117`.
  - Quick Look's panel-forwarded keys carry `metaKey` as data (`quick-look-state.svelte.ts`); that's macOS-only code,
    leave it.
  - **Two macOS changes land here, both on § "Invariants" item 18's list**: the viewer stops answering `⌃C`, `⌃A`, and
    `⌃F` (primary modifier only), and a macOS user's saved rebind of `nav.home` / `nav.end` / `nav.pageUp` /
    `nav.pageDown` now also moves those keys in the places browser and the servers hub (and a removed key stops working
    there). Pin both in tests; nothing else on macOS may move.
  - The native text table must never claim a combo a text field doesn't own on that OS: on macOS, bare `Home` / `End`
    scroll and don't move the caret, so they're not in the macOS set; a test per platform walks the table against the
    decisions file, set for set.
  - Off macOS, `Ctrl+←/→` (copy path between panes) loses to word motion in EVERY main-window text field (path bar,
    quick filter, rename, the Ask Cmdr composer): the table is checked before dispatch, whatever focus scope is active.
    Test from each of those fields, plus a Settings text input and the viewer's search input.
- **Test plan**: `global-keydown.test.ts` per platform (selected text plus the copy combo copies text; word motion in a
  text input stays with the input; outside a text input the commands still run), row-click tests under
  `withPlatform('windows')` and `'linux'` (Ctrl+click toggles, never renames; on macOS ⌘+click toggles and Ctrl+click
  doesn't), viewer keyboard tests, the pane-key-router case, `handleNavigationShortcut` per platform (red first: a test
  that rebinds `nav.home` away from `Alt+↑` sees `Alt+↑` still jump to the top today).
- **DONE**: the mask commit is separate and the real-X-event spec passes in `pnpm check desktop-e2e-linux` (red first:
  it fails on the commit before, with the File menu opening); the report says which windows carry the main bar on Linux
  and what the Settings recorder does; `rg -n "'⌘C'|'⌘←'|'⌘→'" "apps/desktop/src/routes/(main)/global-keydown.ts"` is
  empty; no row handler reads `event.metaKey` directly; `keyboard-shortcuts.ts` compares no `event.key` against a
  navigation key; the report carries the classified `metaKey` list.

### Milestone 6: the Windows and Linux keymaps (and `⌥←` / `⌥→` on macOS)

- **Starts only after** milestone 5's mnemonic mask has landed (the Multi-rename `Alt+<letter>` keys need it).
- **Scope**: `lib/commands/sources/*.ts` (data), `crates/cmdr-menu/src/bar.rs` and its snapshots, `lib/multi-rename/`
  docs and tests, a new invariants test, the unit tests and E2E specs that assumed the old keys; second commit:
  `lib/demo/browser-shortcuts.ts` and its test. That order, because the demo's Windows and Linux arms derive from the
  registry's new defaults; between the two commits the demo's off-macOS map is still the dead macOS-spelled one, which
  matches nothing, so nothing a visitor has regresses.
- **Intentions**: transcribe `keymap-decisions.md` § "Changed commands", § "macOS additions", and the off-macOS arm of
  every command in § "Commands on one OS only" (Open terminal here included, though no OS offers it yet) into the
  registry, command by command, and move the bar with it (the parity test forces every menu row). Every command the file
  doesn't list keeps today's keys. #1102's 2026-11-12 comment is the cross-check: if a decision there has no row in the
  file, stop and report.
- **New invariants test** (`lib/shortcuts/keymap-invariants.test.ts`), per platform:
  - No default on Windows or Linux is `Ctrl+Alt+<letter>` (with or without Shift), over every command's raw arm.
  - No Windows or Linux default holds `Super`, over the raw arms.
  - Every default order is canonical (already the vocabulary test; keep it there).
  - Each command whose first Linux key a desktop takes (per the decisions file) has another default that no listed
    desktop takes. The reserved table arrives in milestone 9; here the test names the commands by hand, and milestone 9
    rewires it onto the table.
- **Landmines**:
  - Delete both `Ctrl+D` lines from `allowedConflicts`; the stale-entry test fails until you do.
  - **`Alt+<letter>` and the Linux bar**: extend milestone 5's real-X-event spec with every Multi-rename `Alt+<letter>`
    while the sheet is up, focus in its text fields AND on its option controls, in English (the bar titles take F, E, S,
    V, G, T, H): the option toggles, no menu opens, and the field's text doesn't change. If one opens a menu, the sheet
    isn't registering with the dialog gate: fix that, don't move the key. On Windows the same question belongs to
    task 63.
  - **A macOS user's saved shortcuts**: storage is delta-only and a customized command stores its whole list, so a user
    who customized `nav.back` doesn't gain `⌥←` (intended; say so in `lib/shortcuts/DETAILS.md`). A user who bound `⌥←`
    or `⌥→` to ANOTHER command in an overlapping scope now shares it with the new default. Pin what happens: their
    binding runs (a user's choice beats a default), the conflict shows in Settings, and the menu's `⌘[` / `⌘]` are
    untouched. If the store can't express "user wins" today, stop and report.
  - **E2E specs that guess the modifier**: about 18 spec files pick `ctrlKey` / `metaKey` by `process.platform`
    (`file-operations.spec.ts:679`, `accessibility.spec.ts:484`, …), which encodes the old mechanical rule. Every one
    that presses a moved key asks the running app for the combo (the registry, through `tauriPage.evaluate`) or names
    its platform's key from the decisions file; ❌ no new `isMac ? meta : ctrl`.
  - Linux leaves F-keys out of the bar (GTK takes them before the webview). Rows whose first key becomes an F-key chord
    there (`Shift+F5`, `Ctrl+F1`) get no registered accelerator on Linux, or a display-only one; the parity test's
    exception list says so. Windows follows Linux until it can be run.
  - Vitest's default platform is Linux, so tests far from the keymap will fail on a moved key. Fix each by asking the
    registry (`getDefaultShortcuts`) or by pinning its platform, ❌ never by restoring the old key.
  - E2E specs press some moved keys by hand: `favorites-menu.spec.ts:55-60` (its `Ctrl+D` special case goes),
    `ask-cmdr.spec.ts:56`, `operation-log.spec.ts:31`, and anything using Ctrl with an arrow or a bracket. Grep
    `apps/desktop/test/e2e-playwright` and `test/e2e-linux` for each moved combo.
  - **The browser demo's keymap goes per platform** (its own commit). `lib/demo/browser-shortcuts.ts` is one
    macOS-spelled map seeded for every visitor, so it's dead off macOS today. It becomes `Record<Platform, …>`, each arm
    in that platform's spelling, picked by `lib/demo/platform.ts`'s answer. The macOS arm keeps today's entries
    byte-identical. The Windows and Linux arms are the registry's defaults for that platform minus the combos a browser
    won't deliver to a page there: computed from `getDefaultShortcuts(id, platform)` and one cited browser-withheld
    table, ❌ not hand-written moved keys. "Won't deliver" means the page never gets the keydown, or can't cancel the
    browser's action (for example Chromium's `Ctrl+N`, `Ctrl+T`, `Ctrl+W`); a key the browser merely has a meaning for
    but lets the page `preventDefault` (check `Alt+←` / `Alt+→`, `Ctrl+Tab`, `Ctrl+PageUp` / `PageDown`, `F11` one by
    one) stays, and the demo claims it. Verify each entry against a browser vendor's own docs or source, cite it in the
    module, and leave unverified ones out. Where removing a combo leaves a command with no key, don't invent one: list
    those commands in the report. Test per platform: every arm entry is canonical for its platform, none is on that
    platform's withheld table, the arm changes only commands that hold a withheld combo, and the keymap with the arm
    applied has no conflicts.
  - The error screen claims its key in a capture-phase listener through `eventMatchesCommand` (`ErrorPane.svelte:90`),
    so it follows the registry; its chip already reads the command.
  - macOS: add `⌥←` / `⌥→` LAST in each list, so menus and chips keep showing `⌘[` / `⌘]`. Confirm no macOS command in
    an overlapping scope holds them (none on `5135d6d238`: `⌃⌥←/→` are the divider, `⌘⌥←/→` the tabs).
- **Test plan**: the three keymap snapshots (macOS: exactly four changed lines; Windows and Linux: every changed line
  matches the decisions file), the conflict test with empty allowlists on all three, the invariants test (red first: run
  it before the data moves and see it name today's `Ctrl+Alt+<letter>` keys), the parity test, the demo test,
  `pnpm check desktop-e2e-linux`.
- **DONE**: every line that changed in the Windows and Linux snapshots belongs to a command the decisions file lists,
  and every listed command's line equals its row (walk the list in the report); the macOS snapshot changed in exactly
  the four lines; `allowedConflicts` is empty on every platform; the demo map has three arms and the report lists the
  commands the demo leaves keyless off macOS; the real-X-event spec covers all 11 Multi-rename keys;
  `rg -n "process.platform === 'darwin'" apps/desktop/test` gains no hit.

### Milestone 7: the search dialog's and the viewer's fixed keys join the registry

- **Scope**: `lib/query-ui/` (`query-shortcuts.ts`, `QueryDialog.svelte`, `EmptyState.svelte`, `QueryBar.svelte`,
  `ModeChips.svelte`, `filter-chips/`), `lib/search/SearchDialog.svelte`, `lib/commands/` (ids, sources, the scope),
  `lib/shortcuts/scope-hierarchy.ts`, `routes/viewer/` (`viewer-keyboard.ts`, `ViewerStatusBar.svelte`),
  `intl/messages/en/queryUi.json` and `commands.json`, tests, `test/e2e-playwright/search-*.ts` and the viewer specs.
- **Intentions**:
  - **Search dialog**: the three `fixedKey` commands of `keymap-decisions.md` § "New commands" in a new scope for the
    dialog (`search.modeByNumber` is one command with the digit keys, like `favorites.openByNumber`). The router matches
    them with `eventMatchesCommand`. Chips switch to `commandId` mode; the tooltip and the two tips take the key as a
    placeholder filled from the effective shortcut, so they show the live key on every platform.
  - **Every other chip in the dialog** (§ "The code today" lists them: `⌥A/F/R`, `⌥C` / `⌥V`, `⌥⏎`, `⏎`, `⌘Enter`, and
    any tooltip beside them) renders from one canonical combo through `toDisplayShortcut`, so Linux and Windows read
    `Alt+F` and `Enter`, never a Mac glyph (the spike saw them on Linux). `aria-keyshortcuts` keeps the ARIA spelling.
    Those keys stay hardcoded (only the three above become commands), but each router reads its combo from the same
    per-platform constant its chip shows, so the two can't drift.
  - **The Enter combinations**: `routeEnterCombination` swallows the platform's primary modifier plus Enter and
    `Shift+Enter`; off macOS `Ctrl+Enter` stops falling through to bare Enter's action. Before touching `tipAi`'s
    `⌘Enter` clause, find out what `⌘Enter` does in AI mode on macOS today; if it's the swallowed no-op, the clause is a
    copy bug: stop and report (the lead decides between dropping the clause and binding the key).
  - **Viewer**: the search key and the regex and case toggles become `fixedKey` commands in `Viewer window`, macOS keys
    as today, Windows and Linux keys per the decisions file's § "Needs David" item 1 (settled as proposed).
    `handleModifierShortcut` and `handleSearchToggleKey` match them with `eventMatchesCommand`, and the toggles keep
    working while the search input has focus (the registry keys skip a focused input today: read `handleKeyDown`).
    `ViewerStatusBar`'s chip reads the command.
- **Landmines**:
  - A new scope is three places (`lib/commands/DETAILS.md` § "Adding a command"). The dialog serves Search and the
    selection dialogs; one scope covers both.
  - The dialog's chain skips `Main window`, the way `Main window/Ask Cmdr`'s does (`scope-hierarchy.ts:61`), so its keys
    shadow Ask Cmdr's new chat and the view modes and sorts by design (decisions § "Needs David" item 2). ❌ Not the
    Multi-rename sheet's shape, which inherits `Main window`. The conflict test then has nothing to allow.
  - The viewer's matching narrows on purpose: milestone 5 already limits search to the primary modifier; the toggles
    still take ⌘ or Ctrl plus Alt on every OS. After this, only each platform's own combo; pin both the new keys and the
    dropped ones (`⌃⌥R` on macOS, `Ctrl+Alt+R` and `Super+Alt+R` off macOS) in `viewer-keyboard` tests.
  - `fixedKey` ids join `FIXED_KEY_COMMAND_IDS`, so `DISPATCH_EXEMPT_IDS` follows.
  - The tips are existing strings: rule 5. The translator note that says the glyphs "must stay as-is" changes with them.
  - `lib/search/test-search-dialog-harness.ts` and the E2E helpers build events with `metaKey`; they need the platform's
    modifier.
  - **macOS changes here, both on § "Invariants" item 18's list**: the viewer drops `⌃⌥R` / `⌃⌥C`, and Settings >
    Keyboard shortcuts gains read-only "Fixed" rows for the six new commands. `eventMatchesCommand` also lets `⌘N` and
    `⌘H` match through the Latin fallback on a non-Latin layout, which today's `e.key` test misses; that's a fix, pin
    it. Nothing else in the dialog may change on macOS: its unit tests run under `withPlatform('macos')` with today's
    events and expectations untouched.
  - Focus edge cases: a filter-chip popover open inside the dialog (Escape closes only the popover, pinned in
    `FilterChips.svelte.test.ts`), the recent-items popover, and the AI prompt field all still see the new commands
    match; the selection dialog, which hides the scope chip, still refuses `⌥I` / `Alt+I`.
- **Test plan**: `query-shortcuts` unit tests per platform (Ctrl works and Super doesn't off macOS; ⌘ on macOS; modifier
  supersets rejected), the viewer keyboard tests per platform, the conflict and vocabulary tests pick the new commands
  up for free, the keymap snapshots gain exactly the new commands' lines (macOS keys equal today's hardcoded ones), the
  search and viewer E2E specs on Linux.
- **DONE**: `rg -n "[⌘⌥⌃⇧⏎]" apps/desktop/src/lib/query-ui apps/desktop/src/lib/search -g '!*.test.ts' -g '!*.md'` hits
  only comments; no glyph in `queryUi.json` values outside a `macos {…}` branch; `handleSearchToggleKey` and the
  viewer's search branch read no modifier flag; `ViewerStatusBar` has no hardcoded key; a Linux E2E screenshot or DOM
  assertion of the open search dialog shows no Mac glyph.

### Milestone 8: Quit and full screen (#1110, #1114)

- **Scope**: `lib/commands/` (the four native commands, a new command), `shortcuts-store.ts`, the banner,
  `routes/(main)/command-handlers/`, `src-tauri/src/platform/` (capabilities), `src-tauri/src/quit/` or
  `app_lifecycle.rs` (one IPC), `window_state/`, `crates/cmdr-menu/` (two rows), `menu.json`, tests and docs.
- **Intentions**:
  - **"Native" becomes a per-OS fact the backend states.** One new capability (name it after the surface, for example
    `appMenu`: the OS provides an app menu that owns Quit, Hide, Hide others, and Show all), `true` on macOS only.
    `app.hide`, `app.hideOthers`, and `app.showAll` take `needsCapability`, so off macOS they aren't listed, hold no
    key, and reserve nothing: the phantom "reserved by macOS" warnings go, and the three "macOS" strings can only render
    on macOS.
  - **`app.quit` works everywhere.** On macOS nothing changes (AppKit's Quit item, read-only row, no palette row).
    Without the app menu it's an ordinary command with the keys the decisions file gives it (none): a handler that calls
    ONE new backend command, a palette row, and a menu row at the end of the File menu ("Quit Cmdr" on Linux, "Exit
    Cmdr" on Windows, via the label's platform select). The backend command takes the SAME path closing the main window
    takes: extract the `CloseRequested` branch's body (`app_lifecycle.rs:67-76`: ask `request_quit`; if held, do nothing
    more; else `stop_background_services()` and `exit(0)`) into one named function, and have both the window event and
    the command call it. ❌ No `@tauri-apps/plugin-process` exit, no second quit path, no hand-rolled gate call in the
    command.
  - **`view.toggleFullScreen`**, on all three OSes, with no new capability: `needsCapability: 'fullScreen'`, and
    `full_screen: true` in the Windows and Linux arms, which brings back the two Escape settings and the hint toast. The
    handler reads `isFullscreen()` and calls `setFullscreen(!…)` on the main window, the same API `escape-key.ts`
    already uses to leave macOS's native full screen, so on macOS it enters the same full screen AppKit's View menu item
    does (verify once by hand on macOS: same Space, same green-button state, Escape and the AppKit item both leave it).
    Keys per the decisions file: `F11` on Windows and Linux, none on macOS (AppKit's item keeps its own key, and Cmdr
    binds nothing there). Menu: a View row on Windows and Linux; on macOS no Cmdr row, since AppKit adds its own item to
    the View menu. If the code shows the command can't drive the same full screen on macOS, stop and report: then it
    needs its own capability, and the plan says why in one line.
  - **Quit's menu row** carries no accelerator on either OS, display-only or otherwise: no `Alt+F4` hint (the Windows
    menu model is task 63's).
- **Landmines**:
  - `DispatchExemptId` family 1 is a compile-time union and `NATIVE_SHORTCUT_COMMAND_IDS` a typed const, so neither can
    vary by OS: `app.quit` leaves both, gets a handler in the record (`command-handler-record.test.ts` pins the split),
    and its "native" answer (`isNativeShortcutCommand`, the editor's read-only row) becomes a call-time
    `nativeShortcut && hasCapability('appMenu')`. On macOS it must still never reach JS dispatch: the dispatch map takes
    Tier 1 commands (`showInPalette`, read by `buildShortcutMap` at `initShortcutDispatch` and on every shortcut change,
    and by `getPaletteCommands` per call), so `showInPalette` becomes a getter on the capability, as `name` already is.
    Keep "AppKit owns this" in ONE place.
  - No quit IPC exists today (`quit/commands.rs` has only `quit_confirm`, `quit_cancel`, and `relaunch_app`). The gate
    also sits on `RunEvent::ExitRequested` (`app_lifecycle.rs:153`), which is a backstop, not a reason to add an exit
    elsewhere. ❌ Never a path that skips the gate (`std::process::exit`); work in flight must hold the quit and ask
    (`src-tauri/src/quit/`). The extracted function must keep the window open while the gate holds, as the close path
    does (its comment says why).
  - The capability struct has an arm per OS (`platform/macos.rs`, `linux/mod.rs`, `windows.rs`); regenerate bindings.
    `capability_rows_test.rs` pins menu rows to capabilities per OS.
  - `F11`: GTK may take a bare F-key accelerator before the webview (the bar leaves F-keys out on Linux), so the row
    shows it display-only and the keydown dispatch does the work. Some window managers also bind `F11` themselves.
    `window_state` already asks `window.is_fullscreen()` when it saves (`window_state/mod.rs:241,319,348`), so it
    records whichever path entered full screen; test that the toggle path is one of them.
  - The main window already holds `core:window:allow-set-fullscreen` (`capabilities/default.json`), and `escape-key.ts`
    already calls `setFullscreen(false)` from the frontend, so the toggle can live beside it.
  - **The browser demo**: `appMenu` is a type error in `lib/demo/platform.ts`'s `MAC_VISITOR_CAPABILITIES` until the
    demo says what a Mac visitor gets. Off macOS the demo answers `NO_CAPABILITIES`, so `appMenu` is false and
    `app.quit` turns into an ordinary command there: a Windows or Linux visitor would get a Quit palette row whose IPC
    has no backend. The demo hides Quit (its handler record or a demo-only exclusion; pick the existing pattern the demo
    uses for other backend-only commands). A Mac visitor gets `view.toggleFullScreen` (`fullScreen: true` there): route
    it to the demo's browser full screen (`lib/demo/traffic-lights.ts`'s `canFullScreen`) or hide it; ❌ never a Tauri
    window call in the demo. Test both per visitor platform.
  - **Quit over a dialog**: `app.quit`'s `whileDialogOpen` is `NATIVE_MENU_BAR_ITEM` today. Off macOS it needs a real
    policy: it runs over dialogs (the quit gate is what asks about work in flight), so the File menu's Quit never greys
    out and a stuck dialog can't trap the user. The milestone 5 mnemonic mask doesn't block a click on the bar; pin that
    Quit works from the palette and from the menu while a dialog is up.
  - **Quit from another window**: if the Settings or queue window carries the main bar on Linux (milestone 5's report),
    File > Quit there takes the same backend command; test it doesn't close only that window.
  - **Full screen and the Escape chain**: on Windows and Linux, Escape in a dialog, a popover, or a text field still
    closes or clears that first; `exitFullScreenOnEscape` runs only for an Escape nothing used (`escape-key.ts`). macOS
    keeps that path byte-identical.
  - `menu.app.quit` is an existing string: rule 5. The full-screen command's label and menu row are new strings (English
    plus `@key`).
  - `appMenu` and the `full_screen` flip fail milestone 1's fixture pin test until the fixture learns them; that's the
    test working.
  - macOS change, on § "Invariants" item 18's list: the palette gains a Full screen row, and the keymap snapshot gains
    `view.toggleFullScreen` with an empty list. Nothing else on macOS moves: the four native rows, their read-only
    state, and the palette's lack of Quit and Hide stay.
- **Test plan**: registry tests under each platform plus the capability (macOS: four read-only native rows; Windows and
  Linux: one rebindable Quit, no Hide commands, no reserved banner for `Ctrl+H`; macOS: a full-screen command with no
  key and no Cmdr menu row; a saved `app.quit` customization loads off macOS and is still dropped on macOS), a handler
  test that Quit calls the new IPC, a Rust test that the extracted function holds the quit while the gate holds (the
  gate's tests in `quit/tests.rs` show the harness), bar snapshots, reachability on three platforms, a Linux E2E spec
  for `F11` and Escape if the Docker display supports full screen (if it doesn't, say so in the report and cover the
  toggle with a unit test).
- **DONE**: on Linux, Quit is in the palette and the File menu and runs the gate; `F11` toggles full screen; the Escape
  settings show; the `CloseRequested` branch and the quit command call one function, and no frontend code imports the
  process plugin's `exit`; the capability struct gained exactly one field (`appMenu`), and the report carries the macOS
  hand check of full screen. #1110 and #1114 can close when this lands.

### Milestone 9: system-reserved keys per OS, and the copy rows that waited

- **Scope**: `settings/sections/keyboard-shortcuts-banner.ts` and its test, the invariants test, the catalog rows listed
  in § "The code today", call sites of those strings.
- **Intentions**:
  - **One reserved table for three platforms.** The macOS table becomes `Record<Platform, …>`. Each entry names who
    takes the combo: macOS features as today; on Windows, Windows itself; on Linux, the desktops that take it (GNOME,
    KDE Plasma, Xfce, Cinnamon, Ubuntu), since Cmdr can't ask which desktop is running until task 59. The banner names
    EVERY desktop that takes the combo, never a sample (for example, `Alt+F2` is taken by all five), and keeps its "use
    anyway" choice. Only entries the decisions file marks verified go in. When task 59 brings the desktop value, the
    Linux entries narrow to the running desktop with no shape change.
  - The invariants test from milestone 6 reads this table: no Windows default is reserved by Windows; a Linux default a
    desktop takes must have a second key no desktop takes.
  - **The copy rows.** Each literal glyph combo in prose becomes a `{combo}` placeholder filled with the command's
    effective first key in display form, through #1103's mechanism (rule 5). The three "macOS" strings stay worded as
    they are and are recorded as macOS-only surfaces (milestone 8 made that true). The global go-to-latest description
    takes its combo from the setting's current value.
- **Landmines**:
  - A user can remove a command's key, and then "Copy files first with {combo}" has nothing to say. Decide per string: a
    fixed key (the viewer's search, a `fixedKey` command since milestone 7) is always there; for a rebindable one, a
    second variant without the key clause, picked at the call site. ❌ Never render an empty chip or a dangling "with".
  - The reserved keys are canonical strings in each platform's own spelling, or they silently never match
    (`shortcut-vocabulary.test.ts` has a block for the macOS table; extend it to all three).
  - The banner's labels are catalog keys; new feature names are new English strings with `@key` descriptions. The
    desktop list is a list, so it goes through `formatConjunctionList` (`lib/intl/DETAILS.md`; raw `Intl.ListFormat`
    throws on Catalina), ❌ never `', '`-joined in code: other languages join and order lists differently. Desktop names
    are proper nouns and stay untranslated in the `@key` note.
  - `settings.fileExplorer.suppressQuickLookHint.description` is a Quick Look surface (macOS only); check whether it
    needs a placeholder at all.
- **Test plan**: banner tests per platform, the vocabulary block for three tables, the i18n checks (`pnpm check --fast`
  runs them), a render test for each changed string with and without a bound key.
- **DONE**: no catalog value holds a literal ⌘, ⌥, ⌃, or ⇧ outside a `macos {…}` branch; `copy-decisions.md` no longer
  lists rows waiting on #1102.

### Milestone 10: docs and close-out

- **Scope**: docs only.
- **Intentions**: every C+D touched says what's true now (current state, no history, no milestone tags):
  - `lib/shortcuts/` C+D: three-way defaults, `defaultShortcuts`, the per-platform tests, the AltGr guard, the
    `IntlBackslash` rule; delete § "Why platform-specific storage?"'s non-injective warning and the "macOS-native"
    wording that's now per-OS.
  - `lib/commands/` C+D: the `shortcuts` shape and helpers, "Adding a command" (three platforms, the snapshot, the
    parity test), the per-OS native fact, the search dialog scope.
  - `lib/platform/` C+D: the keymap no longer asks `isMacOS()`; the new capability; `fullScreen` on three platforms.
  - `crates/cmdr-menu/` C+D and `src-tauri/src/menu/` C+D: three platforms, the parity test, the Quit and full screen
    rows.
  - `lib/query-ui/DETAILS.md`, `lib/multi-rename/` docs, `routes/(main)/DETAILS.md`, `routes/viewer/` C+D (the viewer's
    fixed-key commands), `lib/demo/` C+D (the per-platform seeded map), `settings/sections/DETAILS.md`,
    `file-explorer/pane/DETAILS.md`, `file-explorer/navigation/` docs where they name a key or the old carve-outs.
  - `docs/notes/windows-port/keymap-decisions.md`: § "Needs David" becomes what was settled; check its Linux mnemonic
    lines still match what shipped (the mask, which windows carry the bar).
  - `docs/notes/windows-port/keymap-survey.md`: apply the corrections from #1102's comment (GNOME and `Alt+F1`, Xfce,
    Cinnamon, AltGr, WebView2's extra keys, the six missing TC keys, Explorer's `Ctrl+D`, the two JSON status slips),
    and point at `keymap-decisions.md` as the table that shipped.
  - A pointer for agents to the Total Commander key list (David's vault note, named in § "Inputs"; local only, not
    redistributable), in `keymap-decisions.md` or `lib/shortcuts/DETAILS.md`.
  - `docs/specs/windows-port-plan.md`: task 26 marked **Done** with one paragraph of what shipped; the line under "Off
    macOS, `Ctrl+D` is bound twice" updated; task 27's "rows waiting on #1102" updated; the manual Windows checks from §
    "What can't be tested here" added where the plan keeps its Windows checklist.
  - `docs/architecture.md` if its shortcuts or menu entries name the old shape.
  - Remove this plan per `docs/specs/DETAILS.md` § "Wiping a shipped spec" only when the lead says the work has landed;
    until then leave it.
- **Landmines**: `claude-md-length`, `docs-reachable`, `docs-dead-links`, `docs-link-text`. Keep each `CLAUDE.md` at
  300–400 words.
- **Test plan**: `pnpm check --fast`; `rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9])\b'` over every path the work touched,
  outside this file, is empty.
- **DONE**: a fresh agent can add a command with a key on all three platforms from the docs alone.

## What can't be tested here, and what covers it

- **Windows keymap, conflicts, vocabulary, reachability, invariants**: unit tests under `withPlatform('windows')`.
- **The Windows menu bar's rows and accelerators**: `cmdr-menu`'s snapshots and the parity test (the data has no `cfg`),
  and muda parsing every accelerator on a Mac host.
- **Linux, for real keys**: the Docker E2E lane (`pnpm check desktop-e2e-linux`) on milestones 2 and 5 to 8. Its
  `pressKey` is a synthetic DOM event that GTK never sees, so anything GTK takes first (bar mnemonics, F-keys) needs the
  real-X-event spec milestone 5 adds.
- **macOS full screen through the toggle**: one hand check in milestone 8 (the window API drives the same native full
  screen as AppKit's item); no lane runs macOS full screen.
- **Not covered until Cmdr runs on Windows** (each goes on the port plan's Windows checklist in milestone 10):
  - What WebView2 reports for AltGr (`AltGraph`, `ctrlKey`, `altKey`), and for LeftCtrl+LeftAlt on an AltGr layout.
  - The browser keys WebView2 keeps (#1115): until it lands, `Shift+F5`, `Ctrl+F5`, `Ctrl+R`, `Ctrl+J`, `Ctrl+G`, and
    `Alt+←/→` may not reach Cmdr.
  - Whether a Win32 menu accelerator fires before the webview, and what `Alt+<letter>` does with a menu bar present
    (task 63).
  - `F11`, `Alt+F4` through the quit gate, and `Ctrl+Alt+→/←` on Intel graphics drivers.
- **Not covered on Linux by any lane**: what each desktop really grabs. The reserved table carries only entries verified
  from the desktops' own schema files.

## Invariants (the conformance review checks these)

1. macOS defaults equal the milestone 1 snapshot except the four `⌥←` / `⌥→` lines, plus the lines of the commands
   milestone 7 adds (each equal to the key it hardcoded before) and milestone 8's `view.toggleFullScreen` with an empty
   list.
2. The Windows and Linux defaults equal `keymap-decisions.md`; a command it doesn't list keeps its milestone 1 line.
3. No source under `lib/commands/sources/` reads the platform or a capability at module load; per-OS tests use the
   capability fixture pinned to the Rust arms, and the data rules (6, 7) run over every command's raw arm.
4. No function converts a combo from one platform's spelling to another's at read time; `toPlatformShortcut` and both
   copies of the `⌃`→Shift hack are gone.
5. Zero default conflicts on each of the three platforms, with empty allowlists.
6. Every default on every platform is a string `formatKeyCombo` produces.
7. No Windows or Linux default is `Ctrl+Alt+<letter>` or holds `Super`.
8. Every menu bar row's accelerator equals the registry's first default for that platform, or is on the reasoned
   exception list.
9. A keypress with `AltGraph` set never resolves through the physical or Latin fallback.
10. No key handler in the surfaces milestones 5 and 7 name compares a macOS combo string or reads `metaKey` (alone, or
    `metaKey || ctrlKey` as "either") to mean "the primary modifier"; every such read elsewhere is on milestone 5's
    reported list.
11. Quit off macOS is one backend command calling the same function as the main window's `CloseRequested` branch; no
    frontend exit, no second path.
12. `app.hide`, `app.hideOthers`, and `app.showAll` exist only where the OS has an app menu (`appMenu`, the one new
    capability); `view.toggleFullScreen` exists on all three behind `fullScreen`, with no key and no Cmdr menu row on
    macOS.
13. No catalog value holds a literal Mac modifier glyph outside a `macos {…}` branch.
14. The viewer's search key and toggles and the search dialog's keys are `fixedKey` registry commands; the dialog's
    scope chain doesn't include `Main window`.
15. The browser demo's seeded map has an arm per platform, each canonical; the Windows and Linux arms are the registry's
    defaults minus a cited browser-withheld table, with no key invented, and the demo lists no Quit row and makes no
    Tauri window call.
16. `svelteInputs` and the `svelte` CI filter cover every non-frontend file a vitest test reads (the Rust arms, the
    parity TSV, `ids.rs`, `rows.rs`).
17. No milestone tags outside this file.
18. macOS behavior changes are exactly these, each pinned by a test: the four `⌥←` / `⌥→` defaults (and `⌥←` / `⌥→`
    joining macOS's native text keys); the viewer answers only ⌘ for copy, select all, and search (no `⌃C`, `⌃A`, `⌃F`)
    and only `⌘⌥R` / `⌘⌥C` for its toggles (no `⌃⌥R` / `⌃⌥C`); a saved rebind of `nav.home`, `nav.end`, `nav.pageUp`, or
    `nav.pageDown` also applies in the places browser and the servers hub; the search dialog's `⌘N` / `⌘Y` also match on
    a non-Latin layout; the search dialog's recent searches move from `⌘H` to `⌘Y`, leaving `⌘H` to Hide, and its
    empty-state tip names `⌘Y` and drops the `⌘Enter` clause (`⌘Enter` does nothing there; the AI-on tip is gone);
    Settings lists the six new fixed-key rows; the palette gains Full screen; Move here (`edit.pasteAsMove`, `⌥⌘V`)
    moves nothing while a text field has focus, from any road (it moved files from the quick filter or the path bar,
    since the menu accelerator fires before the field); a key the user bound to one command beats another command's
    default on it (item 20); a rebind or reset to a bar row's own key comes back as the bar built it (item 21). Anything
    else that moves on macOS is a regression.
19. On Linux, while any main-window dialog, overlay, or the palette is up, the main window's bar mnemonics are off and
    every `Alt+<letter>` default reaches the page; after the last one closes, they're back. A real-X-event spec in the
    Linux lane proves it.
20. A key the user bound beats a default: when two commands in overlapping scopes hold one combo, one through a key in
    its custom entry that isn't among its defaults and the other by default, the default is shadowed for every reader
    (the dispatch map, `comboMatchesCommand`, the menu bar's accelerators, the popup labels), so no default added now or
    later takes a user's key. Two user bindings or two defaults keep scope depth, then registry order. Fixed-key and
    macOS-native defaults are never shadowed. One implementation, `lib/shortcuts/binding-priority.ts`, pinned by
    `shortcut-dispatch.test.ts` and `shortcuts-store.test.ts`.
21. A push onto a bar row (`update_menu_accelerator`) of the row's own `MENU_BAR` key behaves exactly like the built
    row: registered where the bar registers it (macOS's bare `F5`, `Tab`, `Shift+Space`), display-only where the bar
    only shows it (Select same kind's `⌥⇧=`). Every other combo takes the modifier floor. One door,
    `cmdr_menu::bar::rebound_row_key`, which `bar_parity_test.rs` checks the bar through, with no bare-key exceptions.
