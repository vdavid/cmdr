# Accessibility audit, 2026-10 (VPAT 2.5Rev INT evidence)

The criterion-by-criterion evidence behind Cmdr's accessibility conformance report. It follows the ITI VPAT 2.5Rev INT
edition (April 2025): WCAG 2.2 Level A and AA (applied to software through W3C's WCAG2ICT), Revised Section 508
(chapters 3, 5, and 6), and EN 301 549 (clauses 4, 5, 11, and 12). The criteria list and wording come from the
downloaded INT `.docx` (itic.org, `VPAT2.5RevINT`).

## Scope and method

- **Product**: Cmdr desktop app for macOS (`apps/desktop/`), Svelte 5 frontend in a Tauri 2 WKWebView, at `main`
  `f31f2aec5` (2026-10-14). The app is "open functionality software": it exposes its UI through WebKit's bridge to the
  macOS accessibility API (NSAccessibility), so VoiceOver and other assistive tech can reach it. Documentation and
  support: getcmdr.com (`apps/website/`), the in-app Keyboard shortcuts window, email, and in-app feedback.
- **Methods**: code reading of every surface (file panes, dialogs, toasts, forms, settings, search, viewer, queue);
  contrast math on the theme tokens in light and dark mode under all nine accent variants; the check lanes
  `a11y-contrast`, `a11y-coverage`, and `svelte-tests` (all green on 2026-10-14, `pnpm check -m`, 3m36s).
- **Automated coverage that exists**: 110 `*.a11y.test.ts` files with about 920 axe-core test cases (tags `wcag2a`,
  `wcag2aa`, `wcag21a`, `wcag21aa`, `wcag22aa`, `best-practice`; `apps/desktop/src/lib/test-a11y.ts`); the
  `a11y-coverage` gate (every `lib/**/*.svelte` has a test or a reasoned allowlist entry); the deterministic contrast
  checker `scripts/check-a11y-contrast/` (WCAG 2.2 plus an APCA Lc 45 floor, accent matrix, row-state matrix); and 11
  E2E axe tests in light and dark mode (12 since #442 added the queue window, which also gained a tier-3 file,
  `routes/queue/queue.a11y.test.ts`) (`apps/desktop/test/e2e-playwright/accessibility.spec.ts`, `color-contrast` off
  there because the static checker owns it). `docs/testing.md` still says 696 tests; the count above is from the files.
- **Screen reader**: every check in David's VoiceOver guide (V1–V12, vault,
  `projects/Cmdr/2026-10-14 Cmdr VoiceOver test guide.md`) heard passing. Sign-off: David, by ear, 2026-10-15, on `main`
  at `8feba4c02` or later, macOS 27.0, VoiceOver 10 ("VoiceOver done, it works fine"). Two earlier agent-driven
  listening runs the same day captured the verbatim phrases (§ VoiceOver listening runs). Each row names the checks that
  confirm it. Unit, axe, and `@guidepup/virtual-screen-reader` tests cover the code too; they model the ARIA spec, not
  WebKit or VoiceOver. Live UI wasn't judged from screenshots.
- **Levels**: Supports, Partially supports, Does not support, Not applicable (the VPAT's own terms). "Partially
  supports" means some functionality doesn't meet the criterion.
- **Re-evaluations**: the report is re-checked against each released version, every two months or so (§ How to
  re-evaluate). Latest: 0.57.0 on 2026-10-30 (§ Re-evaluation 2026-10-30). Rows below describe 0.57.0; evidence paths
  are relative to `apps/desktop/src/lib/` unless they say otherwise.

## Summary

Counts per table (rows marked "see WCAG section" aren't counted):

- **WCAG 2.2 Level A (32)**: 24 Supports, 2 Partially supports, 0 Does not support, 6 Not applicable.
- **WCAG 2.2 Level AA (24)**: 14 Supports, 5 Partially supports, 0 Does not support, 5 Not applicable.
- **Section 508 chapter 3 (9)**: 6 Supports, 3 Partially supports.
- **Section 508 chapter 5 (26)**: 13 Supports, 3 Partially supports, 10 Not applicable.
- **Section 508 chapter 6 (5)**: 4 Supports, 1 Not applicable.
- **EN 301 549 clause 4 (11)**: 8 Supports, 3 Partially supports.
- **EN 301 549 clause 5 (11)**: 2 Supports, 9 Not applicable.
- **EN 301 549 clause 11 (19)**: 12 Supports, 3 Partially supports, 4 Not applicable.
- **EN 301 549 clause 12 (5)**: 5 Supports.
- **No row waits on a screen-reader test.** The rows the code review found supported (1.3.2, 3.3.1, 4.1.3, 502.3.2,
  502.3.4–502.3.6, 502.3.8, 502.3.10–502.3.12, 502.3.14, and their clause 11 mirrors) were confirmed by V4–V9, V11, and
  V12 on 2026-10-15; the report shows them as Supports. No VoiceOver check lowered a row.
- **Whole chapters/clauses not applicable**: 508 chapter 4 (hardware); EN 301 549 clauses 6 (two-way voice), 7 (video),
  8 (hardware), 9 (web: the app isn't web content; the website is assessed as documentation under 12.1.2), 10 (non-web
  documents: none ship), and 13 (relay and emergency services). WCAG AAA (Table 3) wasn't evaluated: none of the three
  standards requires it.

**Risk of downgrade**: low. V1–V12 passed by ear on `main` (the silent-list failure the first agent run found is fixed
and re-heard). What's unheard is outside the guide: C3 (the VoiceOver cursor moving Cmdr's cursor), already counted as a
gap in 502.3.13. Since 0.53.0 the code under V1–V6, V9, and V11 changed (§ Re-hear, 2026-10-30); none of those changes
contradicts a row, but they're unheard.

Top gaps, each with its issue:

1. **The file list was silent to VoiceOver** (#437, fixed and heard 2026-10-15): focus sits on the focused pane's
   listbox, whose `aria-activedescendant` names the cursor row by a document-unique id; rows carry kind, position,
   marked state, and in Full view size and date; a per-pane live region says the pane and folder; ⌃F5 reaches the pane
   chrome. The inline rename field moved out of its option (#449, `a7b318620e`, released in 0.57.0). Left: Full view
   isn't a table (#451), and the VoiceOver cursor moving onto a row (C3) is unheard (#453).
2. **Cursors and status changes aren't announced** (#438, fixed 2026-10-15, heard in V4–V7): the Search result cursor,
   the Settings sidebar, empty folders, selection counts, the license key error, progress text.
3. **Toasts are mouse-first** (#439, fixed 2026-10-15, heard in V9): focus didn't pause them, no key reached them, no
   setting kept them, and Undo trash existed only in a 10-second toast.
4. **State cues rely on faint color** (#440, mostly closed in code 2026-10-15): marked rows now carry a leading-edge bar
   (4.51:1 worst) and a text stroke, and the panes differ by an accent vs gray cursor hairline plus a full vs faint tab
   accent band, all by default (David's design picks). Field borders clear 3:1; Increase contrast and Differentiate
   without color dash the unfocused hairline and lift both hairlines to 3:1. Left: by default both hairlines are solid
   and subtle (1.09:1 accent, 1.41:1 gray worst), so the default focused-pane cue is hue, fill, and the tab band,
   deliberately below 3:1 and with no shape cue until the user asks macOS for one (David's pick), hidden files differ by
   color alone (#452), and the default focus ring is under 3:1 with yellow, orange, green, or Cmdr gold in light mode
   (kept on purpose: VoiceOver draws its own ring; the ink ring is the Increase contrast tier). The default-contrast
   items are #454.
5. **Pointer and keyboard leftovers** (#441, closed in code 2026-10-15; #817, 2026-10-31): tab reorder was drag-only,
   sub-24px targets, tooltips not hoverable, viewer bare-key shortcuts not remappable, zoom topped out at 150%.
6. **Quick fixes** (#442, closed in code 2026-10-15): Reduce motion misses, hard-coded English labels and window titles,
   a missing `lang`, a missing `autocomplete`, badges without text, no queue window a11y test.
7. **No accessibility documentation** (#443, drafted 2026-10-15 at `/trust/accessibility-guide`, awaiting David's
   review): nothing lists the features, limits, or how to get help.
8. **In-app menus under VoiceOver** (#267, fixed and heard 2026-10-15): submenu rows have ids, the entered submenu takes
   focus with its own `aria-activedescendant`, and the volume chooser is a menu button. All four house menus and the
   native viewer right-click menu read their rows.
9. **Label in name** (#450, fixed in `d565214b8a`, released in 0.57.0): seven controls' accessible names didn't contain
   their visible text (found once the rule ran for real, 2026-10-15). No `skipLabelInName` opt-out is left.
10. **Controls nested in controls** (#824, found 2026-10-30, fixed in code 2026-10-31, after 0.57.0): the Settings
    shortcut pill's remove × is now a sibling button in the tab order, Search's `Custom…` size and date fields sit in a
    label where the cell's button was, and Search result path pills are `aria-hidden` spans with the path as `sr-only`
    text. No `nested-interactive` opt-out or skip is left. The first audit missed these at 0.53.0.

Strengths a buyer can rely on: everything core runs from the keyboard (every action is a command with a rebindable
shortcut and a command palette entry); all dialogs share one primitive with correct roles, focus trap, Escape, and focus
restore; text contrast is gated in CI across both themes and every macOS accent; macOS text size, dark mode, accent
color, Reduce motion, Reduce transparency, Increase contrast, and Differentiate without color are honored; no audio,
video, flashing, or time limits beyond toasts; native menu bar and context menus.

## WCAG 2.2 Level A

- **1.1.1 Non-text content**: Supports. Icon buttons are named, the viewer image uses the file name as alt text, and the
  sync and symlink badges name themselves into the row's accessible name (#442). File-kind icons stay `alt=""`, and the
  row's name says the kind instead ("src (folder)", #437, 2026-10-15). `Icon` is `aria-hidden` unless given a label.
  Evidence: `lib/file-explorer/selection/FileIcon.svelte`, `views/file-row-a11y.ts` (`fileRowName`),
  `lib/ui/Icon.svelte`, `routes/viewer/MediaImageView.svelte` (`alt={fileName}`); spot checks of `ModalDialog.svelte`,
  `TabBar.svelte`, `SettingRow.svelte` aria-labels. V1 heard: "c.zip (archive) (5 of 8)", "d-link (symlink) (6 of 8)".
  The Markdown view (0.57.0) keeps each `<img>`'s own `alt`, and a held-back or missing picture's placeholder names it
  by that `alt` (`routes/viewer/markdown-view.ts` `wireImages`).
- **1.2.1 Audio-only and video-only (prerecorded)**: Not applicable. Cmdr has no audio or video content; the viewer
  shows text, images, PDFs, and formatted Markdown only. Evidence: no `<audio>`/`<video>` in `apps/desktop/src`
  (0.57.0); `routes/viewer/media-view.ts` (`isMediaKind`).
- **1.2.2 Captions (prerecorded)**: Not applicable. Same as 1.2.1.
- **1.2.3 Audio description or media alternative (prerecorded)**: Not applicable. Same as 1.2.1.
- **1.3.1 Info and relationships**: Partially supports. Dialogs, settings, and server forms use real headings, labels,
  and roles. The Settings sidebar nests each section's subsections in a `role="group"` named for it (#438). Since #437
  (2026-10-15) the file list is `aria-multiselectable` with `aria-selected` = marked, every option (`..` included)
  carries `aria-posinset`/`aria-setsize`, and the sort buttons' names say the sort state ("Size, sorted descending").
  Left: Full view is a listbox, not a grid, by decision; each row's name carries its size and date, so the column
  headers aren't tied to cells. Evidence: `views/file-row-a11y.ts`, `views/BriefList.svelte`, `views/FullList.svelte`,
  `views/FullListHeader.svelte`, `selection/SortableHeader.svelte` (`fileExplorer.columns.sortState`),
  `settings/components/SettingsSidebar.svelte`, `file-list.vsr.svelte.test.ts`. V1, V2, and V7 heard. #451.
- **1.3.2 Meaningful sequence**: Supports. DOM order matches the visual order in every surface; virtualized rows render
  in list order. Evidence: code reading of panes, dialogs, settings. V12 heard.
- **1.3.3 Sensory characteristics**: Supports. Instructions don't rely on shape, position, or sound alone; pane regions
  are named "Left file pane" and "Right file pane". Evidence: `pane/FilePane.svelte` region label; English catalog
  review.
- **1.4.1 Use of color**: Partially supports. Git status uses letters, errors use labeled glyphs, and size/age colors
  are decorative. Since #440 (2026-10-15), by default: marked rows carry a 3px leading-edge bar in the selection red
  (4.51:1 worst against the row), rounded at each run's ends, plus a 0.4px text stroke; the focused pane's active tab
  has the full accent band where the other has it at half strength. The focused-pane cursor cue is color by default: the
  unfocused pane's hairline is solid gray (the control-border tone at 35%) where the focused one is solid accent. Under
  Differentiate without color or Increase contrast the unfocused hairline turns dashed, adding a shape cue. Hidden files
  still differ only by a dimmer color. Evidence: `app.css` `--color-selection-fg-primary`,
  `--color-cursor-outline-unfocused`, `--color-tab-band*`; `app-file-list.css` marked-row and cursor rules;
  `views/mark-run.ts`; `tabs/TabBar.svelte`; `--color-text-quiet`; `git/status-column.ts`. #452, #454.
- **1.4.2 Audio control**: Not applicable. Cmdr plays no audio.
- **2.1.1 Keyboard**: Supports (#824 closed the one gap, 2026-10-31). The shortcut pill's × is a tabbable sibling
  button, shown while the pill or the × has focus, and removing with it hands focus to the row's `+`
  (`settings/sections/ShortcutPill.svelte`). Search result path pills stay mouse-only; the keyboard reaches a result's
  folder by opening the result, then going up. Every core action is a command with a shortcut and a command palette
  entry: pane switch, navigation, marking, open, go up, go to path, volume chooser, tabs, sort, divider, view mode, all
  file operations, settings. "Move focus to the notifications" (⌃F6) reaches every toast's buttons, and Undo trash is
  also Edit > Undo move to trash (#439, 2026-10-15). Tabs reorder with `tab.moveLeft` / `tab.moveRight` (⌘⌃⇧← / →) and
  cross with `tab.moveToOtherPane` (palette), the drag's rules through `moveTabToPane` (#441, 2026-10-15). Evidence:
  `lib/commands/sources/*.ts`, `pane/tab-operations.ts` (`moveActiveTab`), `ui/toast/toast-focus.ts`,
  `delete/trash-undo.ts` (`undoLastTrash`). Since 0.53.0: Tab and ⇧Tab walk the notifications after ⌃F6 and wrap until
  Escape (`16d818a36f`), Escape cancels a running transfer and focus opens on Pause (`bf6a0f3bcd`), the queue reorders
  with ⌥↑ / ⌥↓ beside its new drag (`67edbd06ac`), and every date sort is in the header's Sort by menu (`98ab349179`).
- **2.1.2 No keyboard trap**: Supports. Dialog focus traps always release on Escape or their buttons; nested traps
  stack. Evidence: `lib/ui/focus-trap.ts`, `ModalDialog.svelte`, lint `cmdr/dialog-needs-focus-trap`.
- **2.1.4 Character key shortcuts**: Supports. File-list single keys (`*`, `+`, `-`) are rebindable and active only with
  list focus; type-to-jump only acts with pane focus. The viewer's single-key shortcuts (`W`, `F`, `0`–`3`, `+`/`=`/`-`,
  Enter / Space on an image) work window-wide and, since #441 (2026-10-15), are registry commands (scope
  `Viewer window`) the user can remap or turn off in Settings; the viewer reads rebinds through the read-only
  `get_restricted_window_shortcuts`. Evidence: `shortcuts/shortcuts-store.ts`, `pane/type-to-jump-keys.ts`,
  `commands/sources/viewer-window.ts`, `routes/viewer/viewer-keyboard.ts`, `viewer-view-mode.ts`,
  `MediaImageView.svelte`, `viewer-keys.remap.test.ts`. Decision prompts' letter keys (S Skip, O Overwrite, and so on)
  act only while their dialog holds focus, after an arming delay, and the queue window's row letters are registry
  commands (`file-operations/decision-keys.ts`, `commands/sources/queue-window.ts`). 0.57.0 adds no bare-key default
  (the new defaults are ⌘N, ⌘[ / ⌘], ⌘⌥X, ⇧↑ / ⇧↓, ⌥↑ / ⌥↓).
- **2.2.1 Timing adjustable**: Supports. The only time limits are toasts (4 s default, 7–10 s for some). Hover and
  keyboard focus hold them, and "Keep notifications until I close them" turns the timers off (#439, 2026-10-15). The
  quit-while-busy 15 s countdown falls under the essential exception (macOS logout deadline). Evidence:
  `ui/toast/ToastItem.svelte` (`updateHold`, `ToastItem.focus.test.ts`), `ToastContainer.svelte`
  (`behavior.notifications.keepUntilClosed`), `lib/quit/QuitConfirmationDialog.svelte`.
- **2.2.2 Pause, stop, hide**: Supports. Moving content is limited to activity indicators (spinners, shimmer, pulses),
  which stop under Reduce motion; the transfer tree's auto-follow pauses on user intent. No marquee or auto-updating
  feed. Evidence: `app-utilities.css`, `ui/ProgressBar.svelte`, `file-operations/transfer-tree/`.
- **2.3.1 Three flashes or below threshold**: Supports. Only single 300 ms warning flashes and 1 Hz caret blinks.
  Evidence: `VolumeSpaceLine.svelte`, `VolumeChooserMenu.svelte`, `ViewerTextCursor.svelte`.
- **2.4.1 Bypass blocks**: Not applicable. Applies to web pages and sets of software; Cmdr is a single app (EN 11.2.4.1
  and 508 don't apply to software).
- **2.4.2 Page titled**: Supports. Each window has a title: main "Cmdr", the viewer the file name, plus Settings,
  Keyboard shortcuts, Queue, Debug. Every user-facing title comes from the catalog (Settings and Keyboard shortcuts
  since #442, 2026-10-15); Debug is a dev-build-only window and stays English like the rest of it. A language switch
  while a child window is open keeps its OS title until it reopens (no `set-title` capability there). EN and 508 don't
  apply this to software. Evidence: `src-tauri/src/licensing/app_status.rs`, `routes/viewer/+page.svelte`,
  `settings-window.ts`, `shortcuts-window.ts`, `debug-window.ts`, `queue-window.ts`.
- **2.4.3 Focus order**: Supports. Dialogs move focus in on open (child autofocus, first field, or the dialog), trap Tab
  in order, and restore focus to the trigger on close; the command palette does the same. Evidence: `ModalDialog.svelte`
  (records and restores `document.activeElement`), `focus-trap.ts`, `CommandPalette.svelte`.
- **2.4.4 Link purpose (in context)**: Supports. In-app links and buttons name their target; no "click here" copy.
  Evidence: English catalog grep (`lib/intl/messages/en/*.json`).
- **2.5.1 Pointer gestures**: Supports. Swipe and mouse side buttons for back/forward have ⌘[ / ⌘] and menu equivalents;
  viewer pinch and wheel zoom have `+`/`-`/`0`. Evidence: `src-tauri/src/mouse_nav.rs`, `commands/sources/file-list.ts`,
  `MediaImageView.svelte`.
- **2.5.2 Pointer cancellation**: Supports. Row `mousedown` only moves the cursor, selects, or arms a drag; opening is
  on double-click and buttons act on click. Evidence: `FullList.svelte`, `BriefList.svelte` row handlers,
  `FunctionKeyBar.svelte`.
- **2.5.3 Label in name**: Supports (Partially supports until 0.57.0). axe's `label-content-name-mismatch` never ran in
  the unit audits (its icon-ligature check needs canvas pixels, so it fell into `incomplete`); the E2E audit ran it for
  real on 2026-10-15 and caught every Full view row (fixed: the visible words run through the name in order, the rest
  parenthesized), and `test-a11y.ts` now runs the rule. The seven controls that opted out with `skipLabelInName` (the
  Servers hub refresh, `QueueRow` buttons, two `MultiRenameDialog` chips, `SuggestedOpsIndicator`, two `lib/ui`
  controls) were fixed in `d565214b8a` (#450, closed); at `v0.57.0` no audit opts out (`git grep "skipLabelInName:"`
  finds none). New strings since keep to it, for example the AI suggestions' "Why, according to the AI:" (`00e6f1b9ab`)
  and the date header's fallback note, which follows the visible label (`SortableHeader.svelte`). Evidence:
  `lib/test-a11y.ts`, `views/file-row-a11y.ts`, `views/DETAILS.md` (row names),
  `test/e2e-playwright/accessibility.spec.ts`.
- **2.5.4 Motion actuation**: Not applicable. No device-motion input.
- **3.1.1 Language of page**: Supports. Every window sets `<html lang>` from the chosen UI language (11 languages).
  Evidence: `intl/messages.svelte.ts` `setLocale()`, `intl/document-language.ts`, `initWindowLanguageSync()` in each
  window route.
- **3.2.1 On focus**: Supports. No control changes context on focus. Evidence: code reading of panes, dialogs, settings.
- **3.2.2 On input**: Supports. Settings apply live (a change of setting, not context); the Settings sidebar switches
  its content pane on arrow keys, like tabs. Evidence: `SettingsSidebar.svelte`.
- **3.2.6 Consistent help**: Supports. The native Help menu sits in the same place in every window (Keyboard shortcuts,
  What's new, Send feedback, debug log, error report). Evidence: `src-tauri/src/menu/menu_bar.rs`.
- **3.3.1 Error identification**: Supports. Rename, new folder, server, license key, and (since 0.57.0) the archive and
  restic password prompt (`file-operations/transfer/UnlockDialog.svelte`: `invalid`, `aria-describedby`, a re-keyed
  `role="alert"` note per wrong try) mark errors with `aria-invalid` and tie the message to the field; the license key's
  activation error also lands in an always-mounted `role="alert"` region (#438, 2026-10-15). Evidence:
  `InlineRenameEditor.svelte`, `NewEntryNameField.svelte`, `ServerFormFields.svelte`,
  `licensing/LicenseKeyDialog.svelte` (`showError`, `licensing.a11y.test.ts`). V5 heard.
- **3.3.2 Labels or instructions**: Supports. Settings and server forms use visible `<label for>`; the license key,
  rename, and new folder fields have a visible prompt plus an accessible name. Evidence: `SettingRow.svelte`,
  `ServerFormFields.svelte`, `SignInCredentialFields.svelte`, `LicenseKeyDialog.svelte`.
- **3.3.7 Redundant entry**: Supports. The contact email is stored and prefilled for feedback, error reports, and
  onboarding; server credentials stay in the Keychain. Evidence: `attach-email/attach-email.svelte.ts`.
- **4.1.1 Parsing**: Supports. Obsolete in WCAG 2.2 and always satisfied; the DOM is generated by Svelte.
- **4.1.2 Name, role, value**: Supports. Dialogs, settings controls (Ark UI-based checkbox, switch, select, slider),
  tabs, the command palette, the queue grid, and menus expose name, role, and state. The Search result cursor and the
  Settings sidebar selection are conveyed as `aria-activedescendant` on the focused field or list, landing a frame after
  focus (#438). Since #437 and #267 (2026-10-15): DOM focus sits on the focused pane's listbox, whose
  `aria-activedescendant` names the cursor row by a per-list id (`$props.id()`) a frame after focus and after every
  navigation commit; `aria-selected` = marked; the container is `role="group"`; submenu rows have ids, an entered
  submenu takes focus with its own `aria-activedescendant` and its parent row's name, and the parent row gets
  `aria-controls`; the volume chooser is a `<button aria-haspopup="menu" aria-expanded>`. Since 0.57.0 the inline rename
  `<input>` is the listbox's sibling, laid over the row's empty name slot (`rename/RenameOverlay.svelte`, #449 closed,
  `a7b318620e`), and Brief view's `role="listbox"` moved to an inner `.listbox-region` to make room; axe is clean with
  rename open in both views. Since #824 (2026-10-31) no control sits inside another: the shortcut pill's × is its
  sibling (`settings/sections/ShortcutPill.svelte`), Search's `Custom…` fields sit in a label
  (`query-ui/filter-chips/`), and Search result path pills are `aria-hidden` spans (`query-ui/PathPills.svelte`). The
  Servers view's host and share rows also gained their missing `role="list"` parent (an uncounted
  `aria-required-parent`, skipped audits in `network.a11y.test.ts`). None of it is heard yet (§ Re-hear). Evidence:
  `pane/active-pane-focus.ts` (`focusActivePane`), `pane/key-dispatch.ts` `handleFocusGuard`,
  `views/list-focus.svelte.ts`, `views/file-row-a11y.ts`, `ui/Menu.svelte`, `ui/deferred-active-descendant.svelte.ts`,
  `navigation/VolumeBreadcrumb.svelte`, `query-ui/QueryBar.svelte`, `SettingsSidebar.svelte`; virtual-screen-reader
  tests `file-list.vsr.svelte.test.ts`, `Menu.vsr.test.ts`, `QueryBar.vsr.svelte.test.ts`,
  `SettingsSidebar.vsr.svelte.test.ts`, `rename/rename.a11y.test.ts` (overlay block). V1, V3, V6, V7, and V10 heard on
  0.53.0-era `main`; the rename overlay is unheard (V5, § Re-hear).

## WCAG 2.2 Level AA

- **1.2.4 Captions (live)**: Not applicable. No live media.
- **1.2.5 Audio description (prerecorded)**: Not applicable. No video.
- **1.3.4 Orientation**: Supports. Desktop windows don't lock orientation.
- **1.3.5 Identify input purpose**: Supports. Server sign-in fields carry `autocomplete="username"` and
  `current-password`, and all three email fields (attach-email, onboarding, Settings > Updates) `email` (the last two
  since #442, 2026-10-15). Evidence: `ServerFormFields.svelte`, `SignInCredentialFields.svelte`,
  `AttachEmailCheckbox.svelte`, `onboarding/StepBeta.svelte`, `settings/sections/UpdatesSection.svelte`.
- **1.4.3 Contrast (minimum)**: Partially supports. Every text token pair is gated at 4.5:1 (3:1 large) in CI in light
  and dark mode under all nine accent variants and all pane tints; marked file names measure 4.52:1 worst case (light)
  and 4.71:1 (dark). The checker also sweeps the Increase contrast tier (`html.increase-contrast` tokens, 15% / 25%
  tints); the 4.16:1 brown-tint corner it found there closed with #440 on 2026-10-15 (deeper light reds). Exception: the
  opt-in "App" date and size palettes put the raw accent on text (gold about 2.4:1). Core tokens on `#ffffff` /
  `#1e1e1e`: primary 17.40 / 13.61, secondary 5.74 / 9.56, tertiary 5.74 / 7.18, error text 6.47 / 8.78, warning text
  7.31 / 9.89. Evidence: `pnpm check a11y-contrast` (green), `scripts/check-a11y-contrast/row_state_matrix.go`,
  `app.css` § Increase contrast. #454.
- **1.4.4 Resize text**: Supports. App zoom (⌘+ / ⌘−, 75–200%; the ceiling rose from 150% with #441, 2026-10-15) reaches
  200% alone and compounds with the macOS Accessibility text size (0.82–3.12×) beyond it; tokens are rem-based and rows
  grow with text. 2.0 adds no new effective scale: 150% × the system's 1.35+ sizes was already past it. Evidence:
  `settings/definitions/appearance.ts` (`TEXT_SIZE_RANGE`), `src-tauri/src/text_size.rs`, `lib/text-size.svelte.ts`,
  `command-handlers/view-handlers.ts`, `settings/reactive-settings.svelte.ts`.
- **1.4.5 Images of text**: Supports. All text is live text; no images of text in the UI.
- **1.4.10 Reflow**: Partially supports. Dialogs and Settings are single-column and their minimum widths scale with
  text. The main window has a 950×550 minimum and a dual-pane, columnar layout that needs two dimensions (the exception
  for content that requires 2-D layout covers the file grid, not the chrome). Pending a manual check at 200% zoom plus
  large system text. Evidence: `src-tauri/tauri.conf.json`, `settings/settings-window.ts`, `file-viewer/open-viewer.ts`.
- **1.4.11 Non-text contrast**: Partially supports. Checkbox and switch borders use `--color-control-border` (about
  3.7:1), and text field borders `--color-field-border` (the same token, 3.1:1 worst on the dialog surface) since #440,
  2026-10-15, gated by `non_text_states.go`. Under Increase contrast every focus ring gains a `--color-focus-ink` layer
  (14.6:1 worst). The marked-file bar is 4.51:1 worst (light) and 4.71:1 (dark) against its row, gated at 3:1 in every
  look by `row_mark_states.go`. The cursor hairlines (the unfocused one dashed there, in the full control-border gray)
  reach 3.83:1 worst under Increase contrast and 3.10:1 under Differentiate without color (both gated); by default both
  are solid and measured not gated: the accent one stays subtle by design at 1.09:1 worst (light, Apple Yellow, brown
  tint), the faint gray one (the control border at 35%) is 1.41:1 worst light and 1.52:1 dark (brown and lime tints),
  4.26:1 under Increase contrast as the full-strength border; the tab accent bands are 1.45:1 (focused) and 1.22:1
  (unfocused) worst by default, 5.26:1 for the focused one under Differentiate without color. By default the 2px accent
  focus ring is under 3:1 on white with Apple Yellow (1.58), Cmdr gold (2.38), Green (2.41), and Orange (2.58).
  Evidence: `app-field.css` (`--color-field-border`), `app.css` `button:focus-visible`, `--shadow-focus-solid`, §
  Differentiate without color, `app-file-list.css` cursor rules; ratios from `pnpm check a11y-contrast` (`-- --verbose`
  lists the measured ones) and `accent_matrix.go` hexes. #454.
- **1.4.12 Text spacing**: Partially supports. Rows have fixed pixel heights with `overflow: hidden`; a 1.5 line height
  fits the Comfortable density but clips in Compact (16 px rows), and middle truncation measures with canvas metrics
  that ignore letter and word spacing overrides. Pending a manual text-spacing test. Evidence: `BriefList.svelte`,
  `FullList.svelte` row styles, `settings/types.ts` density mappings.
- **1.4.13 Content on hover or focus**: Supports. Tooltips show on hover and keyboard focus and any key dismisses them.
  Since #441 (2026-10-15) they're hoverable: a shown tooltip takes the pointer, leaving its trigger starts a 250 ms
  grace that reaching the tooltip (or the trigger) cancels, and a trigger crossed on the way waits for the grace instead
  of taking over. A press on the tooltip dismisses it. Evidence: `lib/tooltip/tooltip.ts` (+ `tooltip.test.ts` §
  hoverable), `app-tooltip.css`.
- **2.4.5 Multiple ways**: Not applicable. Applies to sets of web pages or software.
- **2.4.6 Headings and labels**: Supports. Settings uses `h1`/`h2`/`h3`, dialogs an `h2` title, panes named regions.
  Each pane's listbox is named "{side} pane, {folder}" (#437, 2026-10-15), and since VoiceOver never speaks a listbox's
  name on focus (spike, see V2), a per-pane polite live region says it on pane switch and folder change. Evidence:
  `routes/settings/+page.svelte`, `SettingsSection.svelte`, `ModalDialog.svelte`, `FilePane.svelte` (`listLabel`),
  `pane/pane-location-announcer.svelte.ts`, `ui/LiveAnnouncer.svelte`. V2 heard: `Tab` reads the other pane's cursor
  row, then "Right pane, vo-target, empty folder".
- **2.4.7 Focus visible**: Partially supports. Buttons, links, fields, and menus show a focus ring (`:focus` for fields
  so mouse users see it too), and the file cursor has an accent fill plus outline. Since #440 (2026-10-15) the focused
  pane's cursor hairline is accent and the other's gray, and the focused pane's active tab carries the full accent band
  (the other half). By default both cues stay under 3:1 on purpose (see 1.4.11): the panes differ by hue and the tab
  band's strength rather than by contrast. Increase contrast and Differentiate without color dash the unfocused hairline
  and lift both past 3:1. Evidence: `app.css`, `app-field.css`, `app-file-list.css`, `tabs/TabBar.svelte`. #454.
- **2.4.11 Focus not obscured (minimum)**: Supports. Dialogs are modal and centered. Toasts sit at the top right and
  move to the bottom right while they would cover the focused element's cursor row (`aria-activedescendant`) or the
  focused element (#439, 2026-10-15). Evidence: `ui/toast/toast-placement.ts` (+ test), `ToastContainer.svelte`.
- **2.5.7 Dragging movements**: Supports. The pane divider (double-click reset, ⌃⌥←/→), drag-and-drop copy and move
  (F5/F6, context menu, copy and paste), and tab reordering and moving (`tab.move*`, #441, 2026-10-15) have
  alternatives. The queue window's drag-to-reorder (0.57.0, `67edbd06ac`) has ⌥↑ / ⌥↓ and the row's right-click menu
  (Move up, Move down). Moving a dialog by its title bar has no alternative, and needs none: its position does nothing.
  Evidence: `PaneResizer.svelte`, `commands/sources/main-window.ts`, `pane/tab-operations.ts`,
  `file-operations/queue/DETAILS.md` § Dragging to reorder.
- **2.5.8 Target size (minimum)**: Supports (#817, 2026-10-31). Sort headers, function-key bar, the new-tab button, and
  the tab close button (an unpainted 24×24 `::before`) meet 24×24 px. Breadcrumb segments span the header's full height
  (about 29 px at 100%) with a 26 px floor (`breadcrumb-fit.ts` says why 26), so `~` and one-letter folders hold a 24 px
  square clear of their neighbors. The pane divider holds one too, at its foot: W3C's Understanding page counts a target
  when "a solid 24 by 24 CSS pixel square" fits in it (fetched 2026-10-31), so the divider is 24×24 there, in the
  footers' blank inset, and stays an 8 px strip along the list, where 24 px would cover file icons or the left pane's
  scrollbar (`pane/DETAILS.md` § Pane divider from the keyboard). A static sweep of the app's `.svelte` styles
  (2026-10-31, selectors naming a control with an explicit width under 24 px) found more, now closed: the × and `+` and
  reset on Settings shortcuts, the password reveal, the Settings search clear, a toast's ×, a filter chip's ×
  (`.hit-target` in `app-utilities.css`, unpainted), the Ask Cmdr rail's resize edge (a 24×24 pad over the header's
  padding and glyph), and a resizable dialog's corners (24×24, reaching outward). Search path pills have a 24 px floor
  and take their row's height, 23.6 px at 100%, which the "inline" exception covers: the row's line height sets it. Two
  under 24 px wide meet the "equivalent" exception: the Full date header's Sort by chevron
  (`views/DateSortChevron.svelte`, 20 px wide, the header's 26 px tall), whose menu a right-click anywhere on the header
  row also opens; and the queue row's drag grip (14 px, `aria-hidden`), whose moves the full-row right-click menu also
  makes. Evidence: `TabBar.svelte`, `FilePane.svelte` `.path-segment`, `PaneResizer.svelte`, `FullListHeader.svelte`
  (header height), `file-operations/queue/QueueRow.svelte` `.grip`. Not measured in a running app; the sweep can't see a
  size that comes from padding or a token, so a re-evaluation should measure real targets.
- **3.1.2 Language of parts**: Supports. The language picker lists each language in its own name, with a per-option
  `lang` on the row and on the trigger while selected (#442, 2026-10-15). Evidence: `intl/locale-display-names.ts`,
  `settings/definitions/appearance.ts` `languageOptions()`, `ui/Select.svelte`.
- **3.2.3 Consistent navigation**: Not applicable. Applies to sets of web pages or software. (The menu bar is the same
  in every window.)
- **3.2.4 Consistent identification**: Not applicable. Same as 3.2.3. (Commands keep one name everywhere through the
  command registry.)
- **3.3.3 Error suggestion**: Supports. Error copy says what happened and what to try, for example the listing,
  invalid-name, and license-server messages. Evidence: `lib/intl/messages/en/errors.json` `*.suggestion` keys,
  `docs/style-guide.md`.
- **3.3.4 Error prevention (legal, financial, data)**: Supports. Delete always confirms, permanent delete confirms even
  with "skip confirmation" on, overwrites and rename collisions get conflict dialogs, trash is undoable, and copy and
  move can be rolled back. Undo trash is on its toast and in the Edit menu (#439, 2026-10-15); the menu item greys out
  with nothing to undo and names what it puts back, and an undo over an emptied trash says so (`d5706b6d8`). The
  operation log rolls back any journaled trash. Since 0.57.0 the operation log can be turned off (`b63b2d76e2`, a CRA
  opt-out, on by default): new operations then journal nothing, and every undo surface says so instead of offering an
  undo it can't keep. An opt-in setting keeps the progress dialog up when an operation finishes, with Undo
  (`df79bfc981`). Evidence: `settings/definitions/behavior.ts`, `operation-log/recording.ts`, `DeleteDialog.svelte`,
  `TransferConflictDialog`, `RenameConflictDialog.svelte`, `operation-log/RollbackControls.svelte`,
  `delete/trash-undo.ts`.
- **3.3.8 Accessible authentication (minimum)**: Supports. License key, API keys, and passwords accept paste; no
  cognitive tests. Evidence: no `onpaste` handlers in `apps/desktop/src`; `SettingPasswordInput.svelte`.
- **4.1.3 Status messages**: Supports. Toasts speak through two always-mounted sibling live regions, polite for status
  and assertive for warnings and failures, with no region nested in another (#439, 2026-10-15). The transfer phase
  banner and Search result counts are polite live regions, listing errors are alerts. Since #438 (2026-10-15), each file
  pane's one live region also says an empty folder, a slow load, the selection count, and the quick filter's match count
  (debounced, so a burst says its end state), and the command palette and the Settings search say their result counts.
  EN and 508 don't apply this to software. Evidence: `ToastContainer.svelte` (`ToastContainer.vsr.svelte.test.ts`),
  `ui/toast/toast-speech.ts`, `TransferProgressDialog.svelte`, `QueryResults.svelte`, `pane/ErrorPane.svelte`,
  `pane/pane-status-announcer.svelte.ts`, `ui/LiveAnnouncer.svelte`, `CommandPalette.svelte`, `SettingsSidebar.svelte`.
  V4, V6, and V9 heard.

## Revised Section 508

### Chapter 3: functional performance criteria

- **302.1 Without vision**: Partially supports. Dialogs, settings, menus, and forms are built for screen readers; since
  #437 (2026-10-15) the file list exposes its cursor row with kind and position, marked files, and the pane and folder
  (see 4.1.2), and since 0.57.0 the rename field is a real text field (#449 closed). Left: Full view's columns not being
  a table (#451). V1–V12 heard on 0.53.0-era `main`; § Re-hear lists what changed since. #451.
- **302.2 With limited vision**: Partially supports. App zoom alone to 200%, past it with macOS text size, dark mode, AA
  text contrast in CI. Weak focused-pane and focus-ring contrast with some accents unless Increase contrast or
  Differentiate without color is on. #454.
- **302.3 Without perception of color**: Partially supports. Marked rows carry a bar and a text stroke (#440,
  2026-10-15), and the focused pane's tab band is stronger; the panes' cursor hairlines differ by color alone unless
  Differentiate without color or Increase contrast is on, which dashes the unfocused one. Hidden files still rely on
  color alone. The rest uses glyphs and text. #452, #454.
- **302.4 Without hearing**: Supports. Cmdr uses no audio cues.
- **302.5 With limited hearing**: Supports. Same as 302.4.
- **302.6 Without speech**: Supports. No speech input needed.
- **302.7 With limited manipulation**: Supports. Full keyboard operation with rebindable shortcuts and no
  simultaneous-key requirement beyond modifiers (Sticky Keys works). Toast actions are reachable with "Move focus to the
  notifications" (#439, 2026-10-15), and tabs reorder and move from the keyboard (#441, 2026-10-15). Click targets: see
  2.5.8.
- **302.8 With limited reach and strength**: Supports. Software only; no physical reach.
- **302.9 With limited language, cognitive, and learning abilities**: Supports. Plain, conversational copy
  (`docs/style-guide.md`), confirmations, undo and rollback, consistent menus, and a setting that keeps toasts until
  they're closed (#439, 2026-10-15).

### Chapter 4: hardware

- Not applicable. Cmdr is software.

### Chapter 5: software

- **501.1 Scope, incorporation of WCAG 2.0 AA**: See the WCAG 2.2 sections.
- **502.2.1 User control of accessibility features**: Not applicable. Cmdr isn't platform software.
- **502.2.2 No disruption of accessibility features**: Supports. VoiceOver's ⌃⌥ chords pass through (type-to-jump bails
  on ⌃/⌥), system zoom isn't blocked, menus are native. Main-window `Tab` is a rebindable pane switch, which keeps Full
  Keyboard Access off the pane chrome; `pane.focusControls` (⌃F5, remappable; `⌘⇧F5` off macOS) moves focus into it, and
  the arrows walk it (#437, 2026-10-15). Evidence: `pane/type-to-jump-keys.ts`, `commands/sources/main-window.ts`,
  `pane/pane-chrome.ts`.
- **502.3.1 Object information**: Supports. Standard controls expose name, role, and state through WebKit; file rows
  expose name, kind, position, and marked state (#437, 2026-10-15), the inline rename field sits beside the listbox as a
  real text field (#449 closed), and no control sits inside another (#824, see 4.1.2). V1 and V3 heard on 0.53.0-era
  `main` (re-hear, § Re-hear).
- **502.3.2 Modification of object information**: Supports. States set by the user (checkboxes, switches, selects) are
  standard ARIA controls that assistive tech can change. V7 heard.
- **502.3.3 Row, column, and headers**: Partially supports. The queue window is a `role="grid"`; the Full view file list
  has no column headers tied to cells (a listbox by decision; each row's name carries its size and date). Evidence:
  `routes/queue/+page.svelte`, `FullListHeader.svelte`, `views/file-row-a11y.ts`. #451.
- **502.3.4 Values**: Supports. Progress bars expose `aria-valuenow/min/max`, and the transfer readout's bars also say
  the amount on screen as `aria-valuetext` ("1.2 GB of 3 GB", #438, 2026-10-15); sliders and number fields expose
  values. Evidence: `ui/ProgressBar.svelte`, `file-operations/TransferProgressReadout.svelte`. V4 heard.
- **502.3.5 Modification of values**: Supports. Sliders, number inputs, and selects are Ark UI controls operable by
  assistive tech. V7 heard.
- **502.3.6 Label relationships**: Supports. `<label for>` and `aria-describedby` throughout, the license key error and
  the two alertdialogs (`DeleteAiModelDialog`, `ForgetMemoryDialog`) included since #438 (2026-10-15). V5 heard.
- **502.3.7 Hierarchical relationships**: Partially supports. Dialogs, menus, the tablist, and the Settings sidebar
  (subsections grouped under their section, #438) are structured; tabs have no `tabpanel`. #451.
- **502.3.8 Text**: Supports. All UI text is live DOM text exposed through WebKit. V11 heard.
- **502.3.9 Modification of text**: Supports. Text entry uses native `<input>`/`<textarea>`.
- **502.3.10 List of actions**: Supports. Controls are native buttons and ARIA widgets with standard actions. V8 heard.
- **502.3.11 Actions on objects**: Supports. Same as 502.3.10. V8 heard.
- **502.3.12 Focus cursor**: Supports. Dialog and settings focus is real DOM focus, and the Search cursor, the Settings
  sidebar selection (#438), and the file list cursor (#437, 2026-10-15; see 4.1.2) are exposed as
  `aria-activedescendant` on the focused element. V1 and V6 heard.
- **502.3.13 Modification of focus cursor**: Partially supports. Assistive tech can move focus among standard controls,
  and VoiceOver follows Cmdr's cursor (V1 heard); moving the VoiceOver cursor onto a file row likely doesn't move Cmdr's
  cursor (unheard, C3 in § VoiceOver listening runs). #453.
- **502.3.14 Event notification**: Supports. See WCAG 4.1.3. V4 heard.
- **502.4 Platform accessibility features**: Not applicable. Applies to platforms.
- **503.2 User preferences**: Supports. Honors system font, dark/light, accent color, Reduce transparency, Reduce
  motion, Increase contrast, Differentiate without color, and macOS text size, all live. The Reduce motion misses
  (settings sidebar jump, rename shake and zoom, boot loader, switch thumb, disclosure chevrons, swatch and
  onboarding-dot grows) closed with #442 on 2026-10-15. Increase contrast (stronger borders, secondary text, focus-ring
  ink layer, deeper light reds, raised tints) and Differentiate without color (the focused cursor hairline and the
  focused tab band lifted to `--color-accent-pop`, the unfocused hairline dashed in the full control-border gray) closed
  with #440 on 2026-10-15; the contrast checker sweeps both tiers. Show borders (2026-10-15, `8736c60c8`) gives the
  borderless controls (tab close and new tab, the volume chip, path segments, the breadcrumb options and eject buttons)
  a `--color-control-border` hairline. Not yet seen on a Mac with the settings flipped. Evidence: `app.css`
  (`--font-system`, § Increase contrast, § Differentiate without color), `app-utilities.css` § Show borders
  (`.borderless-control`), `src-tauri/src/display_accessibility.rs`, `lib/display-accessibility.ts`,
  `src-tauri/src/text_size.rs`, `lib/accent-color.ts`.
- **503.3 Alternative user interfaces**: Not applicable. Cmdr provides no alternative UI acting as assistive tech.
- **503.4.1 Caption controls**: Not applicable. No video.
- **503.4.2 Audio description controls**: Not applicable. No video.
- **504.2 Content creation or editing**: Not applicable. Cmdr isn't an authoring tool.
- **504.2.1 Preservation of information provided for accessibility in format conversion**: Not applicable.
- **504.2.2 PDF export**: Not applicable.
- **504.3 Prompts**: Not applicable.
- **504.4 Templates**: Not applicable.

### Chapter 6: support documentation and services

- **602.2 Accessibility and compatibility features**: Supports. The accessibility guide (`/trust/accessibility-guide`,
  opened by Help > Accessibility guide, `help.accessibility`) documents the features, how to turn each on, and the known
  gaps (from `gapIssues`); the Keyboard shortcuts window (Help menu) documents every keyboard command. #443.
- **602.3 Electronic support documentation**: Supports. getcmdr.com's 15 main pages pass axe in light and dark mode on
  every change (the `website-e2e` check). The accessibility guide joined the list on 2026-10-30; before that the guide
  itself was untested. Blog posts and the 404 page aren't in it (`/feature-status` is a redirect stub). CI's Lighthouse
  run audits the home page and fails below 90 for accessibility (`lighthouserc.cjs`); the 100 recorded on 2026-09-08 is
  the last measured score, so the report says "passes". Evidence: `apps/website/e2e/accessibility.spec.ts`,
  `apps/website/lighthouserc.cjs`.
- **602.4 Alternate formats for non-electronic support documentation**: Not applicable. No non-electronic docs.
- **603.2 Information on accessibility and compatibility features**: Supports. Support answers by email
  (hello@getcmdr.com) and in-app feedback, and points to the accessibility guide. #443.
- **603.3 Accommodation of communication needs**: Supports. All support is written (email, in-app feedback); no phone
  needed.

## EN 301 549

### Clause 4: functional performance statements

- **4.2.1 Usage without vision**: Partially supports. See 508 302.1. #451.
- **4.2.2 Usage with limited vision**: Partially supports. See 508 302.2. #454.
- **4.2.3 Usage without perception of colour**: Partially supports. See 508 302.3. #452, #454.
- **4.2.4 Usage without hearing**: Supports. No audio.
- **4.2.5 Usage with limited hearing**: Supports. No audio.
- **4.2.6 Usage with no or limited vocal capability**: Supports. No speech input.
- **4.2.7 Usage with limited manipulation or strength**: Supports. See 508 302.7.
- **4.2.8 Usage with limited reach**: Supports. Software only.
- **4.2.9 Minimize photosensitive seizure triggers**: Supports. See WCAG 2.3.1.
- **4.2.10 Usage with limited cognition, language or learning**: Supports. See 508 302.9.
- **4.2.11 Privacy**: Supports. Accessibility features don't expose private data; passwords and keys use masked fields
  and the Keychain.

### Clause 5: generic requirements

- **5.1 Closed functionality (5.1.2–5.1.7)**: Not applicable. Cmdr is open to assistive technology.
- **5.2 Activation of accessibility features**: Supports. Zoom runs from the keyboard, the View menu, and Settings;
  system accessibility settings apply without extra steps.
- **5.3 Biometrics**: Not applicable. No biometric identification.
- **5.4 Preservation of accessibility information during conversion**: Not applicable. Cmdr copies and archives files
  without converting their formats.
- **5.5.1 Means of operation**: Not applicable. Hardware.
- **5.5.2 Operable parts discernibility**: Not applicable. Hardware.
- **5.6.1 Tactile or auditory status**: Not applicable. No locking or toggle keys of its own.
- **5.6.2 Visual status**: Not applicable. Same as 5.6.1.
- **5.7 Key repeat**: Not applicable. Uses the system key repeat.
- **5.8 Double-strike key acceptance**: Not applicable. Uses the system keyboard settings.
- **5.9 Simultaneous user actions**: Supports. Chords are modifier plus key, which Sticky Keys sequences; nothing needs
  two pointer actions at once.

### Clauses 6–10 and 13

- Not applicable. See the summary.

### Clause 11: software

- **11.1–11.4 (WCAG mappings)**: See the WCAG 2.2 sections.
- **11.5.1 Closed functionality**: Not applicable.
- **11.5.2.1–11.5.2.3 Platform accessibility service support and use**: See 11.5.2.5–11.5.2.17. Cmdr uses the platform
  service through WebKit's NSAccessibility bridge.
- **11.5.2.4 Assistive technology**: Not applicable. Cmdr isn't assistive technology.
- **11.5.2.5 Object information**: Supports. See 508 502.3.1.
- **11.5.2.6 Row, column, and headers**: Partially supports. See 508 502.3.3. #451.
- **11.5.2.7 Values**: Supports. See 508 502.3.4.
- **11.5.2.8 Label relationships**: Supports. See 508 502.3.6.
- **11.5.2.9 Parent-child relationships**: Partially supports. See 508 502.3.7. #451.
- **11.5.2.10 Text**: Supports. See 508 502.3.8.
- **11.5.2.11 List of available actions**: Supports. See 508 502.3.10.
- **11.5.2.12 Execution of available actions**: Supports. See 508 502.3.11.
- **11.5.2.13 Tracking of focus and selection attributes**: Supports. See 508 502.3.12; marked files are exposed as
  `aria-selected` in an `aria-multiselectable` listbox (#437, 2026-10-15). V1 and V3 heard.
- **11.5.2.14 Modification of focus and selection attributes**: Partially supports. See 508 502.3.13. #453.
- **11.5.2.15 Change notification**: Supports. See WCAG 4.1.3. V4 and V9 heard.
- **11.5.2.16 Modifications of states and properties**: Supports. See 508 502.3.2.
- **11.5.2.17 Modifications of values and text**: Supports. See 508 502.3.5 and 502.3.9.
- **11.6.1 User control of accessibility features**: Not applicable. Cmdr isn't platform software.
- **11.6.2 No disruption of accessibility features**: Supports. See 508 502.2.2.
- **11.7 User preferences**: Supports. See 508 503.2.
- **11.8.1–11.8.5 Authoring tools**: Not applicable.

### Clause 12: documentation and support services

- **12.1.1 Accessibility and compatibility features**: Supports. See 508 602.2.
- **12.1.2 Accessible documentation**: Supports. See 508 602.3.
- **12.2.2 Information on accessibility and compatibility features**: Supports. See 508 603.2.
- **12.2.3 Effective communication**: Supports. See 508 603.3.
- **12.2.4 Accessible documentation**: Supports. See 508 602.3.

## VoiceOver checks

Each ID matches a step in David's guide; the result confirms the rows named. Every check is PASS by David's sign-off run
(§ VoiceOver listening runs, run 3), which was by ear with no phrase log. Quoted phrases are verbatim from the two
agent-driven runs before it.

- **V1 Browse a folder**: PASS. Rows: 1.1.1, 1.3.1, 4.1.2, 302.1, 502.3.1, 502.3.12, 502.3.13, 4.2.1, 11.5.2.5,
  11.5.2.13, 11.5.2.14. Full view ↓ "subfolder (folder), 3 bytes, (modified) 2026-10-15 10:04 (2 of 8)", "a.txt, 1,02
  kB, (modified) 2026-10-15 10:04 (3 of 8)"; Brief "d-link (symlink) (6 of 8)", "e.bin (7 of 8)"; entering a folder
  reads its `..` row (1 of 2), Backspace the row you came from. A 30,000-file folder read "h-30000.txt (30 001 of 30
  001)" with no `aria-setsize` churn. The `..` row once read its cell bare ("Parent folder, 36,86 kB"), which sounded
  like the parent's size; it says "(this folder)" since `2d18f0be9`. Released 0.52.0 said "File explorer web
  application… You are currently on a web application" on launch, from the old `role="application"`; no run heard it.
- **V2 Switch panes**: PASS. Rows: 1.3.1, 2.4.6. `Tab`: "Parent folder, 0 bytes (1 of 1) Right file pane region", then
  "Right pane, vo-target, empty folder"; back: the left cursor row, then "Left pane, vo-test". The AX trace posts the
  row 5–15 ms before the announcer every time; by ear (run 3), both are spoken.
- **V3 Mark files**: PASS. Rows: 4.1.2, 502.3.1, 11.5.2.13. Space "a.txt, … selected (3 of 8)" then "1 selected"; ↓
  "b.txt … (4 of 8)" (no selection word); ⌘A "7 selected"; ⌘⇧A "Nothing selected".
- **V4 Copy with the progress dialog**: PASS. Rows: 4.1.3, 502.3.4, 502.3.14, 11.5.2.7, 11.5.2.15. Completion "Copied 1
  file."; run 1's 100 MB copy finished at once (APFS clone), so the dialog title and the progress value were heard in
  run 3, on the guide's 2–5 GB copy.
- **V5 Rename**: PASS. Rows: 3.3.1, 502.3.6, 11.5.2.8. The field's name and the invalid-name error (run 3). The license
  key error wasn't part of it (activation contacts the server).
- **V6 Search**: PASS. Rows: 4.1.2, 4.1.3, 502.3.12. "11 of 11 results"; ↓ "h-29998.txt /private/tmp/…/vo-huge 0 bytes
  2026-10-15 10:04 selected (2 of 11)". A search with no results says "No files match these criteria" since `1c9e0acbb`.
- **V7 Settings**: PASS. Rows: 1.3.1, 4.1.2, 502.3.2, 502.3.5, 502.3.7, 11.5.2.16, 11.5.2.17. "Colors and formats
  Appearance group (2 of 15)", ↓ "Zoom and density"; the Settings search said "2 sections match". Every sidebar row
  carries its position since `966f0af0b`. A control's label and value, and the new value after a change (run 3).
- **V8 Confirmation dialog (delete)**: PASS. Rows: 502.3.10, 502.3.11, 11.5.2.11, 11.5.2.12. Title, message, and
  buttons, and the buttons work (run 3).
- **V9 Toast or error message**: PASS. Rows: 4.1.3, 11.5.2.15. "Copied 1 file." was spoken while focus stayed on the
  row; ⌃F6 reached it ("Copied 1 file. group", ↓ "Dismiss notification button"), and a focused toast stayed past 20 s.
  The trash toast read by itself (run 3). The Space tip toast keeps its link in speech since `2ce9db14a`.
- **V10 In-app menus**: PASS. Rows: 4.1.2. ⌥F1 "Macintosh HD APFS current location menu item, group Volumes group (2 of
  4)"; → "naspi on 100.127.48.122 expanded submenu", ↓ "Connect directly now selected menu item (2 of 4)", ↓ "Use Cmdr's
  fast direct connection selected checked menu item (4 of 4)", ← "… collapsed submenu", Escape back to the list's row.
  Favorites (⌃D) "Applications collapsed submenu Favorites group (1 of 2)"; the archive Enter popup "Browse like a
  folder menu item menu Open archive or bundle"; the drive-index badge menu "Rescan now menu item menu Drive index
  status" (silent before `f58df3754`). The viewer's right-click menu (native) opened as "menu (2 items)" in run 1, and
  its rows read in run 3.
- **V11 Read text**: PASS. Rows: 502.3.8, 11.5.2.10. A text file in the viewer read in full (run 3).
- **V12 Reading order**: PASS. Rows: 1.3.2. VoiceOver's order through a Settings section matched the screen (run 3).

## VoiceOver listening runs

All three on 2026-10-15, macOS 27.0, VoiceOver 10. Runs 1 and 2 were agent-driven: `AppleKeyboardUIMode` = 3, dev builds
(`pnpm dev --worktree …`), real keys through `axprobe --key` and System Events, the spoken text polled through
VoiceOver's AppleScript bridge (`content of last phrase`) beside an `axprobe watch` trace of what WebKit posted. The
poll misses a phrase VoiceOver replaces within about 150 ms, so the trace is the truth for "posted" and the phrase log
for "spoken". Fixture: `vo-test` (seven entries), `vo-target` (empty), `vo-p2`, and `vo-huge` (30,000 files) in the
session scratchpad.

- **Run 1, `1e669900c`** (10:03–10:26): everything in § VoiceOver checks except what says otherwise. Under 65–175% CPU
  from startup indexing, and VoiceOver said "Cmdr is not responding" twice at launch.
- **Main failure, the silent list**: after focus reached a list any way but a clean `Tab` (⌘1/⌘2 view toggle, MCP
  navigation, a click on the other pane's empty space, the drive-index badge menu, the first archive popup), WebKit
  posted no focus change for `aria-activedescendant` moves until focus left and came back. Cause: a list or menu surface
  focused before layout has no WebKit accessibility object yet, so WebKit posts focus on the web area and then ignores
  the element's active-descendant moves. Fixed in `f58df3754` (`focusLaidOut`, `ui/DETAILS.md` § Menu).
- **Run 2, `f58df3754`** (10:48–11:06): ⌘2 "right-r2.txt (3 of 8)", ↓ "right-r3.txt (4 of 8)"; ⌘1 "Right pane, vo-p2
  list box right-r3.txt, 0 bytes, (modified) 2026-10-15 00:40 (4 of 8)"; MCP navigation, `Tab`, a real row click in the
  other pane, the archive popup, and the drive-index badge menu all read their row and kept reading on ↓. The quick
  filter said "3 names contain “b”" and ↓ read the filtered rows. Found here and fixed since, unheard: a click on the
  other pane's empty list space didn't switch panes (`fa4f15189`), clearing the filter was silent (`ed7435a72`, "Filter
  cleared, 8 names"), and a click on a toast's text stranded focus on `<body>` (`615a87390`).
- **Pane chrome** (run 1): ⌃F5 reached the webview with Full Keyboard Access on ("Macintosh HD, choose a volume menu pop
  up button"); → "private button", ↑ "vo-test selected tab, 1 of 1 Left pane tabs tab group", ↓ "Name, sorted ascending
  button", Escape back to the list. Real clicks on the volume chip, sort headers, and a tab left focus on the list (the
  chip's menu returned it on Escape). VO-Space on the chip opened its menu.
- **Command palette**: "Search commands… web dialog with 3 items … combo box"; "swap" → "Swap panes ⌘U selected (1 of
  1)", "1 command".
- **Run 3, David's sign-off** (`main` at `8feba4c02` or later): David ran the whole guide, V1–V12, by ear with his own
  keys and reported "VoiceOver done, it works fine". Every check passed, including the steps runs 1 and 2 couldn't reach
  (a slow copy's title and progress, V5, V8, V11, V12, a Settings control's value) and the viewer's right-click menu.

Not covered by any run (outside the guide): C1's VO-→ from the list to the volume chip; C3 (the VO cursor onto a path
segment with "keyboard focus follows VoiceOver cursor"); Enter on a path segment and a real click on one; the real-key
scroll check in Brief and Full view; launch with focus in the left pane; the Full view's localized git status words
(`2e8e38025`); and run 2's three fixes (the empty-space click, clearing the filter, the toast-text click). Raw logs of
runs 1 and 2 stayed in the session scratchpad.

## Re-evaluation 2026-10-30 (0.57.0)

Every row re-checked against tag `v0.57.0` (the report's claims must be true of the released app), from 0.53.0's
2026-10-15 evaluation. Method: `git log v0.53.0..v0.57.0` filtered to ARIA, focus, motion, and announcer changes (`-G`
over `apps/desktop/src`), a diff of every file the rows above cite as evidence, and `git grep v0.57.0` for opt-outs
(`skipLabelInName`, `nested-interactive`, `it.skip` in `*.a11y.test.ts`). Rows whose code didn't move were spot-checked:
window minimum size, zoom range, notification timeouts (4–10 s, plus persistent ones), Help menu items, the shipped
languages, the ⌃F5 / ⌃F6 / tab-move commands, the queue grid, and the absence of `tabpanel` and audio or video.
`pnpm check a11y-contrast` is green (the global CSS gained only the inactive text-selection tokens and a gated
ToggleGroup focus-ring pair). Counts per table are unchanged.

Level changes:

- **2.5.3 Label in name**: Partially supports → Supports (#450 closed, `d565214b8a`).
- **2.1.1 Keyboard**: Supports → Partially supports. Not a regression: the Settings shortcut pill's mouse-only × was
  there at 0.53.0 and the first audit missed it. #824.

Same level, new reason: 4.1.2, 302.1, 502.3.1, 4.2.1, and 11.5.2.5 lose #449 (the rename field is fixed) and stay
"Partially supports" for #451 and #824. New remark facts: Markdown view (1.1.1, 1.2.1), dialog letter keys (2.1.4),
queue drag-to-reorder (2.5.7), the date-sort chevron (2.5.8), the unlock prompt (3.3.1), the operation log off switch
(3.3.4), and the website's axe list (602.3).

### Re-hear (2026-10-30)

The code under these VoiceOver checks changed between 0.53.0 and 0.57.0. Nothing read contradicts the rows, so their
wording stays, but David's 2026-10-15 run didn't hear it. Re-hear on 0.57.0 or later:

- **V5 Rename** (rows 3.3.1, 4.1.2, 502.3.1, 502.3.6): most important. The field now lives beside the listbox
  (`a7b318620e`). Listen for it announced as a text field with its name, the selected name, and the invalid-name
  message; then Escape and the cursor row read again.
- **V1 Browse a folder, V2 Switch panes, V3 Mark files** (1.1.1, 1.3.1, 2.4.6, 4.1.2, 502.3.1, 502.3.12, 11.5.2.13):
  Brief view's `role="listbox"` moved onto an inner region for the rename overlay. In Full view, the date phrase now
  says whichever date the column shows ("(created)", "(added)"), and a fallback date header says why. Run V1–V3 in Brief
  view too, not only Full.
- **V4 Copy with the progress dialog** (4.1.3, 502.3.4, 502.3.14): the dialog opens with focus on Pause, its from → to
  row is its description, a conflict-policy caption sits under it, Escape cancels with a "Copy canceled" alert, and an
  opt-in done state replaces the dialog's buttons. Listen for the title plus description on open, the progress value,
  and the cancel alert.
- **V6 Search** (4.1.2, 4.1.3, 502.3.12): the result list's arrow keys moved to `ColumnList`'s shared cursor model
  (`e84a7dcfec`). Listen that ↓ still reads each result.
- **V9 Toast** (4.1.3, 11.5.2.15): the close button moved first in each toast's DOM, and frames stopped being focus
  stops. Listen to the trash toast, then ⌃F6 and Tab through its buttons, and Escape back.
- **V11 Read text** (502.3.8, 11.5.2.10): the viewer gained Back / Forward and a formatted Markdown view. Re-read a
  `.txt`, then open a `.md` and read it with VO-A (the guide has no Markdown step yet; add one).

- **Nested controls (#824, new)** (2.1.1, 4.1.2, 502.3.1): in Settings > Keyboard shortcuts, Tab from a pill to its ×,
  hear "Remove shortcut, button", press Space, and land on the row's add button. In Search's Size filter, pick `Custom…`
  and hear a number field with its name. On a Search result, hear the path in the row's reading, without any "button".
- **Servers view lists (new)** (4.1.2): hear "list" with an item count on the host list and a host's share list.

Not changed, no re-hear needed: V7, V8, V10, and V12.

### After 0.57.0 (2026-10-31, on `main`)

`report.version` still says 0.57.0, but #824's and #817's level changes describe the next release, so the page is ahead
of the shipped build until then:

- **2.1.1, 4.1.2, 502.3.1, 11.5.2.5**: Partially supports → Supports. 302.1 and 4.2.1 lose #824 and stay "Partially
  supports" for #451.
- **2.5.8, 302.7, 4.2.7**: Partially supports → Supports (#817).

## How to re-evaluate

Every two months or so, and before handing the report to a buyer whose evaluation is due. A per-release trust review
(`docs/guides/trust-page-review.md`) only flags claims the code clearly contradicts; this is the full pass. One agent
does it in a session; David's part is the re-hear.

1. **Pick the released tags**: `OLD` is `report.version` in `apps/website/src/lib/accessibility-report.ts`, `NEW` the
   latest release tag. Every claim describes `NEW`, never `main`.
2. **Close the gap list**: `gh issue view` each `gapIssues` entry and each row's `issues`. A closed issue counts only if
   its fix is in `NEW` (`git merge-base --is-ancestor <fix> NEW`). Lift the rows it held, unless another issue holds
   them.
3. **Find what moved**:
   `git log --oneline OLD..NEW -G'aria-|role=|tabindex|prefers-reduced-motion|focus-visible|announce' -- 'apps/desktop/src/**'`,
   plus `git diff --stat OLD NEW` over every evidence path this doc cites. Read each hit against the rows it touches.
   Watch for new surfaces (a new dialog, view, or window), new drags, new small click targets, new single-key shortcuts,
   new time limits, and new animations.
4. **Sweep the opt-outs** at `NEW`: `git grep` for `skipLabelInName:`, `nested-interactive`, `it.skip` in
   `*.a11y.test.ts`, and `eslint-disable` lines naming an a11y rule. Each is a known gap; make sure a row and an issue
   count it.
5. **Spot-check the steady rows**: window minimum size (`tauri.conf.json`), zoom range (`TEXT_SIZE_RANGE`), notification
   timeouts (`git grep -E "timeoutMs: *[0-9]+"`), Help menu (`menu_bar.rs`), shipped languages, and the website's axe
   page list against `apps/website/src/pages/`. Run `pnpm check a11y-contrast`.
6. **File what's new**: a real gap gets an issue (`docs/tooling/backlog-board.md`, `claude-created`, board fields) and
   goes into the row's `issues` and `gapIssues`.
7. **Update the report and this doc together**: remarks stay plain, for buyers, no file paths; evidence and commits go
   here. Bump `report.version` to `NEW` and `report.date` to today. Add a dated "Re-evaluation" section above, with the
   level changes, and a dated "Re-hear" list naming the V-checks whose code changed (David's VoiceOver guide, vault
   `projects/Cmdr/2026-10-14 Cmdr VoiceOver test guide.md`). Keep a VoiceOver-backed remark's wording unless the code
   clearly contradicts it. Once David re-hears, record the result in § VoiceOver listening runs and drop the list.
8. **Check and commit**: `pnpm check --fast`, the website checks (`website-typecheck`, `website-eslint`,
   `website-build website-e2e`), then commit. The page goes live when it lands, so land it only for claims true of
   `NEW`.
