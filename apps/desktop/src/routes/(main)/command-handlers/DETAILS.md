# Command handlers details

Depth for the family-grouped handler modules. `CLAUDE.md` holds the must-knows; `types.ts` is the canonical home for the
exemption types. This file adds the family breakdown and the single-source rationale.

## The exempt families (`DispatchExemptId`)

20 ids are registered for the rebinding UI with NO handler, in three families (each documented inline in `types.ts`):

- **Native-menu-owned** (`app.quit`, `app.hide`, `app.hideOthers`, `app.showAll`): run by macOS PredefinedMenuItems via
  native selectors. A JS handler would double-fire alongside the native one.
- **Per-keystroke P2** (`nav.up/down/left/right/firstInFull/lastInFull`): ride `handleKeyDown → FilePane`, never the
  bus. Registered only so the rebinding UI can show/edit their shortcuts.
- **Component-scoped** (palette / volume / network / share / context-menu ids): handled inside each component's own
  keydown handler, not the global dispatch spine.

The core silently no-ops these after the preamble.

## Single-source of the exempt ids

`DISPATCH_EXEMPT_IDS` spreads `NATIVE_SHORTCUT_COMMAND_IDS` (family 1) and `FIXED_KEY_COMMAND_IDS` (families 2 + 3) from
`$lib/commands/command-registry`, the same lists the registry's `nativeShortcut` / `fixedKey` flags key off and the
shortcuts editor uses to render those rows read-only, so each "who owns this key" fact lives in exactly one place. The
`DispatchExemptId` union still lists the literals (a type can't spread a runtime tuple); `command-registry.test.ts` pins
the union and the tuple in sync.

## Analytics from the file arms

`file-handlers.ts` emits two events, both because nothing downstream can.

- `quick_look_used` on all four arms of the `file.quickLook` toggle, the refusals included, so the inner-archive gate
  has a number of its own. The double fire of one Shift+Space (AppKit's menu accelerator plus the webview keydown) is
  swallowed by `quickLookDispatchGuardJustFired()` BEFORE the emit — moving the emit above that guard would double every
  number this event produces.
- `editor_opened` on `file.edit`, with no props: F4 hands the file to the OS's text editor (`open -t`), and the file's
  name and extension are exactly what must never cross.

Props and rationale: `apps/desktop/src-tauri/src/analytics/DETAILS.md` § "Starter event set".
