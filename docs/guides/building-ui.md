# Building UI

A router for building app UI: dialogs, settings screens, windows, and form controls. The rule throughout is reach for
the house primitive in `apps/desktop/src/lib/ui` instead of hand-rolling chrome or a raw control. Two things enforce it:
the `cmdr/prefer-ui-primitive` ESLint rule (no raw native controls) and the `ui-primitive-coverage` check (every
primitive appears in the live catalog).

The canonical token and pattern catalog is `../design-system.md`. The LIVE catalog, every primitive rendered flat with
all variants and states, is Debug window (`⌘D`) → "Components" (`apps/desktop/src/routes/dev/components/`). Open it
before building; it's faster than reading source.

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

For a full-screen commit flow (onboarding, consent, multi-step setup the user can't cancel), use a soft sheet:
`apps/desktop/src/lib/onboarding/OnboardingWizard.svelte` and the `--sheet-*` tokens. The sheet-vs-dialog decision table
is in `../design-system.md` § Soft sheets.

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
