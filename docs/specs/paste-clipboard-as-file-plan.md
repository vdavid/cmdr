# Paste clipboard content as a file

Implements [GitHub issue #35](https://github.com/vdavid/cmdr/issues/35): Cmd+V in a pane with non-file clipboard content
(text, image, PDF) creates a file in the focused pane's directory. Today this case shows the warn toast "No files on the
clipboard. Copy files first with ⌘C." and does nothing else.

All design decisions below are settled with David; don't relitigate them.

## Behavior

- `edit.paste` in a pane (no text input focused): the existing file-URL paste path runs first, unchanged. Only when the
  clipboard holds **no file URLs** does the new branch run.
- **Flavor precedence: files > image > PDF > text.** Real clipboards are multi-flavor (Finder file copies carry
  filenames as text; browser image copies carry the URL as text), so pick the highest-intent flavor.
  - Image: prefer `public.png` (write verbatim as `.png`); else `public.tiff` (convert to PNG via `NSBitmapImageRep`,
    write `.png`); else `public.jpeg` (write bytes verbatim as `.jpg`, no recompression).
  - PDF: `com.adobe.pdf`, write bytes verbatim as `.pdf`. No new dependency; it's raw pasteboard data.
  - Text: `public.utf8-plain-text` (the existing read), written UTF-8. Extension `.md` if the markdown sniffer fires,
    else `.txt`.
- **Naming**: base name `pasted` + extension (`pasted.txt`, `pasted.md`, `pasted.png`, `pasted.jpg`, `pasted.pdf`). On
  collision, the ` (N)` scheme — the SAME convention as `find_unique_name`
  (`src-tauri/src/file_system/write_operations/conflict.rs`). Decision: the convention lives in one pure
  `numbered_name`-style helper that BOTH `find_unique_name` and the paste path use (no drift, no second convention); the
  paste path creates atomically via `create_file`'s `O_EXCL` and retries with the next N on the typed already-exists
  error (no TOCTOU window), which also works on writable network volumes.
- **Markdown sniffer**: pure function, conservative. Promote to `.md` only on strong signals: a fenced code block, an
  ATX heading at line start, or ≥2 distinct weaker signals (links `[x](y)`, emphasis, list markers, blockquotes). When
  in doubt, `.txt` — a wrong `.md` guess is worse than a plain `.txt`.
- **After creation**: cursor lands on the new file (reuse the `moveCursorToNewFolder` + `setPendingCursorName` plumbing
  from `src/lib/file-operations/mkdir/new-folder-operations.ts` — that pending-name mechanism exists to survive the
  trailing `directory-diff`, don't bypass it). Then, if the setting says so, `startRename()` begins inline rename:
  - Selection stays **stem-only** (the existing rename editor default). Don't select the extension.
  - The extension-change warning (`fileOperations.allowFileExtensionChanges` = `ask`) is **suppressed for this
    auto-started rename only** — renaming a fresh `pasted.txt` to `notes.md` must not pop a dialog. User-initiated
    renames (F2 etc.) keep the warning.
  - Escape during the auto-started rename **keeps the file** (normal rename-cancel semantics; the paste already
    happened, renaming is a separate op).
- **Toast**: level `info`, transient, **7000 ms** (matches the transfer-complete precedent), shown on every paste. Copy
  includes the generated filename, for example "Pasted clipboard text as pasted.txt" (i18n key, active voice, per style
  guide). One action button, **Settings**, deep-linking to the settings dialog at Behavior > Navigation & file ops. No
  separate Dismiss button — the standard toast X covers it. Model the component on `CrashReportToastContent.svelte`
  (component content + injected `toastId`).
- **Setting**: enum, registry entry in `src/lib/settings/settings-registry.ts`, section
  `['Behavior', 'Navigation & file ops']`, `component: 'radio'`, modeled on `fileOperations.allowFileExtensionChanges`.
  Values `doNothing | createFile | createFileAndRename`, **default `createFileAndRename`**. Labels (sentence case): "Do
  nothing" / "Create file" / "Create and rename". Add the `SettingRow` in `NavigationAndFileOpsSection.svelte` and en
  i18n keys with translator `@` descriptions. `doNothing` restores today's behavior EXACTLY, including the existing "No
  files on the clipboard" warn toast.
- **Read-only targets**: the new branch respects the same `caps.canPasteInto` gate as file paste (MTP, read-only
  archives, read-only SMB refuse identically).
- **`edit.pasteAsMove`**: behaves identically to `edit.paste` for non-file content (move semantics are meaningless for
  clipboard bytes); it must not become a dead key.
- **Text inputs unchanged**: the existing branch that pastes clipboard text into a focused input/textarea/
  contenteditable runs before all of this and stays as is.
- **Non-macOS**: stub `Err`, same as the existing pasteboard functions.
- **No-creation feedback (decision, 2026-07-07)**: when no file gets created, behavior is exactly today's. Setting
  `doNothing` (regardless of clipboard content) and a truly empty/unsupported clipboard (any setting value) both show
  the EXISTING warn toast ("No files on the clipboard. Copy files first with ⌘C."), unchanged copy. Only an actual file
  creation replaces it with the new info toast. Rationale: "Do nothing" must mean literally "what Cmdr did before this
  feature", and a no-op keystroke should keep giving feedback.

## Architecture notes (smart backend, thin frontend)

- New pasteboard reads live in `src-tauri/src/clipboard/pasteboard.rs` beside the existing three functions, using the
  same main-thread-hop (`run_on_main_thread` + mpsc) pattern. Extend `clipboard/mock.rs` so tests can inject
  image/PDF/text clipboard content.
- One new Tauri command (in `src-tauri/src/commands/clipboard.rs`) does the whole job backend-side: read pasteboard →
  pick flavor → sniff/convert → unique name → write file; returns the created file name + a kind discriminator, or a
  **typed** error enum (no string matching — see `.claude/rules/no-string-matching.md`). "Nothing pasteable" is a typed
  variant the frontend treats as a no-op, not an error toast.
- Frontend: `pasteFromClipboard` in the explorer (see `src/routes/(main)/command-handlers/clipboard-handlers.ts` and
  `explorer-api.ts`) grows the fallback branch: no file URLs → check setting → call the new command → cursor land →
  optional `startRename()` → toast.
- Safe write: go through the existing write-operations layer; don't hand-roll `fs::write` without the project's
  data-safety conventions.

## Testing

- TDD, real red first (tester agent owns tests; implementer owns prod code).
- Rust: markdown-sniffer unit tests (table-driven, edge cases: URL-only text, code fence, heading mid-line vs line
  start, plain prose with one asterisk); naming/dedup against a temp dir; command-level tests with the mock pasteboard
  covering flavor precedence, TIFF conversion (can be behind `#[cfg(target_os = "macos")]`), PDF passthrough, empty
  clipboard, read-only dir.
- Frontend (Vitest): setting gating (three values), fallback-branch dispatch (no paths → command called; paths →
  transfer path untouched), toast composition, rename suppression flag plumbing.
- E2E/MCP verification happens after the pair finishes (lead runs it).

## Docs to update (per `.claude/rules/docs.md`)

- `src-tauri/src/clipboard/` colocated docs (new read paths, flavor precedence, why TIFF→PNG).
- `src/routes/(main)/DETAILS.md` paste-flow section (the fallback branch).
- Settings docs only if `adding-a-new-setting.md` steps changed (they shouldn't).
- `CHANGELOG.md` is fed from commit messages at release time; skip it.
