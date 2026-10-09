# Command handlers details

Depth for the family-grouped handler modules. `CLAUDE.md` holds the must-knows; `types.ts` is the canonical home for the
exemption types. This file adds the family breakdown and the single-source rationale.

## The exempt families (`DispatchExemptId`)

20 ids are registered for the rebinding UI with NO handler, in four families (each documented inline in `types.ts`):

- **Native-menu-owned** (`app.quit`, `app.hide`, `app.hideOthers`, `app.showAll`): run by macOS PredefinedMenuItems via
  native selectors. A JS handler would double-fire alongside the native one.
- **Per-keystroke P2** (`nav.up/down/left/right/firstInFull/lastInFull`): ride `handleKeyDown → FilePane`, never the
  bus. Registered only so the rebinding UI can show/edit their shortcuts.
- **Component-scoped** (palette / volume / network / share ids): handled inside each component's own keydown handler,
  not the global dispatch spine.
- **Deliberate override** (`errorPane.toggleTechnicalDetails`): ErrorPane claims ⌘D through a CAPTURE-phase document
  listener that runs ahead of the spine, so it beats whatever the user bound ⌘D to.

The core silently no-ops these after the preamble.

## Single-source of the exempt ids

`DISPATCH_EXEMPT_IDS` spreads `NATIVE_SHORTCUT_COMMAND_IDS` (family 1) and `FIXED_KEY_COMMAND_IDS` (families 2 + 3) from
`$lib/commands/command-registry`, the same lists the registry's `nativeShortcut` / `fixedKey` flags key off and the
shortcuts editor uses to render those rows read-only, so each "who owns this key" fact lives in exactly one place. The
`DispatchExemptId` union still lists the literals (a type can't spread a runtime tuple); `command-registry.test.ts` pins
the union and the tuple in sync.

## Shared bodies behind grouped ids

- `applyZoomPreset` (`view-handlers.ts`) backs the four `view.zoom.setNN` presets.
- `withEntryUnderCursor` (`file-handlers.ts`) backs every get-entry-then-act file and cloud arm. `file.copyPath` is the
  documented exception.
- `copySelectionOr` (`file-handlers.ts`) backs `file.copyPath` and `file.copyFilename`: with a selection it copies every
  selected item, else it runs the arm's cursor fallback. A `changed` read copies nothing and shows a warn toast.
- `copyPathAndAnnounce` (`file-handlers.ts`) does the clipboard write plus the copied-path toast for `file.copyPath` and
  `file.copyCurrentDirectoryPath`. One selected item goes through it too, so it shows the path itself;
  `copyPathsAndAnnounce` takes two or more and shows a count ("Copied 5 paths."), in the same one-slot toast group.

## Copying the selection's paths and names

`file.copyPath` (⌘⌥C) and `file.copyFilename` copy EVERY selected item, one per line (`\n`, no trailing newline), in
pane order, and fall back to the cursor row only when nothing is selected. That's what Finder's ⌥⌘C "Copy as Pathname"
and Total Commander's "Copy names with full path" / "Copy selected names" do, and the cursor-only version surprised a
user who'd selected five files and got one path.

- `file.copyFilename` stays toast-free, as it was for one name. Only copy-path confirms, because its single-item toast
  already existed.
- `file.copyFilename` stays unbound: ⇧⌘C (Nimble Commander's names key) is `pane.clone`'s.
- The labels keep their wording ("Copy path", "Copy filename"), like Finder's "Copy as Pathname", which doesn't
  pluralize either. Changing them would invalidate every locale for no gain in meaning.

## Analytics from the file arms

`file-handlers.ts` emits two events, both because nothing downstream can.

- `quick_look_used` on all four arms of the `file.quickLook` toggle, the refusals included, so the inner-archive gate
  has a number of its own. The double fire of one Shift+Space (AppKit's menu accelerator plus the webview keydown) is
  swallowed by `quickLookDispatchGuardJustFired()` BEFORE the emit; moving the emit above that guard would double every
  number this event produces.
- `editor_opened` on `file.edit`, with no props: F4 hands the file to the OS's text editor (`open -t`), and the file's
  name and extension are exactly what must never cross.

Props and rationale: `apps/desktop/src-tauri/src/analytics/DETAILS.md` § "Starter event set".
