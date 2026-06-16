# Settings UI primitives

Reusable setting-row primitives consumed by `lib/settings/sections/*.svelte`. Each registry-driven primitive reads its
metadata from the settings registry by `id`, subscribes to `onSpecificSettingChange(id, …)` so external resets
propagate, and writes through `setSetting(id, value)`. Section components glue them together; logic that isn't pure
presentation lives there or in the registry, not here. Full details: [DETAILS.md](DETAILS.md). Parent:
[`../CLAUDE.md`](../CLAUDE.md) (registry, store, sections, search).

## File map

Window chrome (not registry-driven; these four live here, not a sibling dir, because the settings page is their only
consumer):

- `SettingsSidebar.svelte`: left-column nav (search, section list, match highlight). Declares `TOP_LEVEL_ORDER` (keep in
  sync with the E2E test in `settings.spec.ts`).
- `SettingsContent.svelte`: right-pane router: maps `selectedSection` to a `sections/*.svelte`, or `SectionSummary` for
  the four top-level sections with subsections.
- `SettingsSection.svelte`: shared section wrapper (h2 title + slot). `SectionSummary.svelte`: card grid deep-linking
  into subsections.

Registry-driven controls (pick by control shape):

- `SettingRow.svelte`: label + description + control + reset-pip + restart-required badge. Carries `split` and
  `searchQuery`.
- `SettingSwitch.svelte`: primary boolean. `SettingCheckbox.svelte`: secondary boolean (hangs off a switch / denser
  layout).
- `SettingSelect.svelte`: enum dropdown (wraps `lib/ui/Select`); owns the `allowCustom` inline-number flow and its
  `__custom__` sentinel (`ui/Select` never sees it).
- `SettingToggleGroup.svelte`: segmented control for short enum lists. `SettingRadioGroup.svelte`: vertical radio for
  longer lists or when an option needs a `customContent` snippet.
- `SettingSlider.svelte`: slider + paired number input. `SettingNumberInput.svelte`: standalone number input (clamps to
  registry `min`/`max`).
- `SettingPasswordInput.svelte`: masked input with reveal toggle. Two modes (store-driven vs controlled); see
  [DETAILS.md](DETAILS.md) § Password-input modes.
- `SettingColorSwatchPicker.svelte`: circle trigger + 4×4 swatch popover for pane tints. `swatch-keyboard.ts`: pure
  arrow/Home/End/PageUp/PageDown index resolver (unit-testable without a DOM).

Each `.svelte` ships a `*.a11y.test.ts` (axe-core tier-3 audit); the swatch picker and `swatch-keyboard` also have
functional `*.test.ts`.

## Conventions

- **Registry-driven by default.** Every primitive except the four window-chrome files takes `id: SettingId` as its first
  prop and pulls label, description, default, and constraints from `getSettingDefinition(id)`; reads value via
  `getSetting(id)` + `onSpecificSettingChange(id, …)`; writes via `setSetting(id, …)`. If you're passing label / options
  / min / max as props from the section, register the setting instead. (The mirror-in-multiple-sections pattern in the
  parent CLAUDE.md covers one entry appearing in two UI locations.)
- **`SettingRow.split`** enforces a 50-50 grid (label left, control right) so control left-edges align across rows
  (percentage-based, since the window is resizable). Use it for select / text / password / slider / number / radio /
  combobox rows. Don't use it for switches (too small), toggle groups (may not fit 50% at narrow widths), or full-width
  custom layouts. Description text spans full width regardless.
- **Card groups: wrap each row run in `{#if anyVisible(shouldShow, ...ids)}<SectionCard>`** (no wrapper component). The
  frame guard and each row's `{#if shouldShow(id)}` MUST read the SAME `shouldShow`, so an all-filtered-out card hides
  its frame (no empty cards). Visibility is section-owned, never re-derived from the registry `card` field. Why +
  reference: `sections/DETAILS.md` (FSW).

## Gotchas

- **Don't classify state by label / option string.** The `id` is the contract; the label is documentation. Branch on the
  value (`getSetting(id) === 'compact'`), not the label. (`AGENTS.md` § no-string-matching applies.)
- **`SettingSelect`'s custom-value mode focuses the inline input via `setTimeout(0)`, not `tick()`.** Ark UI's `Select`
  finishes its close animation on a microtask, and a same-tick focus call gets eaten by the trigger's returning focus.
  If you change this, verify with the a11y test and a manual keyboard run.
- **`SettingColorSwatchPicker` keyboard nav lives in `swatch-keyboard.ts`.** The component owns popover open/close,
  focus, and outside-click; Tab containment comes from the shared `use:trapFocus` (see `lib/ui/DETAILS.md` § Focus
  trapping). Keep new keys in the pure helper so the traversal table stays DOM-free testable.
- **`SettingsSection` title styling is intentionally borderless** (System Settings-style: larger bottom margin, no
  hairline rule). Don't add a `border-bottom` without checking the other section titles; the breathing room is the
  separator.
- **`SettingPasswordInput` controlled mode skips the store subscription** so secret-store updates aren't clobbered by
  stale store reads. See [DETAILS.md](DETAILS.md) § Password-input modes.
