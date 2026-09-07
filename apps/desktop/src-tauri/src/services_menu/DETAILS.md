# Services menu details

Everything below was measured on macOS 26.6.2 on 2026-09-07, on the dev build, by reading the live menu through the
accessibility API and logging inside the responder.

## What was wrong

`Cmdr > Services` listed four Instruments trace templates ("Activity Monitor", "Allocations & Leaks", "File Activity",
"System Trace") and nothing else. Dumping the live services database
(`/System/Library/CoreServices/pbs -dump_pboard`) explains both halves:

- Those four declare **no `NSSendTypes` at all**, only `NSRequiredContext = { NSServiceCategory = "public.source-code" }`.
  A service that needs nothing from the app is shown everywhere, which is why they were the whole list.
- Every file-taking service declares send types. Ghostty, for example:

  ```
  NSMenuItem = { default = "New Ghostty Tab Here"; };
  NSRequiredContext = { NSTextContent = FilePath; };
  NSSendTypes = ( NSFilenamesPboardType, "public.plain-text" );
  ```

  AppKit hides all of them from an app that never says it can send those types.

## Three things had to be true

Registering the send types is the part every write-up mentions, and on its own it changes nothing here. All three of
these were measured to be necessary; with any one missing the menu looks exactly as it did before.

### 1. Register both send types

Counting `NSSendTypes` across the 54 services in that dump, 25 of which take files: **20 declare ONLY
`NSFilenamesPboardType`, 5 declare ONLY `public.file-url` / `NSPasteboardTypeFileURL`, and none declares both.** Apple's
headers deprecate the first in favor of the second and the shipping services have not followed. The two sets don't
overlap, so registering only the modern type leaves 20 of the 25 hidden; `responder.rs` carries an
`#[expect(deprecated)]` naming that count.

`install` runs from `setup` (which is inside tao's `did_finish_launching`, so it is Apple's recommended
`applicationDidFinishLaunching:` moment) and before the menu bar is built, followed by `NSUpdateDynamicServices()`.

### 2. Put the responder where AppKit actually walks

`validRequestorForSendType:returnType:` is documented as travelling the responder chain, and the chain in a wry app is:

```
WryWebView (first responder) → WryWebViewParent (content view) → TaoWindow
```

Measured behavior of that chain, by calling the selector by hand and logging every hit:

- Calling it on the **first responder** walks the whole VIEW chain. `WKWebView` forwards to `super`, so a responder
  above it is reached.
- Calling it on the **window** reaches nothing. `NSWindow` does not forward to its own `nextResponder`.
- Calling it on **`NSApplication`** reaches nothing either.

So a responder spliced in AFTER the window — the obvious reading of "add yourself to the responder chain", and the
first thing tried here — is never asked. `attach_to_window` splices between the **content view and the window**
instead: the content view's `nextResponder` becomes our object, and ours becomes the window. The web view keeps first
refusal, so text services in a text field still work, and nothing leaves the chain.

`nextResponder` is an **unretained** reference. The responder is held in a main-thread `thread_local!` for the process
lifetime; with only a local holding it, the content view would point at freed memory as soon as the call returned.

### 3. Hand the displayed item AppKit's own menu

The last and least obvious one. muda's `PredefinedMenuItem::services` registers the `NSMenu` it creates with
`NSApplication.setServicesMenu:` at BUILD time, but installing the menu bar materializes a second `NSMenuItem` carrying
a different, empty `NSMenu`. Measured: the two `NSMenu` pointers differed, AppKit had filled the registered one with 26
items, and the one on screen held four.

Re-pointing `NSApplication.servicesMenu` at the displayed menu does not work — the setter is ignored once AppKit owns
one (before and after values were identical in the same statement pair). `adopt_installed_services_menu` in
`menu/macos_appkit.rs` goes the other way and hangs AppKit's managed menu off the displayed item. The managed menu is
still a submenu of muda's build-time menu, and AppKit raises `NSInternalInconsistencyException` ("Menu to be set as
submenu is already a submenu of some menu") on a second parent, so it is detached first.

It lives in that file, with the Edit-menu strip and the Help-menu registration, because it is the same kind of fix-up
and has to re-run after every `app.set_menu()`. It is wrapped in its own `objc2::exception::catch`: re-parenting an
`NSMenu` is the one step there that can raise, and an escaping exception took the Edit-menu strip down with it while
this was being built.

## The pasteboard we write

`writeSelectionToPasteboard:types:` writes Finder's layout, which `native_drag/type_plan.rs` already computes and this
module reuses verbatim (`plan_pasteboard_items(paths, DragSessionLocality::Local)`):

- one `NSPasteboardItem` per path, each carrying `public.file-url` as the URL's `absoluteString`,
- `NSFilenamesPboardType` (the whole `NSArray<NSString>` of POSIX paths) on the **first item only**, which is how a
  legacy multi-file pasteboard has always been shaped.

Both shapes go on regardless of which one the incoming `types` asks for. A pasteboard carrying more than a service
declared costs nothing, and a service reading a flavor it didn't declare first is a real thing; branching on `types`
(the way Nimble Commander does) can only lose.

⚠️ **It runs once per candidate service while the menu OPENS**, not only when a service is picked: AppKit writes the
selection to a scratch pasteboard to evaluate each service's `NSRequiredContext`. Measured: seven calls within 90 ms
for one menu open, each carrying the whole 66-row selection.

**Open risk: a very large selection.** The cost is linear in the selected row count and paid ~7× per menu open, on the
main thread. 66 rows is imperceptible; nobody has measured ⌘A over a 100k-row folder, and a hang there would break
"never block the main thread". The honest fix, if it turns out to matter, is for `valid_requestor` to answer `nil`
above a threshold, so the file services simply don't appear for an enormous selection — degrading to the old short
menu rather than to wrong files or a frozen app. Not implemented, deliberately: a threshold nobody has measured would
hide services from selections that are perfectly fine today.

Return types are empty: Cmdr takes nothing back from a service, so `validRequestorForSendType:returnType:` answers
`nil` for any non-empty `return_type`, and services that want to replace the selection don't appear.

## The selection, and what was rejected

`selection.rs` holds one `RwLock<Option<ServicesSelection>>`, replaced wholesale by the frontend push
(`update_services_selection`, `FilePane.svelte`'s `debouncedServicesSelection`). The constraint that shapes it: **both
AppKit methods run synchronously on the main thread and cannot await IPC**, so whatever they need must already be in
memory and readable under a short lock with no I/O. `get_paths_at_indices` satisfies that (it reads a cached listing's
rows, themselves cached per `include_hidden`), so the resolution can happen when a service asks.

Carrying INDICES rather than paths is what makes the push affordable. Selection changes are continuous (a held ⇧↓),
so the expensive half belongs at the ask. A select-all over a 500k-row folder is a listing id and an index array here;
as resolved paths it would be several MB of JSON per debounce window. The frontend builds the payload **inside** its
debounce for the same reason, and triggers off `createSelectionState`'s exact `onChanged` (which fires once per real
mutation) rather than an effect on `selectedIndices.size`, which would miss a gesture that swaps which rows are
selected without changing how many.

**Rejected: reading the MCP pane mirror** (`mcp/pane_state.rs`). It looks like it already has the answer, and doesn't:
its `files` are capped at 100 mirrored rows, it carries no `listing_id` to resolve indices against, its push is gated
on the `syncsToMcp` capability (so some panes never appear), and it exists to describe the app to agents. Writing the
wrong files into a service is a correctness bug a user feels immediately, so this path owns its own state.

**Rejected: reusing `MenuContext.path`** for the cursor fallback. That field is the RIGHT-CLICKED row and every context
menu overwrites it, so after a right-click it disagrees with the cursor. The Services payload carries its own cursor
path, one string, and stays self-contained.

### Fallbacks

A selection that resolves to nothing (its listing was evicted, or a navigation landed between the push and the ask)
falls back to the cursor row rather than handing over an empty pasteboard: the user sees the same result they'd get
with nothing selected, instead of a service that appears to ignore the click. `has_anything` answers from the SHAPE of
the selection without touching the listing cache, because it runs inside AppKit's menu validation, once per registered
send type.

## What it looks like when it works

With a folder under the cursor and nothing selected, on the machine this was built on:

```
Text:              Reveal in ForkLift, Reveal in Nimble Commander
Files and Folders: Add to Stash Shelf of QSpace Pro, Rename in QSpace Pro, Reveal in QSpace Pro,
                   Show Info in QSpace Pro, Double Commander, Scan Folder, New Ghostty Tab Here,
                   New Ghostty Window Here, Open Folder in Nimble Commander, Upload file in Transmit
Internet:          Reveal in Bloom
Development:       Activity Monitor, Allocations & Leaks, File Activity, System Trace
```

On the `..` row with nothing selected, the pane offers nothing and the menu correctly falls back to the four
Development items — the same list it showed before this feature, which is what "no files to give" should look like.

## Deliberately out of scope

- **The Services submenu inside Cmdr's own right-click menu.** It would need the `NSApp.servicesMenu` swap trick and is
  a separate step.
- **A "Quick Actions" submenu.**
- **`NSApp.servicesProvider`**, the other direction: publishing Cmdr's own services so it appears in other apps' menus.
