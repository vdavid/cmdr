# Settings UI primitives

Reusable setting-row primitives consumed by `lib/settings/sections/*.svelte`. Each registry-driven primitive reads its
metadata from the registry by `id`, subscribes to `onSpecificSettingChange(id, …)`, and writes through
`setSetting(id, value)`. Logic that isn't pure presentation lives in the section or registry, not here. Full details:
`DETAILS.md`. Parent: `../CLAUDE.md` (registry, store, sections, search).

## File map

Window chrome (not registry-driven; here, not a sibling dir, because the settings page is their only consumer):

- `SettingsSidebar.svelte`: left-column nav (search, section list, match highlight). Declares `TOP_LEVEL_ORDER` (keep in
  sync with `settings.spec.ts`).
- `SettingsContent.svelte`: right-pane router: maps `selectedSection` to a `sections/*.svelte`, or `SectionSummary` for
  the five top-level sections with subsections (Appearance, Behavior, Indexing, AI, File systems).
- `SettingsSection.svelte`: shared section wrapper (h2 title + slot). `SectionSummary.svelte`: card grid deep-linking
  into subsections.

Registry-driven controls (pick by control shape):

- `SettingRow.svelte`: label + description + control + reset-pip + restart-required badge. Carries `split` and
  `searchQuery`.
- `SettingSwitch.svelte`: primary boolean (wraps `lib/ui/Switch`). `SettingCheckbox.svelte`: secondary boolean (hangs
  off a switch / denser layout).
- `SettingSelect.svelte`: enum dropdown (wraps `lib/ui/Select`); owns the `allowCustom` inline-number flow and its
  `__custom__` sentinel (`ui/Select` never sees it).
- `SettingToggleGroup.svelte`: segmented control for short enum lists. `SettingRadioGroup.svelte`: vertical radio for
  longer lists, an option needing a `customContent` snippet, or an option carrying a control on its own line
  (`itemTrailing`, as Brief mode's "Limit to" does).
- `SettingSlider.svelte`: slider (`lib/ui/Slider`), readout label only, no paired number field, so the value is
  drag-only; `sliderStops` become both ticks and snap targets. `SettingNumberInput.svelte`: number input
  (`lib/ui/NumberInput`) for a typed exact value; a `duration` setting edits in `constraints.unit` while the store stays
  in ms (`durationValueToMs` / `msToDurationValue`). DETAILS § Slider vs number input.
- `SettingPasswordInput.svelte`: masked input with reveal toggle (two modes; see Gotchas).
- `SettingColorSwatchPicker.svelte`: circle trigger + 4×4 swatch popover for pane tints; `swatch-keyboard.ts` is its
  pure key-index resolver (unit-testable without a DOM).

Each `.svelte` ships a `*.a11y.test.ts` (axe-core tier-3 audit); the swatch picker and `swatch-keyboard` also have
functional `*.test.ts`.

## Conventions

- **Registry-driven by default.** Every primitive except the four window-chrome files takes `id: SettingId` first and
  pulls label, description, default, and constraints from the registry (the read/subscribe/write pattern above). Passing
  label / options / min / max as props from a section means the setting isn't registered yet. (One entry in two UI
  locations: the parent CLAUDE.md's mirror pattern.)
- **`SettingRow.split`** enforces a 50-50 grid (label left, control right) so control left-edges align across rows. Use
  it for select / text / password / slider / number / radio / combobox rows; not for switches, toggle groups, or
  full-width custom layouts. Description text spans full width regardless.
- **Card groups: wrap each row run in `{#if anyVisible(shouldShow, ...ids)}<SectionCard>`** (no wrapper component). The
  frame guard and each row's `{#if shouldShow(id)}` MUST read the SAME `shouldShow`, so an all-filtered-out card hides
  its frame (no empty cards). Visibility is section-owned, never re-derived from the registry `card` field. Why +
  reference: `sections/DETAILS.md` (FSW).

## Gotchas

- **Don't classify state by label / option string.** The `id` is the contract, the label is documentation: branch on the
  value (`getSetting(id) === 'compact'`). (`AGENTS.md` § no-string-matching.)
- **`SettingSelect`'s custom-value mode focuses the inline input via `setTimeout(0)`, not `tick()`.** Ark's `Select`
  closes on a microtask, so a same-tick focus call gets eaten by the trigger's returning focus. Changing this needs the
  a11y test plus a manual keyboard run.
- **`SettingColorSwatchPicker` keyboard nav lives in `swatch-keyboard.ts`.** The component owns popover open/close,
  focus, and outside-click; Tab containment comes from `use:trapFocus`. Keep new keys in the pure helper, so the
  traversal table stays DOM-free testable.
- **`SettingsSection` title styling is intentionally borderless** (System Settings-style: bottom margin, no hairline
  rule). Don't add a `border-bottom`.
- **`SettingPasswordInput` controlled mode skips the store subscription** so secret-store updates aren't clobbered by
  stale store reads. See `DETAILS.md` § Password-input modes.
