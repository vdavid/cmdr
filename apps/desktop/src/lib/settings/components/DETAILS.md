# Settings UI primitives details

Depth and rationale for the setting-row primitives. `CLAUDE.md` holds the must-knows that prevent silent breakage.

## Picking a control

Every row is a `SettingRow` (label + description + control + reset pip + restart-required badge; it also carries `split`
and `searchQuery`, and its description text spans the full width regardless of `split`). Pick the inner control by
shape:

- `SettingSwitch`: the primary boolean, wrapping `lib/ui/Switch`.
- `SettingCheckbox`: a secondary boolean, for one hanging off a switch or in a denser layout.
- `SettingSelect`: enum dropdown, wrapping `lib/ui/Select`. It owns the `allowCustom` inline-number flow and its
  `__custom__` sentinel, which `ui/Select` never sees. Rendering one OUTSIDE a settings page — the onboarding wizard's
  language picker does — means passing `portalContainer`, so the menu lands in that modal's overlay rather than under
  its scrim (`ui/DETAILS.md` § Select). The optional `onPicked(value)` fires on a COMMITTED pick (click, or Enter on a
  row) and not on the highlight preview `handleHighlightChange` applies while the user moves through the list, so one
  call means one deliberate choice: that's the seam the two language pickers report `language_changed` from
  (`src-tauri/src/analytics/DETAILS.md` § The language events).
- `SettingToggleGroup`: segmented control for a short enum list.
- `SettingRadioGroup`: vertical radio for a longer list, an option needing a `customContent` snippet, or an option
  carrying a control on its own line (`itemTrailing`, as Brief mode's "Limit to" does).
- `SettingSlider` vs `SettingNumberInput`: see the next section, and § Index-mapped stops for the discrete mode. A
  `duration` setting on the number input edits in `constraints.unit` while the store stays in ms (`durationValueToMs` /
  `msToDurationValue`).
- `SettingPasswordInput`: masked input with a reveal toggle; two modes, below.
- `SettingColorSwatchPicker`: circle trigger plus a 4×4 swatch popover for pane tints. `swatch-keyboard.ts` is its pure
  key-index resolver, unit-testable without a DOM.

Every `.svelte` here ships a `*.a11y.test.ts` (axe tier-3); the swatch picker and `swatch-keyboard` also have functional
`*.test.ts`.

One setting rendered in two UI locations follows the parent's mirror pattern (`../CLAUDE.md`). The card-group frame
guard has a worked reference in `../sections/DETAILS.md`, under the Notifications section
(`behavior.fileSystemWatching.*`).

## Slider vs number input

A registry `component: 'slider'` row is a COARSE choice: `SettingSlider` renders `lib/ui/Slider` with a readout label
and no paired number field, so the value can only be dragged. A row where the user wants to type an exact number uses
`component: 'number-input'` and `SettingNumberInput` instead. ❌ Don't glue a number field back onto the slider: two
controls fighting over one value read as a bug, and the pair costs every slider row more width than it needs.

Practical fallout worth knowing:

- The registry's `sliderStops` feed the slider's ticks AND its magnetic snap targets, so a stop is both visible and
  sticky. A `number-input` row ignores `sliderStops`; it clamps to `min`/`max` and steps by `step`.
- Double-clicking the thumb resets to the registry default. There's no equivalent gesture on the number input; the row's
  reset pip (`SettingRow`) covers it.
- `maxOverride` exists for a ceiling that isn't known until runtime (image-index parallelism, capped at this machine's
  CPU count). The registry keeps a static fallback so search and off-runtime rendering still work.
- A slider's readout joins the value and `unit` with NO space (`125%`), and `ariaValueText` carries the same string so
  screen readers hear the unit too. A row whose number isn't a plain count passes `formatValue` instead (the Ask Cmdr
  wake cadence renders `30s` / `5m` / `2h` through `$lib/units`), and that also turns the spoken value on.

## Index-mapped stops

`constraints.stopsAreDiscrete` makes the stop table the ONLY reachable set of values, and runs the track over the stops'
INDICES instead of over `min`..`max`. The Ask Cmdr wake cadence is the reference row: its stops run 5 seconds to 2
hours, and on a linear track the first three would share a single pixel. In index space every stop gets equal travel,
and `ui/Slider` needs no change for it — `positionOf` is linear over min/max, which is correct in index space, and ticks
and snap targets are consumed in the same space.

- **Two number spaces, four crossings.** The track carries an index, the store carries the stop's value, and
  `slider-stops.ts` (`nearestStopIndex` / `stopAt`) is the only conversion. The four call sites are the `$state` seed,
  the `onSpecificSettingChange` arm, `commit`, and the double-click reset (which starts from the registry default, a
  stored value).
- **The index is never stored.** Reordering the table or inserting a stop would then silently change what every user
  chose. `CLAUDE.md` carries the two guardrails.
- **Placing a stored value goes through `nearestStopIndex`, not `indexOf`.** A value that isn't in the table — a
  hand-edited settings file, or a stop a later build retired — answers `-1` from `indexOf`, which reads as the first
  stop while the store still holds the old number. `SettingSlider.svelte.test.ts` pins both directions.
- **`ariaValueText` is mandatory here, not optional.** Ark hands the raw track value to `getAriaValueText`, so without
  the mapping back a screen reader announces "5" for a five-minute cadence. `SettingSlider` turns the spoken value on
  whenever `unit`, `formatValue`, or discrete mode is in play, rather than on `unit` alone.
- **`maxOverride` is ignored** in this mode: the stop table decides both ends.

## Password-input modes

`SettingPasswordInput` runs in one of two modes based on whether `onchange` is passed:

- **Store-driven (default):** `id` is mandatory and the component reads/writes the settings store directly. Use for
  passwords persisted as plain settings.
- **Controlled:** when both `value` and `onchange` are provided, the component bypasses the store, treats `value` as
  external state, and calls `onchange(newValue)` on every input. Use when the value lives in the OS secret store (AI
  cloud provider API keys) or any other backing store that isn't `settings.json`. In this mode `id` is still required
  for label/aria fallbacks but isn't read or written.

The `$effect` that syncs `internalValue` from `externalValue` is gated on `onchange` being set, and the
`onSpecificSettingChange` subscription is skipped in controlled mode so secret-store updates don't get clobbered by
stale store reads.

## `SettingsSidebar` and `SettingsContent` live here, not a sibling dir

They're paired with the primitives because the settings window has exactly one consumer of each (the `routes/settings`
page). Splitting them into `lib/settings/window/` would add a directory boundary that crosses the same import edges
back. They share the `SettingsSection` wrapper styling and the same registry hooks, so they sit alongside the rest.
