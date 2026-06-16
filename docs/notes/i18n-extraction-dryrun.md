# i18n extraction dry-run (M1 analysis artifact)

> One-shot heuristic scan, NOT a shipped catalog. Regenerate with
> `node apps/desktop/scripts/extract-user-facing-strings.js`.

Candidate user-facing string literals found in the closed sink set (`addToast` content,
`title`/`aria-label`/`label`/`placeholder` props, `.svelte` text nodes). This is a LOWER BOUND on the real string count
(see "What this misses").

## Total: 836 candidate strings across 26 areas

Candidates per area (a 2-column table would trip `docs-table-hygiene`, so this is a list):

- `(main)`: 4
- `ai`: 10
- `command-palette`: 6
- `crash-reporter`: 5
- `debug`: 122
- `dev`: 147
- `downloads`: 22
- `error-reporter`: 13
- `feedback`: 5
- `file-explorer`: 106
- `file-operations`: 34
- `go-to-path`: 10
- `indexing`: 1
- `licensing`: 28
- `low-disk-space`: 1
- `mtp`: 8
- `notifications`: 1
- `onboarding`: 86
- `query-ui`: 47
- `search`: 1
- `settings`: 117
- `shortcuts`: 6
- `ui`: 12
- `updates`: 4
- `viewer`: 35
- `whats-new`: 5

## What this heuristic MISSES (so the total isn't over-claimed)

- **Dynamic / concatenated strings** (`'Copied ' + n + ' files'`) and **template literals with expressions** (`\`Moved
  ${n}\``): not captured. These are exactly the multi-variable cases that need ICU `plural`/`select` — they must be
  found by reading each area during its M2 tranche, not by this scan.
- **Imperatively-set copy**: `element.title = ...`, `setAttribute('aria-label', ...)`, document/window `<title>`s.
- **Composed strings returned from helpers** (the transfer toast was one): the literal is born in a function, far from
  its `addToast` display site.
- **Native menu labels** built in Rust (`muda`): not frontend literals at all (deferred surface, Open decision 5).
- **Already-migrated copy** (`t()` / `<Trans>`): correctly NOT counted.

## Multi-variable / rich-text candidates (need ICU or `<Trans>`)

Heuristic flag: a captured literal containing `{` (interpolation), a digit (likely a count), or a `<tag>` (inline
component). Verify by hand per tranche.

Count: 28

- `ai` lib/ai/AiToastContent.svelte:39 (text): Try creating a new folder (F7) to see AI-powered name suggestions.
- `dev` routes/dev/components/sections/ComboboxSection.svelte:33 (attr:placeholder): Example: gpt-4o
- `dev` routes/dev/components/sections/Dialogs.svelte:125 (text): Backdrop uses `backdrop-filter: blur(4px)`.
- `dev` routes/dev/components/sections/Toasts.svelte:44 (addToast): Hover me to pause; leaving past expiry gives a
  2-second grace.
- `dev` routes/dev/components/sections/Toasts.svelte:87 (text): Burst of 6 grouped toasts
- `dev` routes/dev/graphics/sections/SpinnersSection.svelte:23 (text): sm (12px)
- `dev` routes/dev/graphics/sections/SpinnersSection.svelte:29 (text): md (24px)
- `dev` routes/dev/graphics/sections/SpinnersSection.svelte:35 (text): lg (32px)
- `file-explorer` lib/file-explorer/network/ConnectToServerDialog.svelte:85 (text): Examples: mynas.local,
  192.168.1.100, smb://server/share
- `file-explorer` lib/file-explorer/pane/FunctionKeyBar.svelte:109 (attr:aria-label): {fnKey} (no shift action)
- `file-explorer` lib/file-explorer/pane/TypeToJumpIndicator.svelte:27 (attr:aria-label): Jump to {buffer}
- `file-explorer` lib/file-explorer/pane/clipboard-operations.ts:134 (addToast): Use F5 to copy files from MTP devices
- `file-explorer` lib/file-explorer/pane/clipboard-operations.ts:171 (addToast): Use F6 to move files from MTP devices
- `file-explorer` lib/file-explorer/pane/clipboard-operations.ts:201 (addToast): Use F5 to copy files to MTP devices
- `file-explorer` lib/file-explorer/tabs/TabBar.svelte:91 (attr:aria-label): {paneId} pane tabs
- `licensing` lib/licensing/AboutWindow.svelte:118 (text): AI powered by Falcon-H1R-7B by Technology Innovation
  Institute (TII)
- `licensing` lib/licensing/CommercialReminderModal.svelte:37 (text): Commercial licenses are $59/year/user and support
  continued development.
- `licensing` lib/licensing/LicenseKeyDialog.svelte:406 (attr:placeholder): Example: CMDR-ABCD-EFGH-1234
- `onboarding` lib/onboarding/CloudProviderSetup.svelte:405 (attr:placeholder): Example: https://api.example.com/v1
- `settings` lib/settings/components/SettingColorSwatchPicker.svelte:125 (attr:aria-label): Choose a tint color for
  {label}
- `settings` lib/settings/components/SettingNumberInput.svelte:46 (attr:aria-label): Decrease {label}
- `settings` lib/settings/components/SettingNumberInput.svelte:50 (attr:aria-label): Increase {label}
- `settings` lib/settings/sections/AdvancedSection.svelte:177 (attr:aria-label): Decrease {setting.label}
- `settings` lib/settings/sections/AdvancedSection.svelte:181 (attr:aria-label): Increase {setting.label}
- `settings` lib/settings/sections/AiCloudSection.svelte:392 (attr:placeholder): Example: https://api.example.com/v1
- `ui` lib/ui/ShortcutChip.svelte:79 (attr:aria-label): Customize the {commandName} shortcut
- `viewer` routes/viewer/+page.svelte:1029 (attr:aria-label): File content: {fileName}
- `viewer` routes/viewer/ViewerStatusBar.svelte:97 (text): Click 100% / fit &middot; Scroll zoom &middot; Drag pan

## Full candidate list by area

### `(main)` (4)

- routes/(main)/+page.svelte:725 (text): Cmdr
- routes/(main)/command-handlers/tab-handlers.ts:14 (addToast): Tab limit reached
- routes/(main)/command-handlers/tab-handlers.ts:29 (addToast): No recently closed tabs in this pane.
- routes/(main)/command-handlers/tab-handlers.ts:31 (addToast): Tab limit reached

### `ai` (10)

- lib/ai/AiToastContent.svelte:10 (text): Downloading AI model...
- lib/ai/AiToastContent.svelte:25 (text): Starting download...
- lib/ai/AiToastContent.svelte:29 (text): Cancel
- lib/ai/AiToastContent.svelte:33 (text): Setting up AI...
- lib/ai/AiToastContent.svelte:34 (text): Starting server
- lib/ai/AiToastContent.svelte:38 (text): AI ready
- lib/ai/AiToastContent.svelte:39 (text): Try creating a new folder (F7) to see AI-powered name suggestions.
- lib/ai/AiToastContent.svelte:47 (text): Got it
- lib/ai/AiToastContent.svelte:52 (text): AI starting...
- lib/ai/AiToastContent.svelte:53 (text): Loading the model, this takes a few seconds

### `command-palette` (6)

- lib/command-palette/CommandPalette.svelte:202 (attr:placeholder): Search commands...
- lib/command-palette/CommandPalette.svelte:204 (attr:aria-label): Search commands
- lib/command-palette/CommandPalette.svelte:217 (text): No commands found
- lib/command-palette/CommandPalette.svelte:224 (attr:aria-label): Commands
- lib/command-palette/CommandPalette.svelte:230 (text): Recent
- lib/command-palette/CommandPalette.svelte:233 (text): All commands

### `crash-reporter` (5)

- lib/crash-reporter/CrashReportDialog.svelte:98 (text): Mention this if you reach out about the issue.
- lib/crash-reporter/CrashReportDialog.svelte:120 (text): Always send crash reports
- lib/crash-reporter/CrashReportDialog.svelte:133 (text): Dismiss
- lib/crash-reporter/CrashReportToastContent.svelte:13 (text): Crash report sent. Thanks for helping improve Cmdr.
- lib/crash-reporter/CrashReportToastContent.svelte:15 (text): Change in Settings &gt; Updates

### `debug` (122)

- routes/debug/+page.svelte:175 (text): Debug
- routes/debug/+page.svelte:185 (text): Debug
- routes/debug/+page.svelte:186 (attr:aria-label): Debug sections
- routes/debug/DebugAppearancePanel.svelte:34 (text): Appearance
- routes/debug/DebugAppearancePanel.svelte:36 (text): Dark mode
- routes/debug/DebugClosedTabsPanel.svelte:55 (text): Closed tabs
- routes/debug/DebugClosedTabsPanel.svelte:58 (text): Left pane
- routes/debug/DebugClosedTabsPanel.svelte:73 (text): No recently closed tabs
- routes/debug/DebugClosedTabsPanel.svelte:77 (text): Right pane
- routes/debug/DebugClosedTabsPanel.svelte:92 (text): No recently closed tabs
- routes/debug/DebugDriveIndexPanel.svelte:218 (text): Drive index
- routes/debug/DebugDriveIndexPanel.svelte:224 (text): Loading...
- routes/debug/DebugDriveIndexPanel.svelte:257 (text): Watcher on
- routes/debug/DebugDriveIndexPanel.svelte:261 (text): Watcher off
- routes/debug/DebugDriveIndexPanel.svelte:279 (text): Phase timeline
- routes/debug/DebugDriveIndexPanel.svelte:282 (text): No phase history
- routes/debug/DebugDriveIndexPanel.svelte:320 (text): Start scan
- routes/debug/DebugDriveIndexPanel.svelte:321 (text): Clear index
- routes/debug/DebugDriveIndexPanel.svelte:329 (text): Database
- routes/debug/DebugDriveIndexPanel.svelte:333 (text): Entries
- routes/debug/DebugDriveIndexPanel.svelte:342 (text): Directories
- routes/debug/DebugDriveIndexPanel.svelte:351 (text): Dirs with stats
- routes/debug/DebugDriveIndexPanel.svelte:363 (text): Dirs missing stats
- routes/debug/DebugDriveIndexPanel.svelte:378 (text): Last scan
- routes/debug/DebugDriveIndexPanel.svelte:391 (text): Scan duration
- routes/debug/DebugDriveIndexPanel.svelte:404 (text): DB size
- routes/debug/DebugDriveIndexPanel.svelte:426 (text): DB pages
- routes/debug/DebugDriveIndexPanel.svelte:451 (text): Event statistics
- routes/debug/DebugDriveIndexPanel.svelte:455 (text): Live FS events
- routes/debug/DebugDriveIndexPanel.svelte:466 (text): MustScanSubDirs
- routes/debug/DebugDriveIndexPanel.svelte:477 (text): Rescans completed
- routes/debug/DebugErrorPreviewPanel.svelte:179 (text): Error pane preview
- routes/debug/DebugErrorPreviewPanel.svelte:182 (text): Reset both panes
- routes/debug/DebugErrorPreviewPanel.svelte:185 (text): Transient (errno)
- routes/debug/DebugErrorPreviewPanel.svelte:205 (text): Needs action (errno)
- routes/debug/DebugErrorPreviewPanel.svelte:225 (text): Serious (errno)
- routes/debug/DebugErrorPreviewPanel.svelte:245 (text): VolumeError variants
- routes/debug/DebugErrorPreviewPanel.svelte:266 (text): Reset both panes
- routes/debug/DebugErrorPreviewPanel.svelte:272 (text): Transfer error dialog (modal)
- routes/debug/DebugErrorPreviewPanel.svelte:284 (text): Open
- routes/debug/DebugHistoryPanel.svelte:60 (text): Navigation history
- routes/debug/DebugHistoryPanel.svelte:63 (text): Left pane
- routes/debug/DebugHistoryPanel.svelte:79 (text): No history yet
- routes/debug/DebugHistoryPanel.svelte:83 (text): Right pane
- routes/debug/DebugHistoryPanel.svelte:99 (text): No history yet
- routes/debug/DebugSmbDiagnosticsPanel.svelte:163 (text): SMB diagnostics
- routes/debug/DebugSmbDiagnosticsPanel.svelte:167 (text): Volume
- routes/debug/DebugSmbDiagnosticsPanel.svelte:169 (text): No SMB volumes mounted
- routes/debug/DebugSmbDiagnosticsPanel.svelte:183 (text): Auto-refresh
- routes/debug/DebugSmbDiagnosticsPanel.svelte:197 (text): Refresh volumes
- routes/debug/DebugSmbDiagnosticsPanel.svelte:198 (text): Snapshot now
- routes/debug/DebugSmbDiagnosticsPanel.svelte:260 (text): Flow
- routes/debug/DebugSmbDiagnosticsPanel.svelte:263 (text): Credits available
- routes/debug/DebugSmbDiagnosticsPanel.svelte:273 (text): In flight
- routes/debug/DebugSmbDiagnosticsPanel.svelte:283 (text): Next message id
- routes/debug/DebugSmbDiagnosticsPanel.svelte:296 (text): Wire traffic
- routes/debug/DebugSmbDiagnosticsPanel.svelte:299 (text): Sent
- routes/debug/DebugSmbDiagnosticsPanel.svelte:311 (text): Received
- routes/debug/DebugSmbDiagnosticsPanel.svelte:326 (text): Requests
- routes/debug/DebugSmbDiagnosticsPanel.svelte:329 (text): Sent
- routes/debug/DebugSmbDiagnosticsPanel.svelte:339 (text): Compound chains
- routes/debug/DebugSmbDiagnosticsPanel.svelte:349 (text): Returned err
- routes/debug/DebugSmbDiagnosticsPanel.svelte:359 (text): Explicit cancels
- routes/debug/DebugSmbDiagnosticsPanel.svelte:372 (text): Responses
- routes/debug/DebugSmbDiagnosticsPanel.svelte:375 (text): Routed ok
- routes/debug/DebugSmbDiagnosticsPanel.svelte:385 (text): Wire err
- routes/debug/DebugSmbDiagnosticsPanel.svelte:395 (text): Late (caller dropped)
- routes/debug/DebugSmbDiagnosticsPanel.svelte:405 (text): Stray
- routes/debug/DebugSmbDiagnosticsPanel.svelte:418 (text): Protocol events
- routes/debug/DebugSmbDiagnosticsPanel.svelte:421 (text): STATUS_PENDING
- routes/debug/DebugSmbDiagnosticsPanel.svelte:431 (text): Unsolicited
- routes/debug/DebugSmbDiagnosticsPanel.svelte:444 (text): Errors
- routes/debug/DebugSmbDiagnosticsPanel.svelte:447 (text): Signature
- routes/debug/DebugSmbDiagnosticsPanel.svelte:459 (text): Decrypt
- routes/debug/DebugSmbDiagnosticsPanel.svelte:471 (text): Decompress
- routes/debug/DebugSmbDiagnosticsPanel.svelte:483 (text): Malformed
- routes/debug/DebugSmbDiagnosticsPanel.svelte:495 (text): Session expired
- routes/debug/DebugSmbDiagnosticsPanel.svelte:511 (text): Session
- routes/debug/DebugSmbDiagnosticsPanel.svelte:513 (text): Session id
- routes/debug/DebugSmbDiagnosticsPanel.svelte:515 (text): Should sign
- routes/debug/DebugSmbDiagnosticsPanel.svelte:517 (text): Should encrypt
- routes/debug/DebugSmbDiagnosticsPanel.svelte:519 (text): Algorithm
- routes/debug/DebugSmbDiagnosticsPanel.svelte:529 (text): Negotiated
- routes/debug/DebugSmbDiagnosticsPanel.svelte:531 (text): Max read
- routes/debug/DebugSmbDiagnosticsPanel.svelte:533 (text): Max write
- routes/debug/DebugSmbDiagnosticsPanel.svelte:535 (text): Max transact
- routes/debug/DebugSmbDiagnosticsPanel.svelte:538 (text): GMAC
- routes/debug/DebugSmbDiagnosticsPanel.svelte:546 (text): Server GUID
- routes/debug/DebugSmbDiagnosticsPanel.svelte:558 (text): Client
- routes/debug/DebugSmbDiagnosticsPanel.svelte:561 (text): Reconnects
- routes/debug/DebugSmbDiagnosticsPanel.svelte:571 (text): DFS referrals resolved
- routes/debug/DebugSmbDiagnosticsPanel.svelte:581 (text): DFS cache hits
- routes/debug/DebugSmbDiagnosticsPanel.svelte:590 (text): Auto-reconnect
- routes/debug/DebugSmbDiagnosticsPanel.svelte:593 (text): DFS
- routes/debug/DebugSmbDiagnosticsPanel.svelte:629 (text): Credits
- routes/debug/DebugSmbDiagnosticsPanel.svelte:633 (text): Requests sent
- routes/debug/DebugSmbDiagnosticsPanel.svelte:635 (text): Routed ok
- routes/debug/DebugSmbDiagnosticsPanel.svelte:637 (text): Wire bytes
- routes/debug/DebugSmbDiagnosticsPanel.svelte:650 (text): Pick a different volume above, or open an SMB share first.
- routes/debug/DebugSmbDiagnosticsPanel.svelte:654 (text): No SMB volumes are mounted right now.
- routes/debug/DebugSmbDiagnosticsPanel.svelte:655 (text): Open one in the main window — this dashboard will pick it up
  on the next refresh.
- routes/debug/DebugSmbDiagnosticsPanel.svelte:659 (text): Loading…
- routes/debug/DebugToastPanel.svelte:9 (text): Toast notifications
- routes/debug/DebugToastPanel.svelte:12 (text): Transient
- routes/debug/DebugToastPanel.svelte:18 (text): Default
- routes/debug/DebugToastPanel.svelte:25 (text): Info
- routes/debug/DebugToastPanel.svelte:32 (text): Success
- routes/debug/DebugToastPanel.svelte:39 (text): Warn
- routes/debug/DebugToastPanel.svelte:46 (text): Error
- routes/debug/DebugToastPanel.svelte:50 (text): Persistent
- routes/debug/DebugToastPanel.svelte:56 (text): Default
- routes/debug/DebugToastPanel.svelte:66 (text): Info
- routes/debug/DebugToastPanel.svelte:76 (text): Success
- routes/debug/DebugToastPanel.svelte:86 (text): Warn
- routes/debug/DebugToastPanel.svelte:96 (text): Error
- routes/debug/DebugToastPanel.svelte:100 (text): Dedup
- routes/debug/DebugToastPanel.svelte:106 (text): Replace (same ID)
- routes/debug/DebugToastPanel.svelte:110 (text): Custom timeout
- routes/debug/DebugToastPanel.svelte:120 (text): Bulk actions
- routes/debug/DebugToastPanel.svelte:121 (text): Dismiss transient
- routes/debug/DebugToastPanel.svelte:122 (text): Clear all
- routes/debug/DebugToastPanel.svelte:125 (text): Active

### `dev` (147)

- routes/dev/components/+page.svelte:147 (text): Components
- routes/dev/components/+page.svelte:149 (text): lib/ui
- routes/dev/components/+page.svelte:150 (text): lib/ui/CLAUDE.md
- routes/dev/components/+page.svelte:154 (text): Open in browser ↗
- routes/dev/components/sections/Buttons.svelte:24 (attr:label): Buttons
- routes/dev/components/sections/ChipSection.svelte:9 (attr:label): Chip
- routes/dev/components/sections/ChipSection.svelte:12 (text): Filter variant &mdash; default, configured (with ×
  clear), open, disabled
- routes/dev/components/sections/ChipSection.svelte:14 (attr:label): Modified
- routes/dev/components/sections/ChipSection.svelte:16 (attr:label): Size
- routes/dev/components/sections/ChipSection.svelte:27 (attr:label): Search in
- routes/dev/components/sections/ChipSection.svelte:28 (attr:label): Pattern
- routes/dev/components/sections/ChipSection.svelte:33 (text): Filter variant &mdash; highlighted (AI just populated it)
- routes/dev/components/sections/ChipSection.svelte:35 (attr:label): Size
- routes/dev/components/sections/ChipSection.svelte:40 (text): Recent variant &mdash; mode badge + truncating label
- routes/dev/components/sections/ChipSection.svelte:42 (attr:label): \*.jpg
- routes/dev/components/sections/ChipSection.svelte:43 (text): Aa
- routes/dev/components/sections/ChipSection.svelte:45 (attr:label): all my very long screenshot file names from last
  week
- routes/dev/components/sections/ChipSection.svelte:46 (text): AI
- routes/dev/components/sections/ChipSection.svelte:48 (attr:label): ^temp.\*\.log$
- routes/dev/components/sections/ChipSection.svelte:51 (attr:label): \*.bak
- routes/dev/components/sections/ChipSection.svelte:52 (text): Aa
- routes/dev/components/sections/ComboboxSection.svelte:22 (attr:label): Combobox
- routes/dev/components/sections/ComboboxSection.svelte:25 (text): Text field with suggestions. Pick one or type your
  own (free text persists).
- routes/dev/components/sections/ComboboxSection.svelte:33 (attr:placeholder): Example: gpt-4o
- routes/dev/components/sections/ComboboxSection.svelte:50 (attr:placeholder): Type a model name
- routes/dev/components/sections/ComboboxSection.svelte:57 (text): Loading: in-field spinner while the suggestions
  fetch.
- routes/dev/components/sections/ComboboxSection.svelte:66 (attr:placeholder): Loading models…
- routes/dev/components/sections/ComboboxSection.svelte:73 (text): Disabled.
- routes/dev/components/sections/CommandBoxSection.svelte:10 (attr:label): CommandBox
- routes/dev/components/sections/DateLabelSection.svelte:40 (attr:label): Date label
- routes/dev/components/sections/Dialogs.svelte:12 (attr:label): Dialogs
- routes/dev/components/sections/Dialogs.svelte:15 (text): Modal dialog (default and blurred overlay)
- routes/dev/components/sections/Dialogs.svelte:19 (text): Confirm rename
- routes/dev/components/sections/Dialogs.svelte:22 (text): Rename "draft.md" to "final.md"?
- routes/dev/components/sections/Dialogs.svelte:24 (text): Cancel
- routes/dev/components/sections/Dialogs.svelte:25 (text): Rename
- routes/dev/components/sections/Dialogs.svelte:32 (text): Confirm rename
- routes/dev/components/sections/Dialogs.svelte:35 (text): With blur overlay (real overlay is portal-mounted).
- routes/dev/components/sections/Dialogs.svelte:37 (text): Cancel
- routes/dev/components/sections/Dialogs.svelte:38 (text): Rename
- routes/dev/components/sections/Dialogs.svelte:62 (text): Alert dialog (single action)
- routes/dev/components/sections/Dialogs.svelte:66 (text): Couldn't save settings
- routes/dev/components/sections/Dialogs.svelte:69 (text): The settings file is read-only. Check folder permissions.
- routes/dev/components/sections/Dialogs.svelte:71 (text): OK
- routes/dev/components/sections/Dialogs.svelte:99 (text): This is the real ModalDialog mounted on demand.
- routes/dev/components/sections/Dialogs.svelte:125 (text): Backdrop uses `backdrop-filter: blur(4px)`.
- routes/dev/components/sections/Dialogs.svelte:142 (attr:title): Catalog preview
- routes/dev/components/sections/EmptyStates.svelte:5 (attr:label): Empty states
- routes/dev/components/sections/EmptyStates.svelte:6 (text): Empty folder
- routes/dev/components/sections/FilterPopoverSection.svelte:9 (attr:label): Filter popover
- routes/dev/components/sections/FilterPopoverSection.svelte:32 (attr:label): Size
- routes/dev/components/sections/FilterPopoverSection.svelte:35 (attr:aria-label): Comparator
- routes/dev/components/sections/Groups.svelte:5 (attr:label): Groups
- routes/dev/components/sections/Groups.svelte:8 (text): With label
- routes/dev/components/sections/Groups.svelte:10 (attr:label): Theme
- routes/dev/components/sections/Groups.svelte:11 (text): Card body content.
- routes/dev/components/sections/Groups.svelte:17 (text): Without label
- routes/dev/components/sections/Groups.svelte:20 (text): Card body content.
- routes/dev/components/sections/Groups.svelte:26 (text): Adjacent cards
- routes/dev/components/sections/Groups.svelte:28 (attr:label): First card
- routes/dev/components/sections/Groups.svelte:29 (text): First card body.
- routes/dev/components/sections/Groups.svelte:31 (attr:label): Second card
- routes/dev/components/sections/Groups.svelte:32 (text): Second card body.
- routes/dev/components/sections/Links.svelte:24 (attr:label): Links
- routes/dev/components/sections/Links.svelte:27 (text): In-app button
- routes/dev/components/sections/Links.svelte:28 (text): Open settings
- routes/dev/components/sections/Links.svelte:32 (text): External href
- routes/dev/components/sections/Links.svelte:37 (text): Inline in prose
- routes/dev/components/sections/Loading.svelte:6 (attr:label): Loading
- routes/dev/components/sections/Loading.svelte:9 (text): Default
- routes/dev/components/sections/Loading.svelte:15 (text): Opening folder
- routes/dev/components/sections/Loading.svelte:21 (text): Loaded count
- routes/dev/components/sections/Loading.svelte:27 (text): Finalizing count
- routes/dev/components/sections/Loading.svelte:33 (text): With cancel hint
- routes/dev/components/sections/PopoverSection.svelte:9 (attr:label): Popover
- routes/dev/components/sections/PopoverSection.svelte:34 (text): Any content goes here.
- routes/dev/components/sections/PopoverSection.svelte:35 (attr:aria-label): Demo field
- routes/dev/components/sections/PopoverSection.svelte:35 (attr:placeholder): Type here
- routes/dev/components/sections/Progress.svelte:30 (attr:label): Progress
- routes/dev/components/sections/Progress.svelte:45 (text): size md, animated
- routes/dev/components/sections/SelectSection.svelte:22 (attr:label): Select
- routes/dev/components/sections/SelectSection.svelte:25 (text): Flat list with a per-item description. Picks one of a
  fixed set.
- routes/dev/components/sections/SelectSection.svelte:39 (text): Grouped items (Ark item groups), for example the viewer
  encoding picker.
- routes/dev/components/sections/SelectSection.svelte:53 (text): Disabled.
- routes/dev/components/sections/SelectSection.svelte:60 (text): Empty value, showing the placeholder.
- routes/dev/components/sections/SelectSection.svelte:66 (attr:placeholder): Choose a format
- routes/dev/components/sections/ShortcutChip.svelte:6 (attr:label): Shortcut chip
- routes/dev/components/sections/ShortcutChip.svelte:9 (text): Literal keys
- routes/dev/components/sections/ShortcutChip.svelte:18 (text): Command (bound, clickable)
- routes/dev/components/sections/ShortcutChip.svelte:22 (text): Hover for accent border; click opens Settings &gt;
  Keyboard shortcuts
- routes/dev/components/sections/ShortcutChip.svelte:26 (text): Command (bound, non-clickable)
- routes/dev/components/sections/ShortcutChip.svelte:30 (text): For chips nested inside another interactive control
  (palette rows, F-key bar)
- routes/dev/components/sections/ShortcutChip.svelte:34 (text): Dense (size="sm")
- routes/dev/components/sections/ShortcutChip.svelte:40 (text): Tighter padding + radius for dense rows (the command
  palette)
- routes/dev/components/sections/ShortcutChip.svelte:44 (text): Command (unbound)
- routes/dev/components/sections/ShortcutChip.svelte:47 (text): Renders nothing (the command has no binding)
- routes/dev/components/sections/SizeBadges.svelte:9 (attr:label): Size badges
- routes/dev/components/sections/SizeBadges.svelte:12 (text): Normal
- routes/dev/components/sections/SizeBadges.svelte:21 (text): Selected (gold)
- routes/dev/components/sections/StatusBadgeSection.svelte:8 (attr:label): Status badge
- routes/dev/components/sections/StatusBadgeSection.svelte:11 (text): Statuses
- routes/dev/components/sections/StatusBadgeSection.svelte:20 (text): Next to a title
- routes/dev/components/sections/StatusBadgeSection.svelte:21 (text): Search
- routes/dev/components/sections/Toasts.svelte:27 (addToast): Persistent toast (catalog preview).
- routes/dev/components/sections/Toasts.svelte:44 (addToast): Hover me to pause; leaving past expiry gives a 2-second
  grace.
- routes/dev/components/sections/Toasts.svelte:51 (attr:label): Toasts
- routes/dev/components/sections/Toasts.svelte:52 (text): Static previews of each level (left to right: default, info,
  success, warn, error).
- routes/dev/components/sections/Toasts.svelte:68 (text): Trigger a real toast:
- routes/dev/components/sections/Toasts.svelte:87 (text): Burst of 6 grouped toasts
- routes/dev/components/sections/Toasts.svelte:95 (text): Show a hover-pause toast
- routes/dev/components/sections/Toasts.svelte:96 (text): Hover the toast top-right; move away to see the resume or
  grace behavior.
- routes/dev/components/sections/ToggleGroupSection.svelte:36 (attr:label): Toggle group
- routes/dev/components/sections/ToggleGroupSection.svelte:39 (text): Tabs semantics &mdash; badge, hint, disabled with
  tooltip
- routes/dev/components/sections/ToggleGroupSection.svelte:52 (text): Toggles semantics &mdash; plain labels
- routes/dev/components/sections/ToggleGroupSection.svelte:65 (text): Toggles semantics &mdash; with badge and hint
- routes/dev/components/sections/ToggleGroupSection.svelte:78 (text): Toggles semantics &mdash; disabled root
- routes/dev/components/sections/Tooltips.svelte:8 (attr:label): Tooltips
- routes/dev/components/sections/Tooltips.svelte:11 (text): Plain text
- routes/dev/components/sections/Tooltips.svelte:12 (text): Hover me
- routes/dev/components/sections/Tooltips.svelte:16 (text): With shortcut
- routes/dev/components/sections/Tooltips.svelte:17 (text): Hover me
- routes/dev/components/sections/Tooltips.svelte:21 (text): Rich HTML
- routes/dev/components/sections/Tooltips.svelte:24 (text): Line two with
- routes/dev/components/sections/Tooltips.svelte:31 (text): Overflow only
- routes/dev/graphics/+page.svelte:111 (text): Graphics
- routes/dev/graphics/+page.svelte:119 (text): Open in browser ↗
- routes/dev/graphics/sections/AnimationsSection.svelte:19 (attr:label): Animations
- routes/dev/graphics/sections/AnimationsSection.svelte:62 (text): No live demo
- routes/dev/graphics/sections/AnimationsSection.svelte:64 (text): fadeIn
- routes/dev/graphics/sections/AnimationsSection.svelte:69 (text): No live demo
- routes/dev/graphics/sections/AnimationsSection.svelte:76 (text): See Illustrations
- routes/dev/graphics/sections/AnimationsSection.svelte:78 (text): keyboard demo
- routes/dev/graphics/sections/IconsSection.svelte:42 (attr:label): Icons
- routes/dev/graphics/sections/IconsSection.svelte:44 (text): Icon
- routes/dev/graphics/sections/IconsSection.svelte:45 (text): lib/ui/icons/icon-map.ts
- routes/dev/graphics/sections/IconsSection.svelte:45 (text): currentColor
- routes/dev/graphics/sections/IllustrationsSection.svelte:10 (attr:label): Illustrations
- routes/dev/graphics/sections/IllustrationsSection.svelte:19 (text): Global shortcut keyboard
- routes/dev/graphics/sections/IllustrationsSection.svelte:25 (text): Empty network
- routes/dev/graphics/sections/SpinnersSection.svelte:13 (attr:label): Spinners
- routes/dev/graphics/sections/SpinnersSection.svelte:15 (text): Spinner
- routes/dev/graphics/sections/SpinnersSection.svelte:23 (text): sm (12px)
- routes/dev/graphics/sections/SpinnersSection.svelte:29 (text): md (24px)
- routes/dev/graphics/sections/SpinnersSection.svelte:35 (text): lg (32px)
- routes/dev/graphics/sections/StatusBadgesSection.svelte:40 (attr:label): Status badges
- routes/dev/graphics/sections/StatusBadgesSection.svelte:43 (text): currentColor
- routes/dev/graphics/sections/StatusBadgesSection.svelte:43 (text): Icon

### `downloads` (22)

- lib/downloads/DownloadToastContent.svelte:172 (text): in-app,
- lib/downloads/DownloadToastContent.svelte:181 (attr:aria-label): Show the shortcut tip
- lib/downloads/DownloadToastContent.svelte:194 (text): Something cool to learn about jumping to downloads
- lib/downloads/DownloadToastContent.svelte:196 (text): In-app: Press
- lib/downloads/DownloadToastContent.svelte:196 (text): to jump here
- lib/downloads/DownloadToastContent.svelte:199 (text): In
- lib/downloads/DownloadToastContent.svelte:199 (text): app (global shortcut), press
- lib/downloads/DownloadToastContent.svelte:209 (attr:aria-label): Make this notification more compact
- lib/downloads/DownloadToastContent.svelte:220 (text): Stop showing these
- lib/downloads/DownloadToastContent.svelte:221 (text): Jump to file
- lib/downloads/GlobalShortcutAnimation.svelte:46 (text): caps lock
- lib/downloads/GlobalShortcutRow.svelte:129 (text): Global
- lib/downloads/GlobalShortcutRow.svelte:135 (text): Go to latest download
- lib/downloads/GlobalShortcutRow.svelte:156 (attr:aria-label): Reset to default
- lib/downloads/GlobalShortcutWarnToastContent.svelte:58 (text): Turn it off
- lib/downloads/GlobalShortcutWarnToastContent.svelte:59 (text): Keep it on
- lib/downloads/LatestDownloadEmptyToastContent.svelte:36 (text): Your Downloads folder is empty. Go there anyway?
- lib/downloads/LatestDownloadEmptyToastContent.svelte:38 (text): Dismiss
- lib/downloads/LatestDownloadEmptyToastContent.svelte:39 (text): Go to Downloads
- lib/downloads/LatestDownloadFdaToastContent.svelte:18 (text): Cmdr needs Full Disk Access to watch your Downloads
  folder.
- lib/downloads/LatestDownloadFdaToastContent.svelte:20 (text): Dismiss
- lib/downloads/LatestDownloadFdaToastContent.svelte:21 (text): Open System Settings

### `error-reporter` (13)

- lib/error-reporter/AutoSendToastContent.svelte:25 (text): Error report sent
- lib/error-reporter/AutoSendToastContent.svelte:31 (text): Change settings
- lib/error-reporter/AutoSendToastContent.svelte:32 (text): View
- lib/error-reporter/BundleSavedToastContent.svelte:22 (text): Saved bundle to disk
- lib/error-reporter/BundleSavedToastContent.svelte:25 (text): Dismiss
- lib/error-reporter/BundleSavedToastContent.svelte:26 (text): Reveal in Finder
- lib/error-reporter/ErrorReportDialog.svelte:192 (text): Reference ID:
- lib/error-reporter/ErrorReportDialog.svelte:201 (text): Add a note (optional)
- lib/error-reporter/ErrorReportDialog.svelte:214 (attr:placeholder): What were you trying to do? What did you expect to
  happen?
- lib/error-reporter/ErrorReportDialog.svelte:244 (text): Manifest
- lib/error-reporter/ErrorReportDialog.svelte:266 (text): Preparing preview…
- lib/error-reporter/ErrorReportDialog.svelte:279 (text): Cancel
- lib/error-reporter/ErrorReportToastContent.svelte:26 (text): Dismiss

### `feedback` (5)

- lib/feedback/FeedbackDialog.svelte:57 (addToast): Thanks for the feedback! We read every note.
- lib/feedback/FeedbackDialog.svelte:116 (text): Your feedback
- lib/feedback/FeedbackDialog.svelte:151 (text): browse and vote on GitHub
- lib/feedback/FeedbackDialog.svelte:157 (text): book a call
- lib/feedback/FeedbackDialog.svelte:167 (text): Cancel

### `file-explorer` (106)

- lib/file-explorer/navigation/VolumeBreadcrumb.svelte:465 (addToast): Connecting directly...
- lib/file-explorer/navigation/VolumeBreadcrumb.svelte:472 (addToast): Connected directly for faster access
- lib/file-explorer/navigation/VolumeBreadcrumb.svelte:506 (addToast): Connecting with the saved password...
- lib/file-explorer/navigation/VolumeBreadcrumb.svelte:511 (addToast): Connected directly for faster access
- lib/file-explorer/navigation/VolumeBreadcrumb.svelte:771 (attr:aria-label): Rename favorite
- lib/file-explorer/navigation/VolumeBreadcrumb.svelte:843 (text): Retrying
- lib/file-explorer/navigation/VolumeBreadcrumb.svelte:862 (text): Unavailable
- lib/file-explorer/network/ConnectToServerDialog.svelte:77 (attr:aria-label): Server address
- lib/file-explorer/network/ConnectToServerDialog.svelte:82 (attr:placeholder): hostname, IP address, or smb:// URL
- lib/file-explorer/network/ConnectToServerDialog.svelte:85 (text): Examples: mynas.local, 192.168.1.100,
  smb://server/share
- lib/file-explorer/network/ConnectToServerDialog.svelte:92 (text): Cancel
- lib/file-explorer/network/NetworkBrowser.svelte:548 (text): Name
- lib/file-explorer/network/NetworkBrowser.svelte:549 (text): IP address
- lib/file-explorer/network/NetworkBrowser.svelte:550 (text): Hostname
- lib/file-explorer/network/NetworkBrowser.svelte:551 (text): Shares
- lib/file-explorer/network/NetworkBrowser.svelte:552 (text): Status
- lib/file-explorer/network/NetworkBrowser.svelte:617 (text): Connect to server...
- lib/file-explorer/network/NetworkBrowser.svelte:624 (text): No network hosts found
- lib/file-explorer/network/NetworkBrowser.svelte:625 (text): Make sure you're on a network with SMB-capable devices.
- lib/file-explorer/network/NetworkBrowser.svelte:626 (text): Refresh
- lib/file-explorer/network/NetworkBrowser.svelte:632 (attr:aria-label): Refresh network hosts
- lib/file-explorer/network/NetworkBrowser.svelte:634 (text): Press
- lib/file-explorer/network/NetworkBrowser.svelte:634 (text): or click here to refresh
- lib/file-explorer/network/NetworkLoginForm.svelte:164 (text): Connection mode
- lib/file-explorer/network/NetworkLoginForm.svelte:174 (text): Connect as guest
- lib/file-explorer/network/NetworkLoginForm.svelte:185 (text): Sign in with credentials
- lib/file-explorer/network/NetworkLoginForm.svelte:192 (text): Username
- lib/file-explorer/network/NetworkLoginForm.svelte:207 (text): Password
- lib/file-explorer/network/NetworkLoginForm.svelte:214 (attr:placeholder): Enter password
- lib/file-explorer/network/NetworkLoginForm.svelte:225 (text): Remember in Keychain
- lib/file-explorer/network/NetworkLoginForm.svelte:230 (text): Cancel
- lib/file-explorer/network/ShareBrowser.svelte:35 (addToast): Credentials stored locally (no system keyring detected)
- lib/file-explorer/network/ShareBrowser.svelte:538 (text): Retry
- lib/file-explorer/network/ShareBrowser.svelte:539 (text): Back
- lib/file-explorer/network/ShareBrowser.svelte:543 (text): Retry
- lib/file-explorer/network/ShareBrowser.svelte:544 (text): Sign in
- lib/file-explorer/network/ShareBrowser.svelte:545 (text): Back
- lib/file-explorer/network/ShareBrowser.svelte:552 (text): No shares available
- lib/file-explorer/network/ShareBrowser.svelte:553 (text): This host has no accessible shares, or authentication is
  needed.
- lib/file-explorer/network/ShareBrowser.svelte:555 (text): Sign in
- lib/file-explorer/network/ShareBrowser.svelte:556 (text): Back
- lib/file-explorer/pane/DualPaneExplorer.svelte:1911 (attr:aria-label): File explorer
- lib/file-explorer/pane/ErrorPane.svelte:111 (text): Try again
- lib/file-explorer/pane/ErrorPane.svelte:122 (text): Technical details
- lib/file-explorer/pane/FilePane.svelte:2258 (addToast): Connected directly for faster access
- lib/file-explorer/pane/FilePane.svelte:2895 (attr:aria-label): Disk usage
- lib/file-explorer/pane/FunctionKeyBar.svelte:109 (attr:aria-label): {fnKey} (no shift action)
- lib/file-explorer/pane/FunctionKeyBar.svelte:118 (attr:aria-label): Function keys
- lib/file-explorer/pane/MtpConnectionView.svelte:165 (text): Connecting to device...
- lib/file-explorer/pane/MtpConnectionView.svelte:171 (text): Try again
- lib/file-explorer/pane/NetworkMountView.svelte:304 (text): Couldn't mount share
- lib/file-explorer/pane/NetworkMountView.svelte:307 (text): Try again
- lib/file-explorer/pane/NetworkMountView.svelte:308 (text): Back
- lib/file-explorer/pane/PaneResizer.svelte:51 (attr:aria-label): Resize panes
- lib/file-explorer/pane/SearchResultsView.svelte:231 (text): Search results no longer available
- lib/file-explorer/pane/SearchResultsView.svelte:232 (text): The result set for this search was cleared. Open a new
  search to start again.
- lib/file-explorer/pane/SmbReconnectingView.svelte:89 (text): Reconnecting to server…
- lib/file-explorer/pane/SmbReconnectingView.svelte:116 (text): Cancel
- lib/file-explorer/pane/SmbReconnectingView.svelte:119 (text): Disconnect
- lib/file-explorer/pane/TypeToJumpIndicator.svelte:27 (attr:aria-label): Jump to {buffer}
- lib/file-explorer/pane/VolumeUnreachableBanner.svelte:50 (text): Disconnect
- lib/file-explorer/pane/VolumeUnreachableBanner.svelte:53 (text): Open home folder
- lib/file-explorer/pane/clipboard-operations.ts:134 (addToast): Use F5 to copy files from MTP devices
- lib/file-explorer/pane/clipboard-operations.ts:171 (addToast): Use F6 to move files from MTP devices
- lib/file-explorer/pane/clipboard-operations.ts:201 (addToast): Use F5 to copy files to MTP devices
- lib/file-explorer/pane/clipboard-operations.ts:219 (addToast): No files on the clipboard. Copy files first with ⌘C.
- lib/file-explorer/pane/navigate.ts:383 (addToast): Tab limit reached
- lib/file-explorer/pane/navigate.ts:719 (addToast): Tab limit reached
- lib/file-explorer/pane/tab-operations.ts:370 (addToast): Tab limit reached
- lib/file-explorer/quick-look/QuickLookHintToastContent.svelte:64 (text): Settings &gt; Keyboard shortcuts
- lib/file-explorer/quick-look/QuickLookHintToastContent.svelte:67 (text): Don't show again
- lib/file-explorer/rename/ExtensionChangeDialog.svelte:50 (text): Always allow extension changes
- lib/file-explorer/rename/RenameConflictDialog.svelte:56 (text): Size
- lib/file-explorer/rename/RenameConflictDialog.svelte:60 (text): Modified
- lib/file-explorer/rename/RenameConflictDialog.svelte:71 (text): Size
- lib/file-explorer/rename/RenameConflictDialog.svelte:77 (text): Modified
- lib/file-explorer/rename/RenameConflictDialog.svelte:96 (text): Overwrite and trash old file
- lib/file-explorer/rename/RenameConflictDialog.svelte:102 (text): Overwrite and delete old file
- lib/file-explorer/rename/RenameConflictDialog.svelte:110 (text): Cancel
- lib/file-explorer/rename/RenameConflictDialog.svelte:116 (text): Continue renaming
- lib/file-explorer/selection/SelectionInfo.svelte:230 (text): Nothing in here.
- lib/file-explorer/selection/SelectionInfo.svelte:244 (attr:aria-label): Size not ready yet
- lib/file-explorer/selection/SelectionInfo.svelte:258 (attr:aria-label): Size updating
- lib/file-explorer/tabs/TabBar.svelte:91 (attr:aria-label): {paneId} pane tabs
- lib/file-explorer/tabs/TabBar.svelte:125 (attr:aria-label): Unreachable
- lib/file-explorer/tabs/TabBar.svelte:129 (attr:aria-label): Pinned
- lib/file-explorer/tabs/TabBar.svelte:151 (attr:aria-label): New tab
- lib/file-explorer/views/BriefList.svelte:813 (attr:aria-label): Sort columns
- lib/file-explorer/views/BriefList.svelte:817 (attr:label): Name
- lib/file-explorer/views/BriefList.svelte:825 (attr:label): Ext
- lib/file-explorer/views/BriefList.svelte:833 (attr:label): Size
- lib/file-explorer/views/BriefList.svelte:841 (attr:label): Modified
- lib/file-explorer/views/BriefList.svelte:849 (attr:label): Created
- lib/file-explorer/views/BriefList.svelte:865 (attr:aria-label): File list
- lib/file-explorer/views/BriefList.svelte:941 (text): Empty folder
- lib/file-explorer/views/FullList.svelte:814 (attr:label): Name
- lib/file-explorer/views/FullList.svelte:822 (attr:label): Ext
- lib/file-explorer/views/FullList.svelte:833 (attr:label): Name
- lib/file-explorer/views/FullList.svelte:840 (attr:title): Git status of each file
- lib/file-explorer/views/FullList.svelte:840 (text): Git
- lib/file-explorer/views/FullList.svelte:846 (attr:label): Ext
- lib/file-explorer/views/FullList.svelte:855 (attr:label): Size
- lib/file-explorer/views/FullList.svelte:863 (attr:label): Modified
- lib/file-explorer/views/FullList.svelte:872 (attr:aria-label): File list
- lib/file-explorer/views/FullList.svelte:1040 (attr:aria-label): Size not ready yet
- lib/file-explorer/views/FullList.svelte:1065 (text): Empty folder

### `file-operations` (34)

- lib/file-operations/delete/DeleteDialog.svelte:249 (text): This volume doesn't support trash.
- lib/file-operations/delete/DeleteDialog.svelte:260 (text): Trash
- lib/file-operations/delete/DeleteDialog.svelte:265 (text): Delete
- lib/file-operations/delete/DeleteDialog.svelte:341 (text): Cancel
- lib/file-operations/mkdir/NewFolderDialog.svelte:236 (text): Create folder in
- lib/file-operations/mkdir/NewFolderDialog.svelte:245 (attr:aria-label): Folder name
- lib/file-operations/mkdir/NewFolderDialog.svelte:250 (attr:placeholder): Example: my-project
- lib/file-operations/mkdir/NewFolderDialog.svelte:266 (text): Refresh listing
- lib/file-operations/mkdir/NewFolderDialog.svelte:267 (text): Dismiss
- lib/file-operations/mkdir/NewFolderDialog.svelte:273 (attr:aria-label): AI suggestions
- lib/file-operations/mkdir/NewFolderDialog.svelte:274 (text): AI suggestions:
- lib/file-operations/mkdir/NewFolderDialog.svelte:303 (text): Cancel
- lib/file-operations/mkdir/NewFolderDialog.svelte:304 (text): OK
- lib/file-operations/mkfile/NewFileDialog.svelte:155 (text): Create file in
- lib/file-operations/mkfile/NewFileDialog.svelte:164 (attr:aria-label): File name
- lib/file-operations/mkfile/NewFileDialog.svelte:169 (attr:placeholder): Example: notes.txt
- lib/file-operations/mkfile/NewFileDialog.svelte:179 (text): Cancel
- lib/file-operations/mkfile/NewFileDialog.svelte:180 (text): OK
- lib/file-operations/transfer/ScanPhaseBody.svelte:32 (text): From:
- lib/file-operations/transfer/TransferDialog.svelte:416 (text): Copy
- lib/file-operations/transfer/TransferDialog.svelte:421 (text): Move
- lib/file-operations/transfer/TransferDialog.svelte:466 (attr:aria-label): Destination path
- lib/file-operations/transfer/TransferDialog.svelte:516 (text): Checking for conflicts...
- lib/file-operations/transfer/TransferDialog.svelte:582 (text): Cancel
- lib/file-operations/transfer/TransferErrorDialog.svelte:98 (attr:aria-label): Technical error details
- lib/file-operations/transfer/TransferErrorDialog.svelte:106 (text): Retry
- lib/file-operations/transfer/TransferErrorDialog.svelte:108 (text): Close
- lib/file-operations/transfer/TransferProgressDialog.svelte:1049 (text): Cancel
- lib/file-operations/transfer/TransferProgressDialog.svelte:1229 (text): Rollback
- lib/file-operations/transfer/TransferProgressDialog.svelte:1302 (text): Size
- lib/file-operations/transfer/TransferProgressDialog.svelte:1353 (text): Cancel
- lib/file-operations/transfer/TransferProgressDialog.svelte:1357 (text): Rolling back...
- lib/file-operations/transfer/TransferProgressDialog.svelte:1363 (text): Rollback
- lib/file-operations/transfer/TransferProgressDialog.svelte:1370 (text): Rollback

### `go-to-path` (10)

- lib/go-to-path/GoToPathAncestorToastContent.svelte:31 (text): doesn't exist, so we took you to
- lib/go-to-path/GoToPathAncestorToastContent.svelte:34 (text): Press
- lib/go-to-path/GoToPathAncestorToastContent.svelte:34 (text): to go back.
- lib/go-to-path/GoToPathDialog.svelte:38 (text): Promise
- lib/go-to-path/GoToPathDialog.svelte:208 (attr:aria-label): Path to go to
- lib/go-to-path/GoToPathDialog.svelte:213 (attr:placeholder): Type or paste a path, e.g. ~/Documents
- lib/go-to-path/GoToPathDialog.svelte:223 (attr:aria-label): Recent paths
- lib/go-to-path/GoToPathDialog.svelte:250 (attr:aria-label): Remove from list
- lib/go-to-path/GoToPathDialog.svelte:262 (text): Cancel
- lib/go-to-path/GoToPathDialog.svelte:263 (text): Go to path

### `indexing` (1)

- lib/indexing/IndexingStatusIndicator.svelte:221 (attr:aria-label): Drive indexing status

### `licensing` (28)

- lib/licensing/AboutWindow.svelte:89 (text): About Cmdr
- lib/licensing/AboutWindow.svelte:98 (text): Cmdr
- lib/licensing/AboutWindow.svelte:99 (text): Keyboard-driven file manager
- lib/licensing/AboutWindow.svelte:109 (text): GitHub
- lib/licensing/AboutWindow.svelte:118 (text): AI powered by Falcon-H1R-7B by Technology Innovation Institute (TII)
- lib/licensing/AboutWindow.svelte:121 (text): Website
- lib/licensing/AboutWindow.svelte:125 (text): Get a license
- lib/licensing/AboutWindow.svelte:130 (text): GitHub
- lib/licensing/AboutWindow.svelte:134 (text): Discord
- lib/licensing/CommercialReminderModal.svelte:34 (text): You're using a Personal license.
- lib/licensing/CommercialReminderModal.svelte:35 (text): If you're using Cmdr at work, please get a Commercial license
  to stay compliant.
- lib/licensing/CommercialReminderModal.svelte:37 (text): Commercial licenses are $59/year/user and support continued
  development.
- lib/licensing/CommercialReminderModal.svelte:43 (text): Get commercial license
- lib/licensing/ExpirationModal.svelte:46 (text): License for:
- lib/licensing/ExpirationModal.svelte:58 (text): Renew license
- lib/licensing/ExpirationModal.svelte:59 (text): Continue in personal mode
- lib/licensing/LicenseKeyDialog.svelte:339 (text): License type
- lib/licensing/LicenseKeyDialog.svelte:345 (text): Organization
- lib/licensing/LicenseKeyDialog.svelte:351 (text): Validity
- lib/licensing/LicenseKeyDialog.svelte:369 (text): License key
- lib/licensing/LicenseKeyDialog.svelte:376 (text): Use a different key
- lib/licensing/LicenseKeyDialog.svelte:377 (text): Close
- lib/licensing/LicenseKeyDialog.svelte:385 (text): Cancel
- lib/licensing/LicenseKeyDialog.svelte:386 (text): Continue
- lib/licensing/LicenseKeyDialog.svelte:395 (text): Get a license
- lib/licensing/LicenseKeyDialog.svelte:406 (attr:placeholder): Example: CMDR-ABCD-EFGH-1234
- lib/licensing/LicenseKeyDialog.svelte:448 (text): Cancel
- lib/licensing/LicenseKeyDialog.svelte:454 (text): Cancel

### `low-disk-space` (1)

- lib/low-disk-space/LowDiskSpaceToastContent.svelte:50 (text): Disable these notifications

### `mtp` (8)

- lib/mtp/MtpConnectedToastContent.svelte:42 (text): Disable MTP...
- lib/mtp/MtpConnectedToastContent.svelte:43 (text): OK
- lib/mtp/MtpPermissionDialog.svelte:39 (text): Run this command in your terminal to install the rules and reload them:
- lib/mtp/MtpPermissionDialog.svelte:45 (text): After running the command, unplug and replug the device, then retry.
- lib/mtp/MtpPermissionDialog.svelte:48 (text): Close
- lib/mtp/MtpPermissionDialog.svelte:49 (text): Retry connection
- lib/mtp/PtpcameradDialog.svelte:69 (text): Close
- lib/mtp/PtpcameradDialog.svelte:70 (text): Retry connection

### `notifications` (1)

- lib/notifications/macos-notification-permission.ts:73 (addToast): macOS notifications are off. Open System Settings to
  allow them.

### `onboarding` (86)

- lib/onboarding/CloudProviderPicker.svelte:131 (attr:aria-label): Cloud AI providers
- lib/onboarding/CloudProviderSetup.svelte:395 (text): Endpoint URL
- lib/onboarding/CloudProviderSetup.svelte:405 (attr:placeholder): Example: https://api.example.com/v1
- lib/onboarding/CloudProviderSetup.svelte:421 (text): Paste your API key
- lib/onboarding/CloudProviderSetup.svelte:432 (text): Checking your key…
- lib/onboarding/CloudProviderSetup.svelte:442 (text): Connected!
- lib/onboarding/CloudProviderSetup.svelte:455 (text): Pick a model
- lib/onboarding/OnboardingWizard.svelte:193 (text): Cmdr onboarding
- lib/onboarding/OnboardingWizard.svelte:194 (attr:aria-label): Onboarding progress
- lib/onboarding/OnboardingWizard.svelte:229 (attr:aria-label): Go to previous step
- lib/onboarding/StepAi.svelte:234 (text): Thanks for granting full disk access! Now, the app can access your disk.
  Great!
- lib/onboarding/StepAi.svelte:273 (text): Welcome to Cmdr!
- lib/onboarding/StepAi.svelte:274 (text): Let's set up AI.
- lib/onboarding/StepAi.svelte:276 (text): Now, let's talk AI
- lib/onboarding/StepAi.svelte:284 (text): Here is how you do common actions with and without AI:
- lib/onboarding/StepAi.svelte:289 (text): Feature
- lib/onboarding/StepAi.svelte:290 (text): Without AI
- lib/onboarding/StepAi.svelte:301 (text): Search
- lib/onboarding/StepAi.svelte:302 (text): You type something like
- lib/onboarding/StepAi.svelte:303 (text): You say "my recent fish-related presentations", agent sets your filters.
- lib/onboarding/StepAi.svelte:306 (text): Mass-rename
- lib/onboarding/StepAi.svelte:307 (text): You use the batch rename UI to manually set the rename pattern, review and
  apply.
- lib/onboarding/StepAi.svelte:313 (text): Select
- lib/onboarding/StepAi.svelte:329 (text): You say "select all image files", agent suggests a selection, you review and
  apply at will.
- lib/onboarding/StepAi.svelte:335 (text): You picked this last time. Confirm or change below.
- lib/onboarding/StepAi.svelte:338 (attr:aria-label): AI choice
- lib/onboarding/StepAi.svelte:339 (text): Based on this, do you want AI or not?
- lib/onboarding/StepAi.svelte:352 (text): Yes, I want AI
- lib/onboarding/StepAi.svelte:363 (text): Select a provider
- lib/onboarding/StepAi.svelte:397 (text): Yes, I want AI, but I want to be super private
- lib/onboarding/StepAi.svelte:423 (text): Thanks but no thanks, no AI for me
- lib/onboarding/StepBeta.svelte:173 (text): Help improve Cmdr!
- lib/onboarding/StepBeta.svelte:179 (text): David
- lib/onboarding/StepBeta.svelte:186 (text): Your feedback helps me spot bugs and prioritize features. Here is how you
  can engage:
- lib/onboarding/StepBeta.svelte:189 (text): In-app:
- lib/onboarding/StepBeta.svelte:189 (text): See
- lib/onboarding/StepBeta.svelte:189 (text): Help &gt; Send feedback…
- lib/onboarding/StepBeta.svelte:198 (text): GitHub
- lib/onboarding/StepBeta.svelte:206 (text): Discord
- lib/onboarding/StepBeta.svelte:214 (text): Schedule a call with me
- lib/onboarding/StepBeta.svelte:225 (text): here on GitHub
- lib/onboarding/StepBeta.svelte:226 (text): brew install cmdr
- lib/onboarding/StepBeta.svelte:237 (text): Send anonymous usage stats
- lib/onboarding/StepBeta.svelte:242 (text): Note that it's ON by default to encourage people to send me data during the
  Beta. You can change this any time in Settings.
- lib/onboarding/StepBeta.svelte:248 (text): Stay in touch (optional)
- lib/onboarding/StepBeta.svelte:252 (attr:placeholder): you@example.com
- lib/onboarding/StepBeta.svelte:258 (attr:aria-label): Stay in touch (optional)
- lib/onboarding/StepBeta.svelte:265 (text): Sorry, we couldn't sign you up right now. Try again?
- lib/onboarding/StepFda.svelte:178 (text): You granted full disk access!
- lib/onboarding/StepFda.svelte:179 (text): Nice, that's all Cmdr needs. Restart it now to start using everything.
- lib/onboarding/StepFda.svelte:185 (text): Cmdr currently has full disk access
- lib/onboarding/StepFda.svelte:188 (text): Welcome to Cmdr!
- lib/onboarding/StepFda.svelte:191 (text): It looks like you accepted full disk access before but then revoked it.
- lib/onboarding/StepFda.svelte:192 (text): The app currently has no full disk access.
- lib/onboarding/StepFda.svelte:194 (text): Deny
- lib/onboarding/StepFda.svelte:196 (text): If it
- lib/onboarding/StepFda.svelte:196 (text): wasn't
- lib/onboarding/StepFda.svelte:196 (text): intentional, consider allowing full disk access again. Here are the pros and
  cons:
- lib/onboarding/StepFda.svelte:198 (text): You probably just want to start using the app.
- lib/onboarding/StepFda.svelte:198 (text): Sorry to bother you with this first, but it's needed.
- lib/onboarding/StepFda.svelte:203 (text): Would you like to give this app full disk access? Here's what that means:
- lib/onboarding/StepFda.svelte:208 (text): Pro:
- lib/onboarding/StepFda.svelte:212 (text): Con:
- lib/onboarding/StepFda.svelte:229 (text): If you decide to allow:
- lib/onboarding/StepFda.svelte:232 (text): Click
- lib/onboarding/StepFda.svelte:235 (text): Cmdr
- lib/onboarding/StepFda.svelte:237 (text): Cmdr
- lib/onboarding/StepFda.svelte:240 (text): Cmdr
- lib/onboarding/StepFda.svelte:241 (text): Applications
- lib/onboarding/StepFda.svelte:244 (text): Confirm and click
- lib/onboarding/StepFda.svelte:244 (text): Quit & Reopen
- lib/onboarding/StepFda.svelte:249 (text): Deny
- lib/onboarding/StepFda.svelte:253 (text): Cmdr needs to restart so the new permission takes effect.
- lib/onboarding/StepFda.svelte:255 (text): Restart Cmdr
- lib/onboarding/StepFda.svelte:256 (text): Deny
- lib/onboarding/StepOptional.svelte:51 (text): You're almost ready
- lib/onboarding/StepOptional.svelte:60 (text): Networking
- lib/onboarding/StepOptional.svelte:70 (text): Recommended: on. You can change this any time in Settings.
- lib/onboarding/StepOptional.svelte:78 (text): Drive indexing
- lib/onboarding/StepOptional.svelte:83 (text): Instant search of your whole drive. Think Spotlight, but even faster.
- lib/onboarding/StepOptional.svelte:97 (text): Recommended: on. You can change this any time in Settings.
- lib/onboarding/StepOptional.svelte:105 (text): Automatic updates
- lib/onboarding/StepOptional.svelte:115 (text): Recommended: on. You can change this any time in Settings.
- lib/onboarding/StepOptional.svelte:123 (text): MTP (Android phones, Kindles, cameras)
- lib/onboarding/StepOptional.svelte:125 (text): connect to Android phones, Kindles, cameras
- lib/onboarding/StepOptional.svelte:134 (text): Recommended: on. You can change this any time in Settings.

### `query-ui` (47)

- lib/query-ui/AiPromptStrip.svelte:44 (attr:aria-label): What the agent did with your last AI search
- lib/query-ui/AiPromptStrip.svelte:48 (text): Here's what the agent did:
- lib/query-ui/AiPromptStrip.svelte:65 (text): Nothing to filter on yet. Try rephrasing?
- lib/query-ui/AiPromptStrip.svelte:76 (attr:aria-label): Refine the AI search (coming soon)
- lib/query-ui/EmptyState.svelte:74 (text): Try…
- lib/query-ui/EmptyState.svelte:93 (text): starts fresh,
- lib/query-ui/QueryBar.svelte:109 (text): Press Enter to search
- lib/query-ui/QueryBar.svelte:123 (text): Search
- lib/query-ui/QueryDialog.svelte:895 (attr:aria-label): Dialog actions
- lib/query-ui/QueryResults.svelte:185 (text): Name
- lib/query-ui/QueryResults.svelte:186 (text): Path
- lib/query-ui/QueryResults.svelte:187 (text): Size
- lib/query-ui/QueryResults.svelte:188 (text): Modified
- lib/query-ui/QueryResults.svelte:189 (text): Actions
- lib/query-ui/QueryResults.svelte:214 (text): Loading drive index...
- lib/query-ui/QueryResults.svelte:223 (text): Searching...
- lib/query-ui/QueryResults.svelte:228 (text): No files match these criteria:
- lib/query-ui/SearchRowMenu.svelte:31 (attr:aria-label): More actions
- lib/query-ui/filter-chips/DateFilterPopover.svelte:136 (attr:label): Modified
- lib/query-ui/filter-chips/DateFilterPopover.svelte:141 (attr:aria-label): Modified filter options
- lib/query-ui/filter-chips/DateFilterPopover.svelte:144 (attr:aria-label): Comparator
- lib/query-ui/filter-chips/DateFilterPopover.svelte:169 (attr:aria-label): Date value
- lib/query-ui/filter-chips/DateFilterPopover.svelte:209 (attr:aria-label): Custom date value
- lib/query-ui/filter-chips/DateFilterPopover.svelte:219 (attr:aria-label): Maximum date value
- lib/query-ui/filter-chips/DateFilterPopover.svelte:259 (attr:aria-label): Custom maximum date value
- lib/query-ui/filter-chips/FilterChips.svelte:311 (attr:aria-label): Search filters
- lib/query-ui/filter-chips/FilterChips.svelte:328 (attr:label): Pattern
- lib/query-ui/filter-chips/FilterChips.svelte:344 (attr:label): Size
- lib/query-ui/filter-chips/FilterChips.svelte:358 (attr:label): Modified
- lib/query-ui/filter-chips/FilterChips.svelte:372 (attr:label): Search in
- lib/query-ui/filter-chips/ScopeFilterPopover.svelte:66 (attr:label): Search in
- lib/query-ui/filter-chips/ScopeFilterPopover.svelte:74 (attr:placeholder): All folders
- lib/query-ui/filter-chips/ScopeFilterPopover.svelte:77 (attr:aria-label): Scope folders
- lib/query-ui/filter-chips/ScopeFilterPopover.svelte:95 (attr:aria-label): Hide boring folders
- lib/query-ui/filter-chips/ScopeFilterPopover.svelte:101 (text): Hide boring folders
- lib/query-ui/filter-chips/ScopeFilterPopover.svelte:110 (attr:aria-label): Case-sensitive matching
- lib/query-ui/filter-chips/ScopeFilterPopover.svelte:112 (text): Case-sensitive
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:135 (attr:label): Size
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:140 (attr:aria-label): Size filter options
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:143 (attr:aria-label): Comparator
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:166 (attr:aria-label): Minimum size value
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:214 (attr:aria-label): Custom minimum size value
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:226 (attr:aria-label): Minimum size unit
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:272 (attr:aria-label): Maximum size value
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:313 (attr:aria-label): Custom maximum size value
- lib/query-ui/filter-chips/SizeFilterPopover.svelte:325 (attr:aria-label): Maximum size unit
- lib/query-ui/recent-items/RecentItemsPopover.svelte:200 (text): to move ·

### `search` (1)

- lib/search/SearchDialog.svelte:501 (text): These folders are hidden:

### `settings` (117)

- lib/settings/components/SectionSummary.svelte:58 (text): This section has no subsections.
- lib/settings/components/SettingColorSwatchPicker.svelte:125 (attr:aria-label): Choose a tint color for {label}
- lib/settings/components/SettingColorSwatchPicker.svelte:130 (attr:aria-label): Tint colors
- lib/settings/components/SettingColorSwatchPicker.svelte:135 (attr:aria-label): No tint
- lib/settings/components/SettingNumberInput.svelte:46 (attr:aria-label): Decrease {label}
- lib/settings/components/SettingNumberInput.svelte:50 (attr:aria-label): Increase {label}
- lib/settings/components/SettingRow.svelte:73 (attr:aria-label): Reset to default
- lib/settings/components/SettingRow.svelte:82 (text): Restart required
- lib/settings/components/SettingSelect.svelte:150 (attr:placeholder): Enter custom value
- lib/settings/components/SettingsSidebar.svelte:185 (attr:placeholder): Search settings...
- lib/settings/components/SettingsSidebar.svelte:194 (attr:aria-label): Clear search
- lib/settings/components/SettingsSidebar.svelte:198 (attr:aria-label): Settings sections
- lib/settings/sections/AdvancedSection.svelte:102 (attr:title): Advanced
- lib/settings/sections/AdvancedSection.svelte:111 (text): Reset all to defaults
- lib/settings/sections/AdvancedSection.svelte:138 (text): Reset to default
- lib/settings/sections/AdvancedSection.svelte:177 (attr:aria-label): Decrease {setting.label}
- lib/settings/sections/AdvancedSection.svelte:181 (attr:aria-label): Increase {setting.label}
- lib/settings/sections/AiCloudSection.svelte:358 (attr:label): Service
- lib/settings/sections/AiCloudSection.svelte:377 (attr:label): Endpoint
- lib/settings/sections/AiCloudSection.svelte:392 (attr:placeholder): Example: https://api.example.com/v1
- lib/settings/sections/AiCloudSection.svelte:393 (attr:aria-label): Endpoint URL
- lib/settings/sections/AiCloudSection.svelte:403 (attr:aria-label): Endpoint URL
- lib/settings/sections/AiCloudSection.svelte:412 (attr:label): API key
- lib/settings/sections/AiCloudSection.svelte:429 (attr:label): Model
- lib/settings/sections/AiCloudSection.svelte:460 (text): Checking...
- lib/settings/sections/AiCloudSection.svelte:465 (text): Connected
- lib/settings/sections/AiCloudSection.svelte:466 (text): Recheck
- lib/settings/sections/AiCloudSection.svelte:471 (text): Connected (model list not available)
- lib/settings/sections/AiCloudSection.svelte:472 (text): Recheck
- lib/settings/sections/AiCloudSection.svelte:480 (text): Recheck
- lib/settings/sections/AiCloudSection.svelte:488 (text): Recheck
- lib/settings/sections/AiCloudSection.svelte:496 (text): Recheck
- lib/settings/sections/AiCloudSection.svelte:500 (text): Test connection
- lib/settings/sections/AiLocalSection.svelte:361 (text): Model
- lib/settings/sections/AiLocalSection.svelte:367 (text): Server
- lib/settings/sections/AiLocalSection.svelte:389 (attr:label): Context window
- lib/settings/sections/AiLocalSection.svelte:402 (attr:aria-label): Memory warning
- lib/settings/sections/AiLocalSection.svelte:410 (attr:aria-label): Memory warning
- lib/settings/sections/AiLocalSection.svelte:430 (attr:aria-label): Memory usage gauge
- lib/settings/sections/AiLocalSection.svelte:473 (text): Projected
- lib/settings/sections/AiLocalSection.svelte:478 (text): Freed
- lib/settings/sections/AiLocalSection.svelte:494 (text): Cancel
- lib/settings/sections/AiLocalSection.svelte:500 (text): Stop server
- lib/settings/sections/AiLocalSection.svelte:504 (text): Start server
- lib/settings/sections/AiLocalSection.svelte:508 (text): Delete model
- lib/settings/sections/AiLocalSection.svelte:511 (text): Download model
- lib/settings/sections/AiLocalSection.svelte:536 (text): Stopping server and removing files...
- lib/settings/sections/AiLocalSection.svelte:546 (text): Cancel
- lib/settings/sections/AiSection.svelte:112 (attr:title): AI
- lib/settings/sections/AiSection.svelte:114 (text): Loading...
- lib/settings/sections/AiSection.svelte:120 (attr:label): Provider
- lib/settings/sections/AiSection.svelte:124 (attr:aria-label): AI provider
- lib/settings/sections/AppearanceSection.svelte:71 (attr:title): Colors and formats
- lib/settings/sections/AppearanceSection.svelte:110 (attr:aria-label): Open System Settings to change the system theme
  color
- lib/settings/sections/AppearanceSection.svelte:118 (text): System theme color
- lib/settings/sections/AppearanceSection.svelte:132 (text): Cmdr gold
- lib/settings/sections/AppearanceSection.svelte:178 (attr:placeholder): YYYY-MM-DD HH:mm
- lib/settings/sections/AppearanceSection.svelte:190 (text): Format placeholders
- lib/settings/sections/AppearanceSection.svelte:192 (text): YYYY
- lib/settings/sections/AppearanceSection.svelte:193 (text): MM
- lib/settings/sections/AppearanceSection.svelte:194 (text): DD
- lib/settings/sections/AppearanceSection.svelte:195 (text): HH
- lib/settings/sections/AppearanceSection.svelte:222 (text): Tint volume types
- lib/settings/sections/AppearanceSizesSection.svelte:49 (attr:title): File and folder sizes
- lib/settings/sections/AppearanceZoomSection.svelte:21 (attr:title): Zoom and density
- lib/settings/sections/FileOperationsSection.svelte:20 (attr:title): File operations
- lib/settings/sections/FileSystemWatchingSection.svelte:222 (attr:title): File system watching
- lib/settings/sections/FileSystemWatchingSection.svelte:223 (attr:label): Drive indexing
- lib/settings/sections/FileSystemWatchingSection.svelte:237 (text): Index size
- lib/settings/sections/FileSystemWatchingSection.svelte:267 (text): Open System Settings
- lib/settings/sections/FileSystemWatchingSection.svelte:272 (attr:label): Downloads notifications
- lib/settings/sections/FileSystemWatchingSection.svelte:287 (attr:label): Go to latest download
- lib/settings/sections/FileSystemWatchingSection.svelte:315 (attr:label): Low disk space
- lib/settings/sections/GitSection.svelte:22 (attr:title): Git
- lib/settings/sections/KeyboardShortcutsSection.svelte:94 (attr:title): Keyboard shortcuts
- lib/settings/sections/KeyboardShortcutsSection.svelte:100 (attr:placeholder): Search by action name...
- lib/settings/sections/KeyboardShortcutsSection.svelte:115 (attr:placeholder): Filter by keys...
- lib/settings/sections/KeyboardShortcutsSection.svelte:124 (text): Press ESC to clear
- lib/settings/sections/KeyboardShortcutsSection.svelte:170 (text): Cancel
- lib/settings/sections/KeyboardShortcutsSection.svelte:179 (text): Use anyway
- lib/settings/sections/KeyboardShortcutsSection.svelte:180 (text): Cancel
- lib/settings/sections/KeyboardShortcutsSection.svelte:189 (text): Cancel
- lib/settings/sections/KeyboardShortcutsSection.svelte:197 (text): Remove from other
- lib/settings/sections/KeyboardShortcutsSection.svelte:199 (text): Keep both
- lib/settings/sections/KeyboardShortcutsSection.svelte:200 (text): Cancel
- lib/settings/sections/KeyboardShortcutsSection.svelte:253 (text): macOS
- lib/settings/sections/KeyboardShortcutsSection.svelte:258 (text): Fixed
- lib/settings/sections/KeyboardShortcutsSection.svelte:321 (attr:aria-label): Add shortcut
- lib/settings/sections/KeyboardShortcutsSection.svelte:332 (attr:aria-label): Reset to default
- lib/settings/sections/KeyboardShortcutsSection.svelte:354 (text): Reset all to defaults
- lib/settings/sections/LicenseSection.svelte:49 (attr:title): License
- lib/settings/sections/LicenseSection.svelte:51 (text): Loading...
- lib/settings/sections/LicenseSection.svelte:55 (text): License type
- lib/settings/sections/LicenseSection.svelte:60 (text): Organization
- lib/settings/sections/LicenseSection.svelte:66 (text): Status
- lib/settings/sections/LicenseSection.svelte:76 (text): License key
- lib/settings/sections/LicenseSection.svelte:84 (text): Manage license key
- lib/settings/sections/LicenseSection.svelte:86 (text): Enter license key
- lib/settings/sections/LicenseSection.svelte:87 (text): Get a license
- lib/settings/sections/ListingSection.svelte:38 (attr:title): Listing
- lib/settings/sections/LoggingSection.svelte:62 (attr:title): Logging
- lib/settings/sections/LoggingSection.svelte:75 (text): Open log file
- lib/settings/sections/McpServerSection.svelte:47 (text): Promise
- lib/settings/sections/McpServerSection.svelte:193 (attr:title): MCP server
- lib/settings/sections/McpServerSection.svelte:215 (text): Check port
- lib/settings/sections/McpServerSection.svelte:240 (text): Checking port availability...
- lib/settings/sections/MtpSection.svelte:22 (attr:title): MTP (Android/Kindle/cameras)
- lib/settings/sections/NetworkSection.svelte:36 (attr:title): SMB/Network shares
- lib/settings/sections/NetworkSection.svelte:51 (text): Connect to server…
- lib/settings/sections/SearchSection.svelte:37 (attr:title): Search
- lib/settings/sections/UpdatesSection.svelte:97 (attr:title): Updates & privacy
- lib/settings/sections/UpdatesSection.svelte:105 (text): Send error report
- lib/settings/sections/UpdatesSection.svelte:146 (attr:placeholder): you@example.com
- lib/settings/sections/ViewerSection.svelte:19 (attr:title): Viewer
- lib/settings/settings-applier.ts:238 (addToast): Restart Cmdr to apply the log storage change.
- routes/settings/+page.svelte:418 (text): Settings
- routes/settings/+page.svelte:449 (text): Loading settings...

### `shortcuts` (6)

- lib/shortcuts/ShortcutsList.svelte:55 (text): No shortcut
- routes/shortcuts/+page.svelte:79 (text): Keyboard shortcuts
- routes/shortcuts/+page.svelte:85 (text): Keyboard shortcuts
- routes/shortcuts/+page.svelte:86 (text): Edit shortcuts
- routes/shortcuts/+page.svelte:90 (text): Hide features with no shortcut
- routes/shortcuts/+page.svelte:98 (text): Edit shortcuts in Settings

### `ui` (12)

- lib/ui/Combobox.svelte:89 (attr:label): Loading suggestions
- lib/ui/Combobox.svelte:92 (attr:aria-label): Show suggestions
- lib/ui/CommandBox.svelte:33 (attr:aria-label): Copy command to clipboard
- lib/ui/LinkButton.svelte:2 (text): for in-app actions (default) or
- lib/ui/LoadingIcon.svelte:28 (text): Opening folder...
- lib/ui/LoadingIcon.svelte:30 (text): Loading...
- lib/ui/LoadingIcon.svelte:33 (text): Press
- lib/ui/LoadingIcon.svelte:33 (text): to cancel and go back
- lib/ui/ModalDialog.svelte:142 (attr:aria-label): Close
- lib/ui/ShortcutChip.svelte:79 (attr:aria-label): Customize the {commandName} shortcut
- lib/ui/ShortcutChip.svelte:115 (text): wrapping the
- lib/ui/toast/ToastItem.svelte:189 (attr:aria-label): Dismiss notification

### `updates` (4)

- lib/updates/UpdateCheckToastContent.svelte:18 (text): Send error report
- lib/updates/UpdateToastContent.svelte:15 (text): New version available. Restart to update.
- lib/updates/UpdateToastContent.svelte:17 (text): Later
- lib/updates/UpdateToastContent.svelte:18 (text): Restart

### `viewer` (35)

- routes/viewer/+page.svelte:853 (text): File viewer
- routes/viewer/+page.svelte:895 (text): Close
- routes/viewer/+page.svelte:897 (text): Never show this warning again
- routes/viewer/+page.svelte:908 (attr:placeholder): Find in file...
- routes/viewer/+page.svelte:909 (attr:aria-label): Search text
- routes/viewer/+page.svelte:920 (attr:aria-label): Case sensitive
- routes/viewer/+page.svelte:931 (attr:aria-label): Regex
- routes/viewer/+page.svelte:966 (attr:aria-label): Stop searching
- routes/viewer/+page.svelte:975 (attr:aria-label): Previous match
- routes/viewer/+page.svelte:983 (attr:aria-label): Next match
- routes/viewer/+page.svelte:990 (attr:aria-label): Close search
- routes/viewer/+page.svelte:1008 (text): Loading...
- routes/viewer/+page.svelte:1013 (text): Retry
- routes/viewer/+page.svelte:1014 (text): Cancel
- routes/viewer/+page.svelte:1029 (attr:aria-label): File content: {fileName}
- routes/viewer/MediaImageView.svelte:181 (text): Loading image
- routes/viewer/MediaImageView.svelte:186 (text): Sorry, we couldn't show this image. The file may be damaged or in a
  format we can't display.
- routes/viewer/MediaPdfView.svelte:41 (text): Loading PDF
- routes/viewer/ViewerContextMenu.svelte:93 (attr:aria-label): Viewer actions
- routes/viewer/ViewerCopyDialogs.svelte:54 (text): Large pastes can slow down other apps. Try search (⌘F) to narrow it
  down.
- routes/viewer/ViewerCopyDialogs.svelte:56 (text): Cancel
- routes/viewer/ViewerCopyDialogs.svelte:57 (text): Save as file…
- routes/viewer/ViewerCopyDialogs.svelte:58 (text): Copy
- routes/viewer/ViewerCopyDialogs.svelte:83 (text): Cancel
- routes/viewer/ViewerCopyDialogs.svelte:84 (text): Save as file…
- routes/viewer/ViewerStatusBar.svelte:53 (attr:aria-label): File information
- routes/viewer/ViewerStatusBar.svelte:70 (text): in memory
- routes/viewer/ViewerStatusBar.svelte:82 (text): streaming, indexing...
- routes/viewer/ViewerStatusBar.svelte:97 (text): Click 100% / fit &middot; Scroll zoom &middot; Drag pan
- routes/viewer/ViewerStatusBar.svelte:99 (text): W wrap &middot; F tail &middot; ⌘F search
- routes/viewer/ViewerToolbar.svelte:93 (attr:aria-label): Tail mode: follow file changes
- routes/viewer/ViewerToolbar.svelte:101 (text): Reindexing…
- routes/viewer/viewer-copy.svelte.ts:243 (addToast): The read took too long. Try a smaller selection?
- routes/viewer/viewer-copy.svelte.ts:258 (addToast): The read took too long. Try a smaller selection?
- routes/viewer/viewer-copy.svelte.ts:327 (addToast): Saving took too long. Try a smaller selection?

### `whats-new` (5)

- lib/whats-new/WhatsNewDialog.svelte:52 (addToast): Got it, no more update notes. Re-enable them anytime in Settings >
  Updates & privacy.
- lib/whats-new/WhatsNewDialog.svelte:71 (text): Nothing to see here yet. New changes will show up here after an update.
- lib/whats-new/WhatsNewDialog.svelte:103 (text): See full changelog
- lib/whats-new/WhatsNewDialog.svelte:109 (text): Not interested in changelogs
- lib/whats-new/WhatsNewDialog.svelte:110 (text): Close
