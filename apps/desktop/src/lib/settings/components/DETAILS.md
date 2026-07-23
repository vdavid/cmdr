# Settings UI primitives details

Depth and rationale for the setting-row primitives. `CLAUDE.md` holds the must-knows that prevent silent breakage.

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
  screen readers hear the unit too.

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
