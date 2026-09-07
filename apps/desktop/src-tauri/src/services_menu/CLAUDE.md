# Services menu (macOS)

Makes `Cmdr > Services` show the file services Finder shows. macOS only; nothing else has one.

## Module map

- `mod.rs`: `install()`, called from `setup` BEFORE the menu bar is built (`lib.rs`).
- `responder.rs`: the send-type registration and `CmdrServicesResponder`, the `NSResponder` subclass that answers
  `validRequestorForSendType:returnType:` and writes the pasteboard. All AppKit, all main-thread.
- `selection.rs`: what the menu acts on, pushed by the frontend. No AppKit, fully unit-tested.

The third piece lives elsewhere: `menu/macos_appkit.rs`'s `adopt_installed_services_menu`, because it's a menu-bar
fix-up that has to re-run after every `app.set_menu()`.

## Must-knows

- **Three things are needed, and any one missing gives the same symptom** (the submenu lists four Instruments trace
  templates): register the send types, answer from the responder chain, and point the DISPLAYED Services item at the
  `NSMenu` AppKit fills. Each was measured to be load-bearing; ❌ don't drop one as redundant. `DETAILS.md`.
- **Register BOTH send types; the deprecated one is the load-bearing half.** Of the 25 installed services that take
  files, 20 declare ONLY `NSFilenamesPboardType`, 5 declare ONLY a file-url spelling, and NOT ONE declares both
  (verified on macOS 26.6.2 by counting `NSSendTypes` in `/System/Library/CoreServices/pbs -dump_pboard`, 2026-09-07).
  ❌ Don't "modernize" away the deprecated type: 20 of the 25 go with it.
- **❌ The responder goes BELOW the window, never after it.** `NSWindow` doesn't forward
  `validRequestorForSendType:returnType:` to its own `nextResponder`, so a responder spliced there is never asked. It
  sits between the content view and the window, where the walk from the first responder reaches it.
- **The selection arrives as listing INDICES, not paths**, resolved from the listing cache only when asked. AppKit asks
  synchronously on the main thread and can't await IPC. The search-results snapshot has no cached listing and is the
  one shape that travels as paths.
- **`writeSelectionToPasteboard:` runs once per CANDIDATE SERVICE as the submenu opens**, not only when one is picked
  (seven calls in 90 ms, measured). Anything added there is paid per service, per open.
- **The frontend decides whether a pane may offer rows at all** (`paneRowsAreOsVisible` over `capabilitiesForPane`), so
  a phone, an archive's insides, the `.git` portal, and the host list push nothing.
- **Finder's rule, both ends**: with a selection, act on the selection; with none, act on the cursor row.
- **Main window only.** Settings and the viewer have no pane selection, so they keep the plain menu.

Why each of those, the responder-chain measurements, what was rejected, and what is deliberately still out:
`DETAILS.md`.
