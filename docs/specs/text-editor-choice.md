# Text editor choice

**The quest**: A user asked for F4 to open text files in Sublime Text, the way Double Commander lets them pick an
editor. Today F4 runs `open -t`, which always hands the file to macOS's plain-text handler (TextEdit, unless someone
changed it in Finder). David approved the product design; this plan turns it into three milestones.

**What the user gets**: a "Text editor" card in Settings > Behavior > Navigation & file ops with one dropdown: "System
default (TextEdit)" first, then every app macOS lists as a plain-text editor with its icon, then "Choose an app…". F4,
the search-results F4, and the ⇧F4 new-file auto-open all launch the chosen app. A one-time toast teaches the setting,
and a chosen app that's been removed still opens the file, in the system default.

## Loud rules

- ❗ This plan feeds the loop in `docs/guides/multi-agent-refactors.md`: one agent per milestone, sequential, a report of
  ≤350 words, one commit (or a few) per milestone, messages leading with impact and carrying no AI attribution.
- ❌ No `run_in_background`; foreground checks only. ❌ Never tail or head `pnpm check`.
- ❌ Never add `objc2-uniform-type-identifiers` (directly or through a crate feature), and never call
  `URLsForApplicationsToOpenContentType:` / `URLForApplicationToOpenContentType:`. That crate's `#[link]` loads
  `UniformTypeIdentifiers.framework` (macOS 11), dyld then refuses to start Cmdr on the 10.15 floor, and
  `desktop-macos-framework-floor` fails the build. The LaunchServices C API below answers the same question on 10.15+.
- ❌ Never ask about a made-up `.txt` URL: `URLsForApplicationsToOpenURL:` on a path that doesn't exist returns zero apps
  (measured, see Evidence).
- ❌ No hand-maintained editor table, no `/Applications` scan, no Refresh button. What macOS reports IS the list, its
  oddities included.
- ❌ Don't change what F4 does off macOS: Linux keeps `xdg-open` and shows no Settings row.
- ❌ Don't loosen the pane guard. `canOpenInEditor` keeps refusing rows with no real file on this Mac; the setting only
  changes which app launches.
- ❌ Allowlist warns (`file-length`, `claude-md-length`, `jscpd-rust`, `jscpd-frontend`) get surfaced in the milestone
  report, never bumped (`.claude/rules/file-length-allowlist.md`).
- ❗ TDD in every milestone: watch the new test fail for the right reason before writing the code that passes it.
- ❗ All UI copy in this spec is a DRAFT for David's review.

## Evidence: what macOS calls a text editor

A compiled Swift probe on David's Mac (verified on macOS 26.6 / Darwin 25.6.0, 2026-09-11):

- **`LSCopyAllRoleHandlersForContentType("public.plain-text", kLSRolesEditor)`**: five bundle ids: TextEdit, Warp, Path
  Finder, LibreOffice, Xcode.
- **The same call with `kLSRolesViewer`**: seven, all viewers: Instruments, Script Editor, Firefox, Safari, Claude,
  Chrome, Notes.
- **`NSWorkspace` `URLsForApplicationsToOpenContentType:` for plain text**: 13 paths, viewers and editors mixed, and Warp
  twice (two copies on disk). That API has no role filter.
- **`URLsForApplicationsToOpenURL:` on a nonexistent `/tmp/….txt`**: zero apps.
- **`LSCopyDefaultRoleHandlerForContentType("public.plain-text", …)`**: `com.apple.TextEdit` for both `kLSRolesEditor`
  and `kLSRolesAll`, matching `URLForApplicationToOpenContentType:`. David's LaunchServices prefs carry no plain-text
  override, so how `open -t` behaves with a NON-TextEdit default is unverified here (M1 spike).
- **Editor-role handlers for `public.source-code` and `public.json`**: the plain-text set minus Path Finder (and minus
  LibreOffice for JSON); Markdown adds OpenKnowledge.
- **`man open`**: `-t` "Causes the file to be opened with the default text editor, as determined via LaunchServices".

So the editor-ROLE C query is the right source: it keeps browsers out, answers one entry per bundle id, and exists from
macOS 10.4 to today. `apps/desktop/src-tauri/src/macos_icons.rs` already calls its sibling
`LSCopyDefaultRoleHandlerForContentType` through the `core-services` crate, a direct dependency, and `CoreServices` is
recorded at 10.0 in `scripts/check/checks/macos-framework-versions.json`. `core-services` 1.0.0 (the `Cargo.lock` pin)
declares both as `(CFStringRef, LSRolesMask)`, returning `CFStringRef` and `CFArrayRef`, exports `kLSRolesEditor`,
`kLSRolesViewer`, and `kLSRolesAll`, and marks nothing `#[deprecated]` (read from the crate source, 2026-09-11). Apple
deprecated both in macOS 12 yet keeps them in the macOS 26 SDK, and they're C symbols, so
`desktop-rust-macos-availability` (Objective-C selectors only) has nothing to flag.

⚠️ **Unverified: whether the requester's editors show up.** Neither Sublime Text nor VS Code is installed on David's
Mac. VS Code declares plain text by EXTENSION (`txt`) plus the OSTypes `TEXT` / `utxt` with role `Editor`, not by UTI
(`build/lib/electron.ts` in `microsoft/vscode`, `main`, fetched 2026-09-11), so whether LaunchServices files it under
`public.plain-text` is exactly the open question. M1 starts with a spike for it.

## Map of the current code

- **The launch**: `open_in_editor(path)` in `apps/desktop/src-tauri/src/commands/file_actions.rs`, four sync
  `#[cfg]` arms: `open -t` on macOS and `xdg-open` on Linux (both `not(feature = "playwright-e2e")`), an error string
  elsewhere, and ONE `playwright-e2e` arm, ungated by OS, that records into `crate::open_mock`. Once it asks
  LaunchServices anything it has to become `async` with a timeout: a sync command runs on the main thread
  (`commands/CLAUDE.md`).
- **The frontend wrapper**: `openInEditor(path)` in `apps/desktop/src/lib/tauri-commands/file-actions.ts`,
  `throwIpcError` on failure.
- **The guard**: `apps/desktop/src/lib/file-explorer/pane/editor-open.ts` (`canOpenInEditor`, `openInEditorOrExplain`,
  `EditorOpenOutcome = 'opened' | 'refusedNotOnThisMac'`). All three callers go through `openInEditorOrExplain`:
  - `file.edit` in `apps/desktop/src/routes/(main)/command-handlers/file-handlers.ts`, which tracks `editor_opened` only
    on `'opened'`.
  - The search-results F4, `openSnapshotFileWith` in `apps/desktop/src/lib/file-explorer/pane/search-pane-keys.ts`.
  - The ⇧F4 auto-open: `onOpenInEditor` in `pane/DualPaneExplorer.svelte`, called from `handleNewFileCreated` in
    `pane/dialog-state.svelte.ts`. `DualPaneExplorer.svelte` is at its size cap, and nothing here needs to touch it.
- **Tests pinning the call**: `apps/desktop/src/routes/(main)/command-dispatch.characterization.test.ts` asserts
  `openInEditor` ran exactly once with `ENTRY.path`, and its mock resolves `undefined`; `pane/search-pane-keys.test.ts`
  asserts `toHaveBeenCalledWith('/f.txt')` (exact arguments, so a new parameter fails it) and mocks neither
  `$lib/settings` nor a return value. `pane/navigation-transaction.test.ts` and several `command-handlers/*.test.ts`
  files mock `openInEditor` without dispatching `file.edit`.
- **The template, "Open terminal here"**:
  - Rust `apps/desktop/src-tauri/src/file_system/terminal.rs`: `parse_choice`, pure `launch_argv`, pure
    `resolve_choice(setting, is_installed)`, `installed_app_path` (`URLForApplicationWithBundleIdentifier:`), `app_entry`
    (icon via `open_with::load_app_icon` plus `icons::rgba_to_data_url`), and a `launch` split by cfg that records the
    folder under `playwright-e2e`.
  - Commands `list_terminal_apps` (`blocking_with_timeout_flag`, 2 s, `TimedOut<…>`) and `open_terminal_here`
    (`blocking_typed_result_with_timeout`, 5 s, typed error), both `#[cfg(target_os = "macos")]`.
  - Frontend `apps/desktop/src/lib/open-terminal/`: `terminal-app-setting.ts`, `first-use-pick.ts`,
    `open-terminal-here.ts`, two `*ToastContent.svelte` bodies, `open-terminal-toasts.a11y.test.ts`. Its `DETAILS.md` §
    The toasts carries the lesson this feature must keep: one toast id, `dismissToast` then `addToast`, ❌ never a
    one-slot `toastGroup`.
  - Settings `apps/desktop/src/lib/settings/sections/TerminalAppSelect.svelte` plus `terminal-app-options.ts`, rendered
    in the Terminal card of `NavigationAndFileOpsSection.svelte`; the registry entries `behavior.openTerminalHereApp` and
    `behavior.openTerminalHereToastSeen` in `settings/definitions/behavior.ts`; the deep link
    `openSettingsToTerminalApp()` under surface `'open-terminal-toast'` in the `SettingsSurface` union of
    `settings/settings-window.ts`.
- **App helpers**: `apps/desktop/src-tauri/src/file_system/open_with.rs` (`read_app_display_name`,
  `read_bundle_identifier`, `load_app_icon`, `open_paths_with`, `pick_app_via_open_panel`). `file_system/mod.rs` declares
  the module `#[cfg(target_os = "macos")]`; its AppKit code sits in a private `mod imp` exported through one
  `pub use imp::{…}` list, so a moved helper goes inside `imp` and onto that list.
- **LaunchServices C precedent**: `macos_icons.rs` imports `core_services::{LSCopyDefaultRoleHandlerForContentType,
  kLSRolesAll}`, wraps the +1 result with `wrap_under_create_rule`, and gives every `unsafe` block its own `// SAFETY:`.
- **Platform gating in Settings**: a registry setting has NO platform flag. Only `SearchableRow.macOSOnly` exists
  (`settings/types.ts`), dropped at index build by `settings/sections/searchable-rows.ts::searchableRowEntries`. The
  terminal row renders on Linux and sits at "Checking…" forever (second paragraph of `open-terminal/DETAILS.md`).
- **E2E**: `open_mock` stores a `Vec<PathBuf>`, read back by `e2e_opened_paths`. The "New file round-trip" test in
  `apps/desktop/test/e2e-playwright/file-operations.spec.ts` asserts the ⇧F4 flow recorded the new file's path. The
  fixture's global `afterEach` (`apps/desktop/test/e2e-playwright/fixtures.ts`) fails a test that leaves a toast up. The
  i18n capture's `new-file-dialog` surface only OPENS the dialog, so it never reaches the editor.
- **i18n**: `desktop-i18n-coverage` is an ERROR both for a missing key and for a value left identical to English in a
  full locale (`scripts/check/checks/desktop-i18n-coverage.go`). Ten full locales ship (`de`, `es`, `fr`, `hu`, `nl`,
  `pt`, `sv`, `vi`, `zh`, `zh-Hant`) plus the `en-GB` and `en-AU` overlays. `desktop-message-keys-unused` is ALSO an
  error, in the fast lane: an `en` key no `.ts`, `.svelte`, or `.rs` source names fails the build, so a key can't land
  before the code that references it. `desktop-i18n-term-consistency` is a warn that pairs keys by identical English
  across namespaces; the terminal's toast buttons are `commands.handler.openTerminalHere.{dismiss,openSettings}`.
- **Analytics**: `analytics/config_shape.rs` auto-ships every bool and number setting; a string setting ships only when
  listed in `CATEGORICAL_STRING_KEYS`. `analytics-settings-defaults` regenerates
  `apps/analytics-dashboard/src/lib/server/settings-defaults.gen.json` from the registry: locally it rewrites the file
  and passes, in `--ci` an uncommitted rewrite fails.

## Decisions

1. **Rust answers "which apps, which one, and did it launch"; the frontend owns the stored value and the words.** Same
   split as the terminal: the stored choice is PASSED into each command, because Rust's settings loader is a
   startup-time read only (`settings/CLAUDE.md`). Why: the one precedent works, and the frontend already owns
   persistence and cross-window sync.

2. **The Rust side is a new `text_editor.rs` beside `terminal.rs` under `file_system/`.** Unlike `terminal.rs`, the
   module itself is NOT gated: the Linux and `playwright-e2e` arms of `open_in_editor` answer the same
   `EditorOpenReport` / `OpenInEditorError`, so those wire types compile everywhere, and everything else in the file is
   `#[cfg(target_os = "macos")]` (a `mod imp`, the `open_with.rs` shape), which keeps Linux free of dead-code warnings.
   `installed_app_path` and the icon-to-data-URL step move from `terminal.rs` into `open_with.rs` as shared helpers, so
   the two modules don't copy them. Why not grow `open_with.rs`: that file is the context menu's "Open with"
   candidates with their own cache, while this feature has its own vocabulary (choice, fallback, report) that reads
   better alone.

3. **The stored value is one string, `behavior.textEditorApp`, default `system`.** Rust's `parse_choice` tells three
   shapes apart structurally:
   - `system` (or empty): the macOS plain-text default, launched with `open -t <file>`, unchanged from today.
   - An absolute path: a "Choose an app…" pick, launched with `open -a <app path> <file>`.
   - Anything else: a bundle id, launched with `open -b <id> <file>`.

   A listed app stores its bundle id, so it survives an update that moves or renames the bundle, and LaunchServices
   picks which copy to launch (the probe found Warp twice on disk). A pick stores its bundle id ONLY when
   `URLForApplicationWithBundleIdentifier:` resolves that id to the very bundle picked, compared after
   `std::fs::canonicalize` on both sides (the dialog's spelling and LaunchServices' can differ by a symlink or a trailing
   slash), and the path otherwise (no bundle id, or a second copy chosen on purpose). That canonicalization happens once, at pick time, by asking
   `list_text_editors(<picked path>)` for its `chosenId`. ❌ Browsing Settings never rewrites a stored value. Why a
   sentinel rather than an empty default: `system` reads plainly in `settings.json`, and no bundle id is a bare word.

4. **The list is `LSCopyAllRoleHandlersForContentType("public.plain-text", kLSRolesEditor)`** through `core-services`.
   Each id resolves with `URLForApplicationWithBundleIdentifier:` (an id that doesn't resolve, or resolves to a bundle
   no longer on disk, is dropped), the system default's own id is removed (it's already the first row), and the chosen
   app is appended when it isn't listed (a pick of an app that doesn't claim plain text, or the default pinned
   explicitly). `chosenId` is the stored choice in canonical form (`system`, a bundle id, or a path, per Decision 3),
   and `null` once the chosen app is gone. Names via `read_app_display_name`, icons
   via `load_app_icon`; the frontend sorts rows by name in the app's locale, since that's presentation. The system
   default's name comes from `LSCopyDefaultRoleHandlerForContentType("public.plain-text", kLSRolesAll)`, the call
   `macos_icons.rs` already makes per extension. Why: the Evidence section. **Fallback rule**: if the M1 spike shows
   Sublime Text or VS Code missing under plain text, take the union of editor-role handlers for `public.plain-text`,
   `public.text`, and `public.source-code`, deduplicated by bundle id, and record the evidence in
   `file_system/DETAILS.md`. "Choose an app…" covers anything still missing.

5. **`open_in_editor` answers with a report, not a bare success.** New shape:
   `open_in_editor(path, app_choice, ask_about_other_editors) -> Result<EditorOpenReport, OpenInEditorError>`.
   - **`outcome`**: `opened` or `chosen_app_missing_opened_default_instead`.
   - **`openedInName: string | null`**: the display name of the app that got the file (the chosen app, or the system
     default's resolved name).
   - **`otherEditorsInstalled: boolean | null`**: `null` unless asked. When asked, whether the list from Decision 4
     minus the system default is non-empty. Computed AFTER the launch, so the hint's query never delays the editor, and
     from ids and installed-ness only (❌ no names, no icons): it shares the launch's 5 s deadline, and a deadline that
     expires after `open` spawned would word an editor that did open as `timedOut`.
   - **`OpenInEditorError`**: `launchRefused { errno }` or `timedOut`, typed like `OpenTerminalError`.

   Why one IPC: the terminal's first use costs two (list, then launch), and the only extra facts the hint needs here are
   one boolean and a name Rust already holds. Linux keeps `xdg-open`, ignores the two new arguments, and answers
   `{ outcome: opened, openedInName: null, otherEditorsInstalled: null }`.

6. **A removed app: the press still opens the file, in the system default.** Rust checks installed-ness before launching
   (a bundle id through `URLForApplicationWithBundleIdentifier:` AND `is_dir()` on the path it answers, a path through
   `is_dir()`), falls back to `open -t`,
   and reports `chosen_app_missing_opened_default_instead`. The frontend resets the setting to `system` and raises a
   persistent toast naming the app the file DID open in, with "Open settings". Why: the user asked to edit this file and
   the default editor does that; the toast explains the surprise; the reset keeps the next press quiet. The toast names
   the fallback rather than the missing app because, once a bundle is gone, nothing is left to read its name from, and
   there's no table to ask (unlike the terminal). Accepted edge: an app picked from a volume that's unmounted right now
   gets reset too; the toast links straight to the row.

7. **The frontend feature is a new `text-editor/` module under `apps/desktop/src/lib/`, with its own `CLAUDE.md` +
   `DETAILS.md`.** `pane/editor-open.ts` keeps the guard and hands the launch to that module's `openFileInEditor(path)`,
   so the three callers don't change. Why not grow `editor-open.ts`: the guard is a pane question (is there a real file
   behind this row), while the choice, the hint, and the toasts are a settings-and-copy feature shaped like
   `open-terminal/`. `EditorOpenOutcome` gains `'launchFailed'`, so `editor_opened` stays counted only when an app was
   asked to open the file (a fallback open still counts).

8. **The hint.** A persistent toast; everything this feature says goes out under ONE toast id, `text-editor`, dismissed
   first and re-added. It can come from any of the three callers: they share one entry, and "which app opens my files"
   is the same question after ⇧F4 as after F4. The frontend passes `ask_about_other_editors = true` only while
   `behavior.textEditorHintSeen` is false AND the stored choice is `system`. The pure `decideEditorHint({ hintSeen,
   storedChoice, report })`:
   - **Hint already spent**: no hint, no write.
   - **The launch threw** (`launchRefused`, `timedOut`): no hint, the flag stays unspent.
   - **The stored choice isn't `system`**: no hint, and spend the flag silently. They already found the setting, and
     Rust wasn't asked.
   - **`otherEditorsInstalled` is `null`** (Linux, or not asked): no hint, no write.
   - **`otherEditorsInstalled` is `false`**: no hint, and ❗ the flag stays UNSPENT, so someone who installs Sublime Text
     next month is still told.
   - **`otherEditorsInstalled` is `true`**: show the hint (named when `openedInName` is present, the unnamed wording
     otherwise) and spend the flag.

   Why spend-on-show: the terminal's rule (`first-use-pick.ts`), for the same reason. No adoption of a running editor on
   first use: unlike terminals there's a real system default already, and "running right now" says little about which
   editor someone wants for their files.

9. **Cost.** Nothing is cached. The Settings row lists on mount and after every write of its setting
   (`list_text_editors`, 2 s deadline, `TimedOut<TextEditorList>`; a deadline that expires answers empty and the row
   stays disabled at "Checking…", which claims nothing). F4 is one IPC, and the editor query rides on it only while the
   hint is due. `open_in_editor` gets a 5 s deadline, like the terminal launch. M1 reports the list's duration on
   David's Mac; if icon decoding pushes it past roughly 300 ms, say so rather than raising the deadline.

10. **Settings placement: a new "Text editor" card directly above the Terminal card**, holding one row with a bespoke
    `TextEditorSelect.svelte` (the options are live, so it can't be `SettingSelect`). Why its own card: the neighbor is
    titled "Terminal", and single-row cards are already this page's pattern. Off macOS the card doesn't render and the
    setting isn't searchable: `SettingDefinition` gains `macOSOnly?: true` with `SearchableRow.macOSOnly`'s meaning,
    dropped at index build in `buildSearchIndex`. The terminal row takes the same flag and the same `isMacOS()` gate in
    that milestone, which retires its "Checking… forever on Linux" wart for a line or two.

11. **E2E: no new Playwright spec.** The `playwright-e2e` build keeps recording the FILE path into `open_mock` (never the
    argv, never the app), so every `e2e_opened_paths` consumer is untouched. Choice parsing, argv, the fallback, and the
    list assembly are pure Rust tests; the hint table and the side effects are Vitest. What an E2E spec would add is
    "Sublime Text was asked", which `open_mock` can't say without reshaping a store other specs read, for an assertion a
    unit test already makes. ❗ The ⇧F4 round-trip test must spend `behavior.textEditorHintSeen` in its setup
    (`mcpCall('set_setting', …)`, as `open-terminal-here.spec.ts` does for its own flag): on any Mac with Xcode or
    another editor the hint would fire, and the global `afterEach` fails the test over the leaked toast.

12. **`openFileInEditor` never throws.** Every outcome the user should hear about becomes a toast, the same contract as
    `openTerminalHereForFolder`.

## Draft copy (for David's review)

Each key is tagged with the milestone whose code first names it, which is where it lands and gets translated (§ The
copy procedure).

Settings, in `messages/en/settings.json`:

- (M2) `settings.navigationAndFileOps.card.textEditor`: "Text editor"
- (M2) `settings.behavior.textEditorApp.label`: "Edit files in"
- (M2) `settings.behavior.textEditorApp.description`: "Cmdr lists the text editors macOS knows about on this Mac. To use
  a different app, pick "Choose an app…"."
- (M3) `settings.behavior.textEditorApp.systemDefault`: "System default ({app})"
- (M3) `settings.behavior.textEditorApp.systemDefaultUnnamed`: "System default"
- (M3) `settings.behavior.textEditorApp.chooseApp`: "Choose an app…" (the approved design said "Other app…"; this
  matches the Terminal row right below it)
- (M3) `settings.behavior.textEditorApp.chooseAppTitle`: "Choose a text editor"
- (M3) `settings.behavior.textEditorApp.checking`: "Checking your apps…"
- (M3) `settings.behavior.textEditorHintSeen.label`: "Text editor hint shown"
- (M3) `settings.behavior.textEditorHintSeen.description`: "Whether the one-time hint about picking a text editor has
  been shown."

Toasts, in `messages/en/fileExplorer.json` beside `fileExplorer.edit.notOnThisMac`:

- (M3) `fileExplorer.edit.hint`: "Opened in {app}. Want a different editor next time? Pick one in Settings, under
  Navigation & file ops."
- (M3) `fileExplorer.edit.hintUnnamed`: "Opened in your default text editor. Want a different one next time? Pick one in
  Settings, under Navigation & file ops."
- (M2) `fileExplorer.edit.dismiss`: "Dismiss"
- (M2) `fileExplorer.edit.openSettings`: "Open settings"
- (M2) `fileExplorer.edit.appMissing`: "The editor you picked isn't installed anymore, so this file opened in {app}."
- (M2) `fileExplorer.edit.appMissingUnnamed`: "The editor you picked isn't installed anymore, so this file opened in
  your default text editor."
- (M2) `fileExplorer.edit.launchRefused`: "Cmdr couldn't start your text editor. Try opening it yourself once, then
  come back."
- (M2) `fileExplorer.edit.timedOut`: "Your text editor is taking a while to start. It may still open."

ICU values double every apostrophe (`isn''t`, `couldn''t`). `{app}` is an uncontrolled insert (any app's own name): say
so in its `@key` description, and keep it in a slot a translator can restructure around
(`docs/guides/i18n-translation.md` § Write placeholder strings to be restructurable). "Dismiss", "Open settings",
"Choose an app…", and "Checking your apps…" repeat the terminal keys' English on purpose
(`commands.handler.openTerminalHere.*` and `settings.behavior.openTerminalHereApp.*`), so
`desktop-i18n-term-consistency` warns if a locale translates them differently.

## Milestones

### M1. Rust: list, resolve, launch, report

**Scope**

- **Spike first** (timebox about 45 minutes; report the result before building the list). Download the current Sublime
  Text and VS Code macOS archives into the scratchpad and read each `Contents/Info.plist` `CFBundleDocumentTypes`
  (`LSItemContentTypes`, `CFBundleTypeExtensions`, `CFBundleTypeOSTypes`, `CFBundleTypeRole`). ❗ Registering either app
  with `lsregister` writes David's LaunchServices database, so do it ONLY with David's OK relayed through the lead;
  without it, pick between the `public.plain-text` query and Decision 4's union on what the plists declare, and say
  which evidence decided. Also ask, through the lead, whether David will spend a minute confirming that `open -t` and
  the resolved default name agree after changing a `.txt` file's default in Finder; if not, record it as an open item in
  `file_system/DETAILS.md`. Finder's "Change All" writes an all-roles handler, so that check can't tell `kLSRolesAll`
  from `kLSRolesEditor`: note there too that a role-specific override (from `duti`, say) could name one default while
  `open -t` launches another. ❌ Never change David's default handler yourself.
- **Shared helpers**: move `installed_app_path` out of `terminal.rs` into `open_with.rs`'s `mod imp`, exported on its
  `pub use imp::{…}` list, plus an `app_icon_data_url(app_path)` wrapping `load_app_icon` and
  `icons::rgba_to_data_url`. `terminal.rs` calls both. Behavior-identical.
- **The module**, `text_editor.rs` under `file_system/`, declared UNGATED in `file_system/mod.rs` (Decision 2): the wire
  types `EditorOpenReport`, `EditorOpenOutcome`, and `OpenInEditorError` compile everywhere, and the rest sits behind
  `#[cfg(target_os = "macos")]`:
  - `SYSTEM_DEFAULT_CHOICE = "system"` and `TextEditorChoice { SystemDefault, BundleId(String), AppPath(PathBuf) }`.
  - Pure: `parse_choice`, `launch_argv(&TextEditorChoice, file) -> Vec<String>`, `resolve_choice(choice,
    is_installed)`, the list assembly (handler ids + default id + chosen + a resolver → rows), the pick
    canonicalization, and `other_editors_installed`.
  - The two LaunchServices calls behind small `unsafe` wrappers: each with its own `// SAFETY:`, null-checked, wrapped
    with `CFArray::wrap_under_create_rule` / `CFString::wrap_under_create_rule` exactly once.
  - `list_text_editors(setting) -> TextEditorList { defaultAppName, defaultAppIcon, apps: Vec<TextEditorApp { id,
    displayName, icon }>, chosenId }`.
  - `open_in_editor(path, setting, ask) -> Result<EditorOpenReport, OpenInEditorError>`, with `launch` split by cfg:
    spawn `open`, or `crate::open_mock::record(<file>)` under `playwright-e2e`.
- **Commands** in `commands/file_actions.rs`, registered in `ipc.rs` beside the terminal's:
  - `open_in_editor` on macOS becomes ONE `async` arm, `#[cfg(target_os = "macos")]` with no feature condition, through
    `blocking_typed_result_with_timeout` (5 s; the join-failure mapper logs and answers `TimedOut`, as
    `open_terminal_here` does) into `text_editor::open_in_editor`, whose own `launch` records under `playwright-e2e`.
  - Off macOS the arms stay sync, with the new signature and the plain report: `playwright-e2e` records (the Linux
    Docker lane runs `file-operations.spec.ts` and still asserts the ⇧F4 path), Linux spawns `xdg-open`, anything else
    answers `launchRefused { errno: null }`. ❗ Today's `playwright-e2e` arm has no OS condition: add
    `not(target_os = "macos")` to it, or the macOS E2E build defines `open_in_editor` twice.
  - New macOS-only `list_text_editors(app_choice) -> TimedOut<TextEditorList>` (2 s, `blocking_with_timeout_flag`).
- **Frontend plumbing only**: `pnpm bindings:regen`; `openInEditor(path, appChoice, askAboutOtherEditors):
  Promise<EditorOpenReport>` throwing an `OpenInEditorFailure` (`TypedFailure`) plus `asOpenInEditorError`, and
  `listTextEditors(appChoice)`. `editor-open.ts` calls `openInEditor(rowPath, 'system', false)` with a comment that M2
  replaces the literal.
- **Docs**: `file_system/DETAILS.md` gets a § "Text editor (`text_editor.rs`)" (the Evidence section trimmed, with its
  anchor; why the role query; the Create rule; the pick canonicalization; the `open -t` verification status) and a
  module-map line. `file_system/CLAUDE.md` gets one guardrail: text editors come from the LaunchServices editor-ROLE C
  query, ❌ never the UTI crate (10.15 floor) or a made-up URL (answers nothing). Also `commands/DETAILS.md`'s
  `file_actions.rs` bullet, the `open_mock.rs` module comment, a backend line in `docs/architecture.md` beside
  `file_system/terminal.rs`, and `tauri-commands/DETAILS.md` if its inventory lists `file-actions.ts` commands.

**Intentions**: F4 behaves byte-identically after M1, because every caller still passes `system`. The Rust surface is
complete, so M2 and M3 are frontend-only.

**Landmines**

- ❌ The UTI crate, the content-type selectors, and made-up URLs (loud rules).
- ❗ Both LaunchServices functions return +1 objects. A get-rule wrap leaks; wrapping twice over-releases and crashes
  later somewhere unrelated.
- `LSCopyAllRoleHandlersForContentType` returns NULL when nothing claims the type: an empty list, not an error.
- The system default can be absent (TextEdit deleted and nothing else claims plain text): its name is `None`, and
  `open -t` does whatever it does today.
- `core-services` 1.0.0 marks neither function `#[deprecated]` (Evidence), so clippy stays quiet and no
  `#[allow(deprecated)]` belongs here. If a later bump adds the attribute, allow it on the call with a reason naming the
  10.15 floor, ❌ never module-wide.
- A sync command must not grow a LaunchServices call (`commands/CLAUDE.md`), and every refusal is a typed variant, ❌
  never a message the frontend matches (`error-string-match`).
- The blocking pool is fine for these calls (the terminal does the same); ❌ never rayon.
- Warp twice, Xcode and LibreOffice as "editors": expected, macOS says so. ❌ No filter table.
- The `terminal.rs` extraction stays behavior-identical; its tests pass unchanged.
- `desktop-bindings-fresh` stays red until bindings are regenerated.

**Test plan** (red, then green; in `text_editor.rs`'s `mod tests` unless noted)

1. `parse_choice`: `system` and `""` read as `SystemDefault`, `/Applications/Sublime Text.app` as `AppPath`,
   `com.sublimetext.4` as `BundleId`. Stub it to always answer `SystemDefault`, watch the path and bundle-id asserts
   fail, then implement.
2. `launch_argv`: `open -t <file>`, `open -b <id> <file>`, `open -a <app> <file>`; a file path with spaces, quotes, `&`,
   and non-ASCII travels verbatim as the last argument. Red on a stub that answers `open -t` for every choice.
3. `resolve_choice`: an installed choice is used as is with `Opened`; a missing bundle id and a missing path both fall
   back to `SystemDefault` with `ChosenAppMissingOpenedDefaultInstead`; `SystemDefault` is never "missing", even when
   the installed check says no (it's never asked).
4. List assembly: drops ids that don't resolve or resolve to a missing bundle, removes the default's id, appends a
   chosen bundle id or path that isn't listed, never duplicates a chosen id that is (a stored path whose canonical form
   is a listed bundle id included), keeps LaunchServices' order; `chosenId` is `system` for the default and `None` for
   a missing app.
5. Pick canonicalization: bundle id when it resolves to the same bundle, a symlinked spelling of that path included;
   the path when it resolves elsewhere or there's no bundle id.
6. `other_editors_installed`: empty and default-only answer false; one more app answers true.
7. macOS-only smoke test: the plain-text default's bundle id is `Some` (TextEdit ships with macOS). Break the CF
   wrapper to return `None`, see it fail, restore.
8. `open_with.rs`: `installed_app_path("com.apple.TextEdit")` is `Some` on macOS (moves with the helper, or is added).
9. Frontend: tests for the `openInEditor` and `listTextEditors` wrappers (typed failure round trip), because
   `svelte-tests` holds every file to a 70% coverage floor. Update the exact-argument expectations in
   `command-dispatch.characterization.test.ts` and `pane/search-pane-keys.test.ts` to `(path, 'system', false)`
   deliberately: the call changed on purpose.

**DONE**: `pnpm check clippy rust-tests desktop-bindings-fresh desktop-rust-macos-availability
desktop-macos-framework-floor` green (the floor check needs a built binary; if it skips, say so) plus `pnpm check
svelte` for the wrapper and call site. The spike's result and the list timing are in the report. Commit, for example
`feat(editor): Cmdr can list the text editors macOS knows and launch a file in a chosen one, groundwork for picking what F4 opens`.

### The copy procedure (M2 and M3 each run it)

A key can't land ahead of its code: `desktop-message-keys-unused` (an error, fast lane) fails on an `en` key nothing
references, and `desktop-i18n-coverage` (an error) fails on one left in English. So each frontend milestone adds the
Draft copy keys tagged with its number, writes the code that names them, and translates them, all before its commit.

**Steps**

- Add the milestone's `en` keys, each with a `@key` description (surface, trigger, what `{app}` holds; for
  `textEditorApp.description` and `textEditorApp.chooseApp`, that "Choose an app…" must match
  `settings.behavior.openTerminalHereApp.chooseApp`). Run `pnpm intl:keys`, then
  `node apps/desktop/scripts/sync-locale-keys.ts`.
- Translate into the ten full locales per `docs/guides/i18n-translation.md` § "New feature → add strings and translate
  to ALL languages": each locale's style guide and glossary, the reference pile at the MAIN clone's absolute path, the
  reusable translator block, at most three translator subagents at a time (spawned without `name` when you're a subagent
  yourself). Record glossary entries per locale: "text editor" in M2, "system default" in M3.
- Overlays (`en-GB`, `en-AU`): nothing to fork unless evidence says otherwise; "editor", "installed", and "default" are
  spelled the same.

**Landmines**

- The reference pile is NOT in the worktree: `~/projects-git/vdavid/cmdr/_ignored/i18n/<tag>/`.
- Apostrophes are doubled in ICU values and normal in `@key` descriptions.
- Translate "text editor" as the generic noun. TextEdit's own name is an app name that `{app}` carries; don't confuse
  the two.
- `desktop-i18n-term-consistency` (a warn): "Dismiss" and "Open settings" (M2), "Choose an app…" and "Checking your
  apps…" (M3) should translate exactly as the terminal keys do in each locale.
- ❌ Don't edit the existing terminal keys.
- ❌ Never add a key before the code naming it, and never widen `unusedKeyDynamicPrefixes` to get one through.

**Red step**: run `pnpm check desktop-i18n-coverage` right after `sync-locale-keys` and see it fail on the English
skeletons; translate; see it pass.

**Checks**, inside the milestone's DONE: `pnpm check desktop-i18n-parity desktop-i18n-icu desktop-i18n-plural
desktop-i18n-stale desktop-i18n-coverage desktop-i18n-dont-translate desktop-i18n-term-consistency
desktop-i18n-aria-label desktop-i18n-doc-citations desktop-message-keys-unused`, warns reported.

### M2. F4 honors the choice

**Scope**

- **Registry**: `behavior.textEditorApp` in `settings/definitions/behavior.ts` beside the terminal entries (`type:
  'string'`, `default: 'system'`, `component: 'select'`, `cardKey: 'settings.navigationAndFileOps.card.textEditor'`,
  keywords such as `editor`, `text editor`, `F4`, `Sublime Text`, `VS Code`, `BBEdit`, `TextEdit`), with a comment in
  the terminal's style, and its `SettingsValues` key in `settings/types.ts`. No row until M3; until then the entry is
  searchable with nothing rendered, which is fine inside the branch. ❌ Never add it to `CATEGORICAL_STRING_KEYS`: the
  value can be a path inside someone's home folder. Commit the `settings-defaults.gen.json` rewrite that
  `analytics-settings-defaults` makes.
- **Copy**: § The copy procedure, for the keys tagged (M2).
- **The module**, `text-editor/` under `apps/desktop/src/lib/`:
  - `text-editor-choice.ts`: a leaf with zero imports, `SYSTEM_DEFAULT_EDITOR_CHOICE = 'system'` (mirrors Rust's
    `SYSTEM_DEFAULT_CHOICE`).
  - `text-editor-setting.ts`: `getTextEditorChoice()` (a missing or non-string value reads as `system`),
    `setTextEditorChoice`, and `openSettingsToTextEditor()` (surface `'text-editor-toast'`, section `['Behavior',
    'Navigation & file ops']`, anchor `settingAnchorId('behavior.textEditorApp')`, which starts resolving in M3; until
    then the section opens at its top).
  - `open-file-in-editor.ts`: `openFileInEditor(path): Promise<boolean>`. Reads the choice, calls `openInEditor(path,
    choice, false)` (the hint arrives in M3), resets to `system` and raises the missing-app toast on
    `chosen_app_missing_opened_default_instead`, words `launchRefused` and `timedOut`, never throws, and says everything
    under the one id with dismiss-then-add.
  - `TextEditorToastContent.svelte`: one presentational body (a resolved `message`, an optional Dismiss, and "Open
    settings"), used by the missing-app toast now and the hint in M3. `$lib/nudges/NudgeToastContent.svelte` is title +
    body + note + decline/accept, which doesn't fit a one-sentence message, so a small body of its own is expected.
  - `CLAUDE.md` (module map; must-knows: the guard lives in the pane, spend-on-show, ask only while due, one toast id,
    the toast names the fallback) and `DETAILS.md` (the hint table, the removed-app decision, the toasts, testing).
- `settings/settings-window.ts`: `'text-editor-toast'` joins `SettingsSurface`.
- `pane/editor-open.ts`: delegates to `openFileInEditor`; `EditorOpenOutcome` gains `'launchFailed'`; its header comment
  stops naming `open -t` as the whole story.
- **Docs**: the F4 bullet in `pane/DETAILS.md` (the guard stays here, the app comes from `text-editor/`), step 4 of
  `file-operations/mkfile/DETAILS.md` (it says `onOpenInEditor` "calls `openInEditor`", which was already one hop
  stale), and a frontend map line for `text-editor/` in `docs/architecture.md`.

**Intentions**: a choice set through MCP's `set_setting` or `settings.json` now drives F4 everywhere, fallback included;
nothing in the UI advertises it yet.

**Landmines**

- ❗ One toast id, dismiss then add, and both toasts persistent (`open-terminal/DETAILS.md` § The toasts).
- Word the missing-app toast from the REPORT's `openedInName`; don't re-read the setting after resetting it.
- `import-cycles`: keep `text-editor-choice.ts` import-free, so `settings/sections/` can import it in M3 without a cycle
  through `$lib/settings`.
- The three callers stay untouched. `command-dispatch.characterization.test.ts` pins `openInEditor`'s arguments; its
  settings mock answers `100` for every key, which `getTextEditorChoice` reads as `system`, so it keeps expecting
  `(path, 'system', false)`. ❗ That mock and `pane/search-pane-keys.test.ts`'s spy both resolve `undefined`, and
  `openFileInEditor` now reads the report: have them resolve `{ outcome: 'opened', openedInName: null,
  otherEditorsInstalled: null }`. `search-pane-keys.test.ts` also reaches `$lib/settings` unmocked from here on; mock it
  there if the real store doesn't load.
- `svelte-tests` holds each new `.ts` file to 70% coverage.
- Every string goes through `tString` (`cmdr/no-raw-user-facing-string`).
- ❌ Don't touch `DualPaneExplorer.svelte` or `FilePane.svelte`.

**Test plan** (Vitest; red, then green)

1. `open-file-in-editor.test.ts`, mocking `$lib/tauri-commands`, `$lib/settings`, and `$lib/ui/toast`: a stored
   `com.sublimetext.4` reaches `openInEditor` as `appChoice`, and a missing or non-string value sends `system`. Red on a
   stub that hardcodes `system`.
2. Same file: `chosen_app_missing_opened_default_instead` with `openedInName: 'TextEdit'` writes `system`, calls
   `dismissToast('text-editor')` before `addToast`, and uses the named message; `openedInName: null` uses the unnamed
   one. Red first, with no reset.
3. Same file: an `OpenInEditorFailure` of `timedOut` raises the timed-out message, `launchRefused` the launch-refused
   one; the function resolves `false` and never rejects.
4. `editor-open.test.ts` (new): the guard still refuses an `adb://…` row and an archive-inner row without calling
   `openFileInEditor`; a real row maps `true` to `'opened'` and `false` to `'launchFailed'`.
5. `text-editor-toasts.a11y.test.ts`, mirroring `open-terminal-toasts.a11y.test.ts`.
6. The copy: the procedure's red step on `desktop-i18n-coverage`.

**DONE**: the copy procedure's checks, `pnpm check svelte`, then `pnpm check docs-reachable docs-dead-links
docs-link-text claude-md-length`, then plain `pnpm check`. Commit, for example
`feat(editor): F4 opens files in the editor you chose, and falls back to the system default with a word when that app is gone`.

### M3. The Settings row and the one-time hint

**Scope**

- **Platform flag**: `SettingDefinition.macOSOnly?: true` in `settings/types.ts`; `settings-search.ts::buildSearchIndex`
  drops `macOSOnly` settings off macOS, read at index build the way `searchableRowEntries` does. Set it on
  `behavior.textEditorApp`, `behavior.textEditorHintSeen`, and `behavior.openTerminalHereApp`.
- **Registry**: hidden `behavior.textEditorHintSeen` (boolean, default false) and its `SettingsValues` key; commit the
  `settings-defaults.gen.json` rewrite that `analytics-settings-defaults` makes.
- **Copy**: § The copy procedure, for the keys tagged (M3).
- **Options**, `settings/sections/text-editor-options.ts` (pure): the system-default row first (named or unnamed label,
  the default's icon), the apps sorted with an `Intl.Collator` in the app's locale, then the choose-app sentinel; and
  `selectedTextEditorId(list)`, where a `null` `chosenId` DISPLAYS `system` and never writes.
- **The row**, `settings/sections/TextEditorSelect.svelte`: lists on mount and on `onSpecificSettingChange`, stays
  disabled at "Checking…" while empty, and "Choose an app…" opens `@tauri-apps/plugin-dialog`'s `open()` (filter `app`,
  default `/Applications`), then stores `(await listTextEditors(picked)).data.chosenId ?? picked`. ❗
  `TerminalAppSelect.svelte` is the same shell: extract the shared part (the `Select`, the checking state, the picker,
  the refresh on change) into one component both rows use, rather than copying it (`jscpd-frontend`). Each row keeps its
  own `*-options.ts`.
- **The page**: in `NavigationAndFileOpsSection.svelte`, the "Text editor" card sits above Terminal, both cards behind
  `isMacOS()` plus `anyVisible`; update the header comment's card list.
- **The hint**: `text-editor/editor-hint.ts` (pure `decideEditorHint`, Decision 8), wired into `openFileInEditor`, which
  now asks Rust only while the hint is due; the hint toast uses `TextEditorToastContent` with Dismiss.
- **E2E**: the "New file round-trip" test in `file-operations.spec.ts` spends `behavior.textEditorHintSeen` before its
  dispatch, with a comment saying why. Grep the suite for any other path that reaches `file.edit` or confirms a new
  file, and do the same there.
- **Docs**: a § for the row in `settings/sections/DETAILS.md` (mirroring the terminal's, pointing at
  `text-editor/DETAILS.md` for the hint); the `macOSOnly` flag on settings in `settings/DETAILS.md`, next to the
  searchable-row one; a step-1 bullet in `docs/guides/adding-a-new-setting.md`; delete the "One surface is NOT gated"
  paragraph from `open-terminal/DETAILS.md`; the hint table in `text-editor/DETAILS.md`.

**Intentions**: the feature is whole. People find the row, the hint teaches it once, and nothing of it shows on Linux.

**Landmines**

- ❗ The global `afterEach` toast guard (Decision 11).
- The hint must never fire on Linux: `otherEditorsInstalled` is `null` there, and the table's `null` row is the
  guarantee, so test that row.
- Off macOS a search hit on a `macOSOnly` setting would open a page with nothing to show; `searchable-rows.test.ts` pins
  the row version, so add the setting version.
- ❌ Never write the setting while browsing: the missing-app display is display only.
- The choose-app sentinel must be neither `system`, nor a path, nor bundle-id-shaped (the terminal's `__choose_app__`
  qualifies; share it through the extracted component).
- `NavigationAndFileOpsSection.svelte.test.ts`: if it mocks `listTerminalApps`, mock `listTextEditors` the same way.
- `hidden` settings still enter search by design, which is why the hint flag takes `macOSOnly` too.

**Test plan** (red, then green)

1. `editor-hint.test.ts`: one test per Decision 8 bullet. Red on a stub answering "no hint, no write": the `true` row and
   the non-`system` row fail.
2. `open-file-in-editor.test.ts`: `askAboutOtherEditors` is true only while unspent AND `system`; the hint is raised
   with `{ app }` and the flag written; `false` leaves the flag alone; a non-`system` choice spends it without asking
   Rust.
3. `text-editor-options.test.ts`: the system row first (named and unnamed), apps sorted by the locale collator (use a
   pair where byte order and collation disagree, like "Ölmaker" and "Zed" under `de`), choose-app last; a `null`
   `chosenId` selects `system`.
4. Settings search: with `isMacOS` false the three `macOSOnly` settings are gone from results; true brings them back.
   Call `clearSearchIndex()` between the two: `buildSearchIndex` memoizes, so without it the second answer is the
   first one's.
5. `NavigationAndFileOpsSection.svelte.test.ts`: off macOS neither card renders; on macOS the Text editor card sits above
   Terminal.
6. The terminal row's existing tests stay green through the extraction.
7. The copy: the procedure's red step on `desktop-i18n-coverage`.

**DONE**: the copy procedure's checks, then plain `pnpm check` green; the "New file round-trip" test run alone per the single-spec recipe in
`apps/desktop/test/e2e-playwright/CLAUDE.md`, green; then `pnpm check --include-slow` once. Commit, for example
`feat(settings): pick the app F4 opens files in from the editors macOS lists, and hear once where that setting lives`.

## Out of scope

- Per-extension editors (Markdown in one app, JSON in another).
- Terminal editors (vim, Helix, nano) and any command-template custom option.
- Jump-to-line from content search (`subl file:42`, `code -g file:42`).
- Editing files that aren't on this Mac (phones, servers, archive insides): the guard's refusal stays.
- Linux: `xdg-open` unchanged, no row.
- Changing macOS's own default handler from Cmdr.
- Renaming the palette's "Edit in default editor" or the menu's "Edit in editor".
- Analytics properties on `editor_opened`.
- Adopting a running editor on first use.

## Invariants

- F4 with the setting at `system` runs `open -t <file>`, exactly as before this plan.
- The pane guard runs before any launch; the setting never widens what F4 accepts.
- Rust decides which app launches and whether it's installed; the frontend owns the stored value and every word shown.
  ❌ No decision crosses IPC as a sentence.
- A removed chosen app never loses the press: the file opens in the system default, the user hears it once, the setting
  resets.
- The hint flag is spent only when the hint shows, or when a non-default choice proves the setting was already found.
- The editor query runs only when the Settings row renders or when an F4 lands while the hint is due.
- No framework newer than macOS 10.15 enters the binary, and no selector newer than 10.15 is called ungated.
- The `playwright-e2e` build launches nothing and records the file path only.
- Everything the feature says goes out under one toast id, dismissed before it's re-added.
- Nothing from this feature renders or matches search off macOS.
- Every key lands in the same commit as the code naming it, translated into all ten full locales.
