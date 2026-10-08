# Building UI

A router for building app UI: dialogs, settings screens, windows, and form controls. The rule throughout is reach for
the house primitive in `apps/desktop/src/lib/ui` instead of hand-rolling chrome or a raw control. Two things enforce it:
the `cmdr/prefer-ui-primitive` ESLint rule (no raw native controls) and the `ui-primitive-coverage` check (every
primitive appears in the live catalog).

The canonical token and pattern catalog is `../design-system.md`. The LIVE catalog, every primitive rendered flat with
all variants and states, is Debug window (`⌘⇧D`, dev builds) → "Components" (`apps/desktop/src/routes/dev/components/`).
Open it before building; it's faster than reading source.

## Building a dialog

Use `apps/desktop/src/lib/ui/ModalDialog.svelte`. It owns the overlay scrim, focus trap, Escape-to-close, the MCP dialog
registry wiring, and the standard body padding. Don't hand-roll dialog chrome, don't set your own body padding, and
don't add a focus trap yourself: the primitive already does all of it.

- Register the dialog: add its id to `SOFT_DIALOG_REGISTRY` and pass it as `ModalDialog`'s `dialogId` (an unregistered
  id is a TypeScript error). This feeds the Rust MCP backend's "available dialogs".
- Optional props: `resizable` (drag any edge or corner; `"horizontal"` when nothing inside uses extra height),
  `fillBody` (fixed-height frame), and the rest, documented in `../design-system.md` § Dialogs. A dialog that renders a
  path or a filename takes `resizable`, and gives every shortened string a `use:tooltip` carrying the full text. The
  body's side inset is NOT one of them: `ModalDialog` always owns it, in any token (`dialog-inset.spec.ts` measures
  every gallery dialog and fails on a section that re-pays it).
- Add it to the dialog gallery: Debug > Soft dialogs lists every registered soft dialog and opens it with fixture data,
  which is how the dialogs get design-reviewed. A new dialog needs a row (enforced by the `dialog-gallery-coverage`
  check); see `apps/desktop/src/lib/dialog-gallery/CLAUDE.md`.
- Details and gotchas: `apps/desktop/src/lib/ui/CLAUDE.md` and its `DETAILS.md`.

**Asking for a credential? Use the one sign-in sheet**, `apps/desktop/src/lib/servers/SignInSheet.svelte`, opened
through `servers/open-sign-in.ts`. It renders what the backend says to ask, owns the retry and the inline refusal, and
never dials; ❌ don't build a second password dialog for a new protocol. `apps/desktop/src/lib/servers/CLAUDE.md`.

For a full-screen commit flow (onboarding, consent, multi-step setup the user can't cancel), use a soft sheet:
`apps/desktop/src/lib/onboarding/OnboardingWizard.svelte` and the `--sheet-*` tokens. The sheet-vs-dialog decision table
is in `../design-system.md` § Soft sheets.

## Building a menu

Two menu systems: the OS's, through muda (`apps/desktop/src-tauri/src/menu/`), and the house `Menu`
(`apps/desktop/src/lib/ui/Menu.svelte` plus `createMenu`). **Native for a right-click on a backend object; the house
`Menu` when the content is more than plain rows, or when the menu hangs off our own chrome.** Apply in order:

1. **Is any of the CONTENT more than `[icon] label [✓]` rows?** Look in two places, and either one settles it. Inside a
   row: a disk-space bar, a connection dot, a live badge, an inline rename field, a drag handle, a sub-line (the
   primitive's `below` snippet). Around the rows: a heading note, a wrapping explanation, a footer, a quiet caption (its
   `footer` snippet). If yes → house `Menu`, stop, because muda renders none of it.

   ❗ Ask it about the content, never about the rows alone: that's the way to get this one wrong. The drive index
   badge's rows are all plain labels and it's still ours, because a master-switch note wrapping over several lines and a
   "last indexed" caption ride the `footer` snippet
   (`apps/desktop/src/lib/file-explorer/navigation/DriveIndexBadge.svelte`). The volume switcher and the favorites menu
   are ours on the in-row half of the test.

2. **If every row is a plain action, what does it act on?** Backend state (a path, a volume, a server, a file) → native:
   the pick becomes an IPC command anyway, so the round-trip is free and the OS behavior comes with it. Frontend state →
   a native SELECTOR only if BOTH are true: AppKit has a responder action for it (`copy:` / `selectAll:` / `cut:` /
   `paste:`), AND the state that action reads lives in the DOM. A selector lands on a DOM selection or a focused text
   field and nowhere else; pointed at a model the app maintains itself it routes into WebKit and quietly no-ops, which
   is the worst failure shape there is, because the item looks right and does nothing. Otherwise it's Custom native
   items that hand the pick back to the frontend, or the house `Menu`; question 3 picks between them.

   The viewer is the case to remember, because its Edit menu splits down this exact line (`build_viewer_menu` in
   `apps/desktop/src-tauri/src/menu/menu_structure.rs`). **Cut and Paste forward the native selectors and work**: they
   land on the search box, a real text field, which is the only thing in that window they could act on. (They're Custom
   items that re-send the selector rather than Predefined ones, so their enabled state can follow that box's focus; the
   selector is what matters here, not who sends it.) **Copy and Select all are Custom items routed to the frontend**,
   because what they act on is the viewer's own offset-based selection: `.file-content` is `user-select: none`, so the
   only DOM selection a selector can find is the `.status-bar` footer, and native `selectAll:` highlights the footer
   while native `copy:` copies it. That's the failure shape this question exists to catch, and it's worse than a no-op,
   since the item looks right and does the wrong thing. Its CONTENT (right-click) menu is native the same way: Custom
   Copy and Select all whose enabled state the frontend computes at open time (`build_viewer_context_menu`) and whose
   pick Rust emits back to the viewer (`ViewerContextMenuAction`). So frontend state can sit behind a native menu, as
   long as it's Custom items that hand the pick back; ❌ what never works is a native selector pointed at it.

3. **Tiebreaker: what opened it?** A right-click is an OS convention and people expect the OS's menu. A menu hanging off
   a chip or a button in our own chrome should look like ours.

Two of the three pointing the same way is the comfortable case, and worth noticing when it happens: the drive index
badge is settled at question 1 by its footer content, and question 3 agrees on its own, since the menu hangs off a dot
in our chrome. One question carrying the answer alone is the one to think twice about.

**A row's actions inside a house `Menu` go in its → submenu, never a native popup over it.** Right-click opens that same
submenu (the primitive does it), so the two doors can't show two lists. The volume switcher's and the favorites menu's
rows work this way, and the servers hub reuses the same list at the pointer for a one-place server, because it's one
list (`apps/desktop/src/lib/file-explorer/navigation/row-menu.ts`) and a second, native copy would drift from it.

The trade, honestly: native gives OS look and feel, VoiceOver, responder-chain actions, escape from the window bounds,
and system appearance, but no rich rows, a round-trip for frontend state, and nothing Playwright can reach. The house
`Menu` gives arbitrary rows, direct frontend state, our design language, and testability, and costs clipping at the
window edge plus owning keyboard, focus, placement, and a11y. That last cost is now paid once in the primitive rather
than per menu, which is what makes it a fair default for its half of the split.

❗ **Native does NOT get you auto-enabling.** muda calls `setAutoenablesItems(false)` on every `NSMenu` it builds, so
`validateMenuItem:` / `validateUserInterfaceItem:` never runs for a muda item, in the menu bar or in a popup, and an
item's enabled state comes from muda's own flag alone. We compute it per item
(`apps/desktop/src-tauri/src/menu/item_states.rs`); don't ship a native item assuming the OS will grey it out. (Verified
on muda 0.19.3, at lines 134, 338, and 791 of its macOS "platform_impl/macos/mod.rs", 2026-09-17. That one stays quoted
rather than backticked: it's a crate-internal path this repo holds no file for, and `dead-links` reads a backticked path
as a reference. The same finding is recorded at `apps/desktop/src-tauri/src/dock/menu/native.rs`.)

The consumer contract, the row snippets, and what the primitive owns: `apps/desktop/src/lib/ui/DETAILS.md` § Menu.

## Form controls

Never write a raw `<input>`, `<textarea>`, or `<select>`; the `cmdr/prefer-ui-primitive` ESLint rule rejects them.
Native controls render with the OS accent and gray out when the window loses focus, which looks broken next to the app's
accent-token chrome, and a hand-rolled text field re-invents border, radius, padding, and focus-ring CSS that then
drifts. The same rule also rejects hand-rolling the control instead: a `<button>` or `<div>` carrying `role="switch"` /
`role="checkbox"` / `role="radio"` re-implements state, keyboard, and focus wiring the primitive already owns. Use these
instead:

- `TextInput`: any single-line text field (text, password, email, search, url, number). Takes a `radius`, an optional
  leading icon, and a trailing snippet for clear buttons and reveal toggles.
- `TextArea`: the multi-line sibling, same chrome.
- `Checkbox`: a single on/off box.
- `RadioGroup`: one choice from a small set of mutually exclusive options.
- `ToggleGroup`: a segmented control (tabs, or a single-select value picker). Prefer it over `RadioGroup` when the
  options are short and benefit from sitting side by side.
- `Select`: a dropdown for a longer list of values.
- `Combobox`: a text field with suggestions (free text plus a filtered list), not a value-bound select.
- `Chip`: a small pill button (filter trigger or recent-query pill).

## Info glyphs

Two primitives, and the same rule rejects hand-rolling either: a `<button>` whose only meaningful child is
`<Icon name="info">` is flagged.

- `InfoTip`: a `<button>` parking a long explanation behind a ⓘ, reachable on Tab as well as hover. Its body is a plain
  `text` string or a `children` snippet, and `align="radio-row"` fits it into a `RadioGroup`'s trailing slot.
- `StatusGlyph`: a `<span>` marking a condition on the thing beside it (a restricted folder, a symlinked subtree). Never
  focusable, because these live in virtual-scroll rows where a tab stop per row wrecks the keyboard model.

A glyph that means nothing on its own (a banner's or a dialog header's leading mark) is neither: it's a bare `<Icon>` in
a `<span>`. Picking between them and the geometry each assumes: `apps/desktop/src/lib/ui/DETAILS.md`.

## Building a settings screen

Compose from the settings components in `apps/desktop/src/lib/settings/components`:

- `SettingsSection` groups rows under a titled heading.
- `SettingRow` lays out one setting (label, description, control, reset affordance, search highlighting).
- The `Setting*` wrappers bind a control to a registry setting id and delegate to a `lib/ui` primitive:
  `SettingCheckbox`, `SettingRadioGroup`, `SettingToggleGroup`, `SettingSelect`, `SettingSwitch`, `SettingSlider`,
  `SettingNumberInput`, and the rest in that directory.

Don't reach for a `lib/ui` primitive or a raw control directly in a settings section; use the matching `Setting*`
wrapper so the value binds, persists, resets, and appears in settings search. Adding a setting end to end (registry
entry plus row) is its own procedure: `adding-a-new-setting.md`.

## Building a window

A new top-level window (like Settings or the File viewer) is a route, an opener, a capability file, and shell wiring.
Follow `adding-a-window.md`; missing capabilities fail silently, so read the capabilities section.

Settings reach a new window automatically: the ROOT `routes/+layout.svelte` calls `initWindowSettings()`, which seeds
the store and the reactive layer for every route. Classify the new route in `WINDOW_SETTINGS_ACCESS`
(`apps/desktop/src/lib/settings/window-settings.ts`) to match whether its capability file grants `store:default`; a test
fails if you don't.

## Showing a size, a speed, or a duration

`apps/desktop/src/lib/units/CLAUDE.md` is the one place a byte count, a transfer rate, or a duration becomes text.
`formatByteSize` / `formatByteRate` follow the user's binary-vs-SI setting; `<Size bytes>` (`lib/ui/Size.svelte`) is the
component form with size-tier colors, and `<DateLabel>` is the date equivalent. ❌ Don't hand-roll a `formatBytes` or a
`formatEta`: `cmdr/no-private-unit-format` rejects them, because four private copies once drifted apart and two windows
showed different numbers for the same transfer.

## Adding a new primitive

A new `lib/ui/` primitive is a four-part contract, enforced so it can't half-land:

1. The component itself in `lib/ui/`.
2. A tier-3 a11y test, enforced by the `a11y-coverage` check: either a colocated `<Name>.a11y.test.ts` or a
   `*.a11y.test.ts` in the same directory that imports the component.
3. A section in the Debug > Components catalog (`routes/dev/components/`), enforced by `ui-primitive-coverage`.
4. An entry in `../design-system.md` § Component patterns.

The canonical statement of this contract lives in `apps/desktop/src/lib/ui/CLAUDE.md`; read it before adding or changing
a primitive. Prefer a primitive over a raw native control everywhere (`cmdr/prefer-ui-primitive`).
