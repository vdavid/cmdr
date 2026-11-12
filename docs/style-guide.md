# Style guide

Writing, code, and design styles.

## Writing

- Voice
  - **The website speaks product-first: no "I", no "we"**: On getcmdr.com, write "Cmdr indexes your drive" and "your
    feedback shapes what's next", never "we built" or "I'm improving". "We" overstates a one-person project; "I"
    overemphasizes that it's one person. Exceptions: FAQ questions and testimonials speak as the user ("Can I use
    it…?"), blog posts are signed personal writing, and legal pages keep the conventional "we".
  - **The app may speak as David where it's deliberately personal**: the onboarding beta step and the About window sign
    their copy ("Hi, I'm David!"). That's intentional warmth, not a violation of the website rule.
- Menu labels
  - **A trailing `…` means the dialog can change what the command acts on**, not that a dialog appears at all. `Copy…`
    takes a destination; `Delete` only asks whether to go ahead. Use the U+2026 character, never three periods. The
    verdict per item, and why this beats Apple's own "requires further input" phrasing, live in
    `apps/desktop/src-tauri/src/menu/DETAILS.md`.
- Ellipsis
  - **Cmdr writes every ellipsis as `…` (U+2026), never three periods**: progress text ("Loading…"), placeholders
    ("Example: sk-ant-abc123…"), and truncation (leading or trailing) all use the same character.
- Quotation marks
  - **UI copy quotes with curly marks, never straight ones**: “double” for a quoted phrase or label, nested ‘single’ for
    a quote inside a quote, never `"` or `'`.
  - **Quote user-owned names and UI labels named in prose**: a server, drive, share, or branch name, and a button, menu
    item, or setting named in a sentence (`…so “Add anyway” saves it as you typed it`), take curly quotes, in every key
    of a family. Locales quote them anyway, so a bare English label leaves them guessing where the label ends.
- Translatable sentences (these keep every language able to follow)
  - **"Item" when a row can be a file or a folder**: when a row, line, or insert can be either, the English and its
    `@key` description say "item", never "file". A file-only word makes translators pick a file-only noun and gender.
  - **No pronoun pointing back at a separate title or an insert**: name the thing ("Close that app", "so the log starts
    fresh"). "It" after `{app}` or a title forces a gender or case the translator can't know.
  - **Command descriptions speak in the third person**, like macOS help tags: "Opens the badge…", never "Open the
    badge…".
- Apostrophes
  - **UI copy writes every apostrophe as `’` (U+2019), like Apple**: “Don’t show again”, “Cmdr’s”, “you’re”. In the
    app's catalogs this also retires ICU's doubled `''`. A straight `'` stays only where it's literal code a user types
    (a shell command in backticks).
- Specific terms
  - **US English spelling, everywhere**: "color", "canceled", "behavior", "gray", "favorites". The app's `en` message
    catalog is the base the British and Australian overlays fork from, so the register is load-bearing there; the
    decision, the Apple precedent behind it, and the "don't “fix” a US spelling back to British" guardrail live in
    `docs/i18n/en/style.md`. The website and the docs follow the same spelling so one voice reads across all of them.
  - **"anymore" is one word when it means "no longer"** ("isn’t connected anymore"); "any more" stays two words only for
    quantity ("any more files"). A new no-longer string also needs `en-GB` and `en-AU` overrides spelled "any more":
    `docs/i18n/en-GB/style.md` § `any more` for "no longer".
  - **The disk picker is the "volume switcher", in every user-facing string.** It once answered to both "volume chooser"
    and "volume switcher", which sent readers and translators looking for two surfaces. The message keys and command ids
    keep `Chooser` (`shortcuts.scope.volumeChooser`, `pane.leftVolumeChooser`), because those are persisted in
    everyone's shortcut settings and can't be renamed; translate the value and ignore the key. Per-locale resolutions:
    each locale's `docs/i18n/<locale>/terms.json`.
  - **The file list's two layouts are "Full view" and "Brief view"**, the View menu's own names, never "view modes".
  - **Folder vs directory**: We know these mean the same. We allow both. Use whichever feels better in each situation.
    Like, on the backend, listing "folders" with `readdir` feels wrong, but also, "folder" comes more natural on the
    front-end and end-user docs.

### Platform vocabulary

The canonical English word per OS for everything Cmdr names that the OS names differently. Each OS reads its own words
(`docs/design-principles.md` § "Platform-native, not generic"); a value that differs is one whole-value
`{platform, select, …}` block (`apps/desktop/src/lib/intl/DETAILS.md` § Platform variants), whose `other` branch is
Linux. Why each choice was made, with the evidence: `docs/notes/windows-port/copy-decisions.md` and
`copy-glossary-evidence.md` beside it. Translations follow each OS's own localized term, branch by branch
(`docs/guides/i18n-translation.md`).

Each line reads macOS / Windows / Linux.

- The machine
  - **This Mac, your Mac** / **this PC, your PC** / **this computer, your computer**. On Windows, never "PC" and
    "computer" in one string; "device" is the general word there ("which devices the export allows").
  - **macOS as the actor** ("macOS blocked…") / **Windows** / no actor on Linux. See the Linux actor rule below.
  - **macOS version** / **Windows version** / **Linux distribution and version**.
  - **startup disk** / **system drive** / **system disk**. On Windows, the PC's own storage is a "drive" ("check that
    the drive is still connected", "your PC doesn’t have room"); "disk" survives only in "disk image" and "disk space".
- Files and folders
  - **Finder** / **File Explorer** / **your file manager**. The reveal action: "Show in Finder" / "Show in File
    Explorer" / "Show in file manager".
  - **Get Info** / **Properties** / **Properties**. The permissions place: "Sharing & Permissions" / the **Security**
    tab / "Permissions".
  - **Locked** (the Finder flag) / the **Read-only** attribute / a protection flag.
  - **home folder** / **user folder** / **home folder**. Windows says "user folder" everywhere, the command included
    ("Go to user folder"); "home" stays a palette search alias, since Windows' own "Home" is File Explorer's landing
    page.
  - **Eject** for a removable drive or a mounted image on every OS. A network connection differs: "Eject" / "Disconnect"
    (a mapped network drive) / "Unmount" (a network mount). Linux never "disconnects" a mount.
  - Paths: `~/Documents`, `/…` / `C:\Users\…`, `\\server\share` / `~/Documents`, `/…`.
- Trash
  - macOS and Linux: **Trash** for the place, "move to trash" for the action. Linux keeps the macOS branch's casing.
  - Windows has two registers.
    - **Label** (the name of the F8 action on a button, menu row, or palette command): "Delete", like "Undo delete" and
      "Overwrite and delete old file".
    - **Sentence** (everything else: titles, confirmations, progress, toasts, errors, settings descriptions, queue rows,
      Ask Cmdr narration): name the place, always "the Recycle Bin", capitalized and with the article, like "Move 3
      files to the Recycle Bin?" and "Moved to the Recycle Bin". The article drops only in a label that names the place
      ("Go to Recycle Bin").
    - Telling them apart: if the string IS the action's name, it's a label; if it describes what happens or happened,
      it's a sentence. An on/off switch for the behavior names the place ("Move to the Recycle Bin"), since "Delete"
      would read as deleting on or off.
    - **The permanent sibling says "permanently"** wherever the trash label reads "Delete" on the same surface: "Delete
      permanently", "Delete the rest permanently". Otherwise the two actions read the same.
  - The British and Australian overlays say "Bin" on macOS and Linux, and "Recycle Bin" on Windows.
- Quit
  - macOS and Linux: **Quit** / "quit", in both registers below.
  - Windows has two registers.
    - **Action** (a command, menu row, button, or dialog title): "Exit", like "Exit Cmdr", "Exit now", "Exit Cmdr?",
      "Delete all and exit".
    - **Narration** (Cmdr is the subject of a sentence): "close", like "then closes", "Cmdr closed unexpectedly last
      time", "Closing Cmdr".
    - Telling them apart: an action is something the user picks; narration reports what Cmdr does or did.
  - Another app follows its own OS too: "Quit and reopen {app}" / "Close and reopen {app}" / "Quit and reopen {app}".
- System apps and places
  - **System Settings** (pane names come localized from the OS) / **Settings**, with the path written out ("Settings >
    Personalization > Colors") / "your system settings" (or the setting in words: "your system’s network settings").
  - Cmdr's own settings window in a menu: "Settings…" / "Settings" / "Settings…".
  - **Keychain**, **Keychain Access** / **Credential Manager**, named only where the user has to go there (a checkbox
    says "Remember password") / **your keyring** ("Remember in keyring").
  - **Terminal** ("In Terminal, run…") / **PowerShell** ("In PowerShell, run…", with a command that works there) / **a
    terminal** ("In a terminal, run…"). Windows never says "Terminal": Windows Terminal is the default only on
    Windows 11.
  - **Activity Monitor** / **Task Manager** (Ctrl+Shift+Esc) / your system monitor app.
  - **Spotlight** / search in Start / no name.
  - **Dock** / **taskbar** / no name.
  - **Disk Utility** > First Aid / the drive's **Properties** > **Tools** > **Check** / no name.
  - Installing and removing: the Applications folder, drag to Trash / **Settings** > **Apps** > **Installed apps** /
    "the way you installed it" (Cmdr ships as deb, rpm, and AppImage).
  - A notification channel's name: "macOS" / "Windows" / "System".
- Keys and instructions
  - Modifier names in prose: ⌘ ⌥ ⌃ ⇧ / Ctrl, Alt, Shift, Win / Ctrl, Alt, Shift, Super. Prefer passing the bound combo
    as a placeholder over writing a key into copy.
  - Windows instructions say "select" for "click", and "turn on" / "turn off" for "enable" / "disable". "Right-click" is
    fine on every OS.
- Words that stay the same on every OS: "disk image", "full screen", "right-click", "Eject" for removable drives.
- Features only macOS has take no Windows or Linux word. Their surfaces are hidden off macOS
  (`apps/desktop/src/lib/platform/DETAILS.md` § Capabilities): Quick Look, Finder tags, Full Disk Access, the Local
  Network permission, the Services menu, iCloud Drive, the app menu ("Cmdr > …").

**The Linux actor rule.** Linux has no name for "the thing that refused": it may be the kernel, the desktop, or a
security module. So where macOS says "macOS wouldn’t…" and Windows says "Windows wouldn’t…", the Linux sentence is
rewritten with Cmdr or the item as its subject ("Cmdr wasn’t allowed to touch {count} of the items you picked.").
Swapping "macOS" for "the system" isn't enough: the sentence gets a new subject. "Your system" stays where it names a
place or a setting ("your system’s network settings"), "this computer" names the machine, and "Linux" appears only for a
platform fact (a crash report sends the distribution name).

**A branch changes only for its OS.** A Windows or Linux branch differs from the macOS text only where that OS needs
different words; anything else stays byte-identical, so its translations carry over.

## Code

Comments: Only add docs that actually add info. No tautologies.

- ✅ Add meaningful comments for public functions, methods, and types to help the next dev.
- ❌ BUT DO NOT use comments for stuff like `Gets the name` for a function called `getName` :D
- ⚠️ Before adding a comment, try using a more descriptive name for the function/param/variable.
- ❌ DO NOT repeat TypeScript/Rust/Go types in the docs (like `@param`/`@returns`).
- ✅ USE comments to mark caveats, tricky/unusual solutions, formats (`YYYY-MM-DD`), and constraints (`must end with /`)

### TypeScript

- No enums, value namespaces, or constructor parameter properties: the desktop `tsconfig.json` sets
  `erasableSyntaxOnly`, so Node can run `scripts/*.ts` by stripping types, and ESLint skips `no-unsafe-enum-assignment`
  (~17 s per Svelte lint pass). For a closed set of values, use an `as const` object and derive the type:
  `type MatchQuality = (typeof MatchQuality)[keyof typeof MatchQuality]`.

### Rust

- Max 120 char lines, 4-space indent, cognitive complexity threshold: 15, enforced by clippy.
- Wrap comments (`//`, `///`, `//!`) at 100 chars. Stable `rustfmt` doesn't enforce this (it's a manual convention). To
  bulk-reflow, run nightly `rustfmt` once with `unstable_features = true`, `wrap_comments = true`, `comment_width = 100`
  in `rustfmt.toml`, then revert the config.

### Agent-facing docs

Agent-facing docs (`CLAUDE.md`, `DETAILS.md`, `AGENTS.md`, and everything under `docs/`) are read by AI agents as a
linear token stream, not a 2D layout. So:

- **No two-column tables.** Use a bullet list instead, one item per row, formatted `- **Title**: details`: a bold title,
  then a literal ": " (colon + space) separator, then the rest of the row. Never a dash or em-dash as the separator. The
  padding in an aligned table wastes tokens and, worse, any edit reflows every row, which causes spurious git merge
  conflicts. A three-column table whose first column is a pure sequential index counts as two columns: drop the index (a
  numbered list works if the index is meaningful) and bullet the two real columns. The `docs-table-hygiene` check
  enforces this (error-level, no allowlist: a two-column table is always convertible).
- **No column wider than 100 characters.** oxfmt pads every cell in a column out to its widest cell, so a single
  prose-or-list cell taxes the whole column with padding tokens. A column that wide isn't tabular data: trim the cell,
  or destructure the table into sections (a `###` heading per row) or bullets. `docs-table-hygiene` enforces this too,
  error-level and no allowlist (every wide column is fixable by trimming or destructuring).
- **Genuine matrices stay tables.** A table with three or more meaningful columns (a capability grid with ✓/✗ across
  several columns, a comparison table) is legitimately 2D, so keep it. The check flags only the two-column case and
  oversized columns, never a real matrix whose cells stay short.

Human-facing markdown is exempt: `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, and everything under `apps/website/`
and `brand/` stay human-optimized and may use tables freely.

## Design

See `design-principles.md` for product design values (UX, accessibility, cancellation, platform behavior). Read it when
designing features or making UX decisions.

Always read the [frontend guide](frontend-guide.md) too if you're working on front end stuff like TS/Svelte or Astro!
