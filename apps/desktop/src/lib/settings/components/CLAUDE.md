# Settings UI primitives

Reusable setting-row primitives consumed by `lib/settings/sections/*.svelte`. Each registry-driven primitive reads its
metadata (label, options, min/max, default) from the settings registry by `id`, subscribes to
`onSpecificSettingChange(id, ...)` so external resets propagate, and writes through `setSetting(id, value)`. Section
components glue them together; logic that isn't pure presentation lives there or in the registry, not here.

Parent: [`../CLAUDE.md`](../CLAUDE.md) for the registry, store, sections, and search.

## File map

- **`SettingsSidebar.svelte`**: Left-column nav: search input, top-level + nested section list, "section has matches"
  highlight. Declares `TOP_LEVEL_ORDER` (keep in sync with the E2E test in `settings.spec.ts`)
- **`SettingsContent.svelte`**: Right-pane router: maps `selectedSection` to one of the 18 `sections/*.svelte`
  components, or renders `SectionSummary` for the four top-level sections that have subsections (`Appearance`,
  `Behavior`, `File systems`, `Developer`)
- **`SettingsSection.svelte`**: Shared section wrapper: h2 title + slot. One per `sections/*.svelte` so the title
  styling lives in one place
- **`SectionSummary.svelte`**: Card grid shown when a top-level section with subsections is selected; each card
  deep-links into a subsection
- **`SettingRow.svelte`**: Label + description + control + reset-pip + restart-required badge wrapper. Carries `split`
  (see below) and `searchQuery` (for `<mark>`-style label highlighting via `highlightMatches`)
- **`SettingSwitch.svelte`**: Primary boolean toggle (Ark UI `Switch`)
- **`SettingCheckbox.svelte`**: Less prominent boolean control (Ark UI `Checkbox`). Use for secondary booleans that hang
  off a switch or live inside a denser layout
- **`SettingSelect.svelte`**: Dropdown for enum settings: a thin registry wrapper around `lib/ui/Select`. Builds the
  items array (incl. its `__custom__` sentinel row) and owns the `allowCustom` inline-number flow; `ui/Select` never
  sees `__custom__`. Supports `allowCustom` from the registry, which renders an inline text input when the user picks
  `Custom…`
- **`SettingToggleGroup.svelte`**: Segmented control for short enum lists. Thin wrapper around `lib/ui/ToggleGroup` with
  `semantics="toggles"`. `labelOverrides` lets a button label track another reactive setting (for example, `kB` ↔ `KB`
  on binary/SI switch)
- **`SettingRadioGroup.svelte`**: Vertical radio list for longer enum lists or when each option needs a `customContent`
  snippet (description, sublabel, preview)
- **`SettingSlider.svelte`**: Slider + paired number input (Ark UI `Slider` + `NumberInput`). Reads `min` / `max` /
  `step` / `sliderStops` from the registry constraints
- **`SettingNumberInput.svelte`**: Standalone number input for settings without a slider (small ranges, exact values).
  Clamps to registry `min`/`max` on change
- **`SettingPasswordInput.svelte`**: Masked text input with reveal toggle. Two modes (see § "Password-input modes"
  below)
- **`SettingColorSwatchPicker.svelte`**: Circle trigger + 4×4 swatch popover for picking a tint color. Used by the
  per-volume-type pane tints under `Appearance > Colors and formats`
- **`swatch-keyboard.ts`**: Pure arrow-key/Home/End/PageUp/PageDown index resolver for the swatch grid. Extracted from
  the picker so the traversal arithmetic is unit-testable without a DOM

Each `.svelte` ships with `*.a11y.test.ts` (axe-core tier-3 audit). `SettingColorSwatchPicker` and `swatch-keyboard`
also have functional `*.test.ts` files.

## Conventions

### Registry-driven by default

Every primitive except `SettingsSidebar`, `SettingsContent`, `SettingsSection`, and `SectionSummary` takes
`id: SettingId` as its first prop and pulls everything else from `getSettingDefinition(id)`:

- Label, description, default, constraints (`min`, `max`, `step`, `options`, `sliderStops`, `allowCustom`).
- Current value via `getSetting(id)` (initial) + `onSpecificSettingChange(id, …)` (live updates from other windows or
  registry resets).
- Writes through `setSetting(id, value as SettingsValues[typeof id])`.

If you find yourself passing label / options / min / max as props from the section, register the setting properly
instead. The mirror-in-multiple-sections pattern (see parent CLAUDE.md) covers the case where one registry entry needs
to appear in two UI locations.

### 50-50 split layout (`SettingRow.split`)

`SettingRow` has a `split` prop that enforces a 50-50 grid (label left, control right). Keeps left edges of controls
vertically aligned across rows. Percentage-based, not pixel-based, because the settings window is resizable.

**Use `split`** for rows whose control is a select, text input, password input, slider, number input, radio group, or
combobox — anything that benefits from consistent horizontal alignment.

**Don't use `split`** for:

- Switches (too small; 50-50 wastes space and doesn't improve alignment).
- Toggle groups (multi-button controls that may not fit in 50% width at narrow window sizes).
- Full-width custom layouts (keyboard shortcuts table, license card, advanced auto-generated rows).

Description text below each row intentionally spans the full width.

### Password-input modes

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

### `SettingsSidebar` and `SettingsContent` live here, not a sibling dir

They're paired with the primitives because the settings window has exactly one consumer of each (the `routes/settings`
page). Splitting them into `lib/settings/window/` would add a directory boundary that crosses the same import edges
back. They share the `SettingsSection` wrapper styling and the same registry hooks, so they sit alongside the rest.

## Gotchas

### Don't classify state by label / option string

The `id` is the contract; the label is documentation. When you need to branch on which option is active, compare the
value (`getSetting(id) === 'compact'`), not the label string. Parent rule [`AGENTS.md`](../../../../../AGENTS.md) § "No
string-matching error or state classification" applies.

### `SettingSelect`'s custom-value mode

When `definition.constraints.allowCustom` is true and the user picks `Custom…`, the component renders an inline text
input and focuses it on the next event-loop tick via `setTimeout(0)`. The `setTimeout` (vs `tick()`) is deliberate: Ark
UI's `Select` finishes its own close animation on a microtask, and a same-tick focus call gets eaten by the returning
focus from the trigger. If you change this, verify with the a11y test and a manual keyboard run.

### `SettingColorSwatchPicker` keyboard nav lives in `swatch-keyboard.ts`

The picker component owns the popover open/close, focus management, and outside-click; Tab containment comes from the
shared `use:trapFocus` on the popover (see `lib/ui/DETAILS.md` § "Focus trapping"). The arrow-key index math is in the
pure module so the traversal table (Arrow keys, Home/End, PageUp/PageDown wrap rules) can be tested without a DOM. Keep
new keys in the pure helper.

### `SettingsSection` title styling is intentionally borderless

`System Settings`-style: larger bottom margin instead of a hairline rule. Don't add a `border-bottom` here without
checking the other section titles — the breathing room is the visual separator.
