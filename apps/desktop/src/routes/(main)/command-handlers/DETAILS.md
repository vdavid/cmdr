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
