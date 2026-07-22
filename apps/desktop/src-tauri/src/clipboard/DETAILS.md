# Clipboard details

Depth and rationale. `CLAUDE.md` holds the must-knows; this is the decision record.

## Copy-at-source, decide-at-paste

Follows Finder's model. Cmd+X sets an internal cut flag; the move-vs-copy decision happens at paste time. The cut state
is Cmdr-internal only, so pasting Cmdr-cut files in Finder does a copy (Finder doesn't know about our flag). This
matches third-party file managers (Path Finder, ForkLift).

## Direct NSPasteboard via `objc2`, not a Tauri plugin

The codebase already uses `objc2` for drag image detection, and the official `tauri-plugin-clipboard-manager` only
supports text/images, not file URLs. Direct access gives full control without adding a dependency.

## Cut state in Rust, not the frontend

The backend is authoritative for file operations, so keeping cut state in Rust avoids frontend/backend sync issues. The
frontend queries via IPC when needed. On paste, the backend validates that the live clipboard paths still match the
stored cut-state paths; a mismatch (another app replaced the clipboard) clears the stale cut state and falls back to a
copy.

## E2E mock as a `#[cfg]` module swap, not a `dyn` trait

Three call sites already hop to the main thread via `app.run_on_main_thread()` and pass `PathBuf` values; a trait object
would add `Send` bounds the `objc2` types resist. A `cfg`-driven module swap keeps every call site byte-identical
between configurations and removes the prod-only `objc2` link cost from E2E builds. Acceptance: a full E2E run leaves
`pbpaste` unchanged.

## Runtime `CMDR_CLIPBOARD_BACKEND=mock` override

Lives inside `pasteboard.rs` (not `mod.rs`) because it's a debugging tool for prod-feature builds. Sampled once via
`LazyLock` at first access, so the hot path is a single atomic load. Both the compile-time mock path and this runtime
override share `store.rs`, so a test that flips the env in one process sees the same data the E2E mock module sees in
another. See the "Mock-backend convention" in `docs/tooling/instance-isolation.md`.

## Paste clipboard content as a file (issue #35)

When Cmd+V lands in a pane with no file URLs on the clipboard but some other pasteable content (text, image, PDF), the
backend writes that content to a new `pasted.<ext>` file. The pure core lives in `payload.rs` so precedence, the
markdown sniff, and the flavor mapping are unit-testable with no Tauri runtime or `MainThreadMarker`:

- **`ClipboardData`** (in `store.rs`) is the read bundle: `Option<Vec<u8>>` per image/pdf flavor plus `Option<String>`
  text. It has its OWN static, separate from the file-URL `ClipboardEntry`, so a content paste never clobbers a pending
  file copy (pinned by `payload_tests::injecting_clipboard_data_does_not_clobber_the_file_url_entry`).
  `read_clipboard_data` is the read side (used by both the `playwright-e2e` mock module and the prod
  `CMDR_CLIPBOARD_BACKEND=mock` env path). `write_clipboard_data` / `clear_clipboard_data` are `#[cfg(test)]`
  unit-test-only injection (no prod / E2E caller exists yet; add an admin surface if E2E ever needs to inject content).
- **`pick_clipboard_payload(ClipboardData) -> ClipboardPayload`** applies flavor precedence: image (`public.png` >
  `public.tiff` > `public.jpeg`) > pdf (`com.adobe.pdf`) > text (`public.utf8-plain-text`). Real clipboards are
  multi-flavor (a Finder image copy carries the URL as text; a browser image copy carries the page URL), so we pick the
  highest-intent one.
- **Why TIFF→PNG**: macOS screenshots and many apps put `public.tiff` on the pasteboard. We convert it to PNG
  (`tiff_to_png`, via `NSBitmapImageRep` — a data class, so no main-thread requirement) and write `.png`, because a
  `.tiff` file is a poor default (large, poorly supported). A failed decode falls through to the next flavor rather than
  writing a broken file. `public.png` is written verbatim (no re-encode); `public.jpeg` is written verbatim as `.jpg`
  (no recompression); `com.adobe.pdf` verbatim as `.pdf`.
- **Markdown sniff** (`looks_like_markdown`, conservative): text becomes `.md` only on a strong signal (fenced code
  block, or an ATX heading at line start) or ≥2 DISTINCT weak signal KINDS (link, emphasis pair, list marker,
  blockquote) — "distinct" is by KIND, so two links are one kind. When in doubt it stays `.txt`; a wrong `.md` guess is
  worse than a plain `.txt`.
- **The read path** (`pasteboard::read_pasteboard_data`) does the minimum on the main thread: NSPasteboard is
  main-thread-only, so it just copies each flavor's raw bytes (`dataForType:` per UTI + `stringForType:` for text) into a
  `ClipboardData`. Flavor precedence and the TIFF→PNG decode (`pick_clipboard_payload`, hundreds of ms on a big image)
  run OFF the main thread, in a `spawn_blocking` in the command, so the UI never janks (principle 3). The mock/env path
  returns the injected `ClipboardData`; both feed the same `pick`, so precedence is identical across configurations.
- **The write** lives in `file_system/write_operations/paste_clipboard.rs` (`write_payload_to_dir`), NOT here — see that
  module's docs. The command (`commands/clipboard.rs::paste_clipboard_as_file`) reads the raw flavors on the main thread,
  picks/converts off-main, then hands the payload to the writer under a 30 s write timeout (longer than the 5 s
  empty-mkfile tier because the payload can be a large image; the partial-file-on-timeout edge is documented in the
  write module's DETAILS).
