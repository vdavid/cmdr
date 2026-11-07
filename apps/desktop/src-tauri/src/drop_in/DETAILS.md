# Drop in details

Pull-tier docs for `drop_in/`. Must-knows live in `CLAUDE.md`. The pasteboard facts this design rests on, and the manual
QA plan, live in `apps/desktop/src/lib/file-explorer/drag/DETAILS.md` § "Drops from other apps".

## Flow

1. **Enter** (`drag_image_detection.rs` → `macos::on_dragging_entered`): reads the dragging pasteboard's type list and
   classifies it (`payload::classify`): `FileUrls`, `Promises`, `ImageData`, `InApp`, or `Nothing`. Emits
   `drag-payload-kind` before forwarding to wry, so the frontend has it when wry's enter event arrives. For a promise or
   image drag it also remembers the frontmost app's name (the drag source, unless Cmdr itself is frontmost).
2. **Perform** (`macos::on_perform_drag_operation`, then wry's original): only for `Promises` / `ImageData`. Makes a
   fresh drop folder, reads `public.url`, and:
   - **Promises**: `readObjectsForClasses:@[NSFilePromiseReceiver]`, then `receivePromisedFilesAtDestination:…` per
     receiver on a fresh `NSOperationQueue`. One reader callback per promised file (`fileTypes.count`, at least one per
     receiver) feeds a `ReceiveTally`; the last one emits `promise-drop-ready` (paths that exist) or
     `promise-drop-failed { nothingReceived }`. A watchdog thread gives up after 10 minutes (`timedOut`, or ready with
     what landed). Reading or receiving raised, or no receiver came back: falls to the image bytes.
   - **Image bytes**: `public.png` / `public.tiff` / `public.jpeg`, copied off the pasteboard on the main thread, then
     picked and decoded off it by `clipboard::pick_clipboard_payload` (TIFF→PNG), named by `payload::image_file_name`
     (the URL's leaf with the real extension, else `image.<ext>`).
   - `promise-drop-started { dropId, itemCount, source }` goes out synchronously, before wry's drop event.
3. **Frontend** pairs started/ready with wry's empty-paths drop and opens the Copy dialog (`lib/file-explorer/pane/`
   `promise-drop.ts`).
4. **Cleanup** (`folders.rs`): a copy started by `copy_between_volumes` whose sources lie in a drop folder claims it;
   `terminal_effects::complete` / `cancelled` release the operation's claim and remove the folder once no other
   operation claims it. `release_promise_drop` (frontend: dialog cancelled, no target, refused) removes an unclaimed one.
   A failed drop removes its own (empty) folder. Startup sweeps the root (`<data dir>/drops`, per instance).

## Decisions

- **Decision**: receive through `NSFilePromiseReceiver` for every promise style
  - **Why**: one API reads all three promise styles seen in the wild: `NSFilePromiseProvider` (WebKit since 2026-03,
    Photos, our own drag-out), the Carbon-style `com.apple.pasteboard.promised-file-url` item Chrome writes, and the
    legacy `NSFilesPromisePboardType` (older Safari, Mail). Verified that it yields a receiver for the latter two with a
    standalone probe (below); the drop-time fulfillment itself needs a real drag.
- **Decision**: the drop folder lives under the app data dir, per instance, and the backend owns its lifetime
  - **Why**: the transfer has many exits (done, cancel, error + Retry, background, queue window, adopted views). Tying
    removal to the operation's terminal event covers every one without the frontend tracking routes. Per instance, like
    the viewer and "Open with" temps, so a dev and a prod Cmdr never sweep each other.
- **Decision**: a failed copy keeps its folder
  - **Why**: the error dialog's Retry starts a new copy from the same sources. The startup sweep takes what's left.
- **Decision**: a late claim releases at once
  - **Why**: `claim_for_operation` runs after `start_volume_copy` returns, and a one-image copy can complete first.
    `DropFolders` remembers the last 64 unclaimed endings, so the claim sees the copy already ended.
- **Decision**: the source label is chosen on the frontend
  - **Why**: its words are i18n catalog strings. Rust sends facts (`appName`, `pageUrl`), never a sentence.
- **Decision**: register `NSFilePromiseReceiver.readableDraggedTypes` on the webview (union with what it has)
  - **Why**: WKWebView's own registered types cover a browser image (URL, image types), but a promise-only drag (a Mail
    attachment) might not reach `draggingEntered:` at all without it. Assumed, not verified (no real Mail drag).

## Gotchas

- **`readObjectsForClasses:` asks a Chrome-style provider for its content type** (verified in the probe: the item's
  data provider is called for `promised-file-content-type`). Cheap, but it's why enter classifies from the TYPE LIST
  only and the receivers are read on perform.
- **`fileNames` is empty before the files are received** (verified in the probe), so the promised file name can't feed
  the started event; the frontend names the files from the ready paths.
