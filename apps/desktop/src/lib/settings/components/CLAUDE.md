# Settings UI primitives

Reusable setting-row primitives consumed by `lib/settings/sections/*.svelte`. Logic that isn't pure presentation lives
in the section or registry, not here. Parent: `../CLAUDE.md` (registry, store, sections, search).

## Module map

- Window chrome, not registry-driven: `SettingsSidebar` (nav + search; declares `TOP_LEVEL_ORDER`, keep in sync with
  `settings.spec.ts`), `SettingsContent` (routes `selectedSection` to a `sections/*.svelte` or `SectionSummary`),
  `SettingsSection` (h2 + slot), `SectionSummary` (card grid into subsections).
- Registry-driven rows: `SettingRow` (label + description + control + reset pip + restart badge) wrapping
  `SettingSwitch` / `Checkbox` / `Select` / `ToggleGroup` / `RadioGroup` / `Slider` / `NumberInput` / `PasswordInput` /
  `ColorSwatchPicker`. Which one to reach for: DETAILS § Picking a control.
- Every `.svelte` here is axe-audited (tier 3) from one directory-level file, `setting-components.a11y.test.ts`. Adding
  a primitive means adding a `describe` block there, not a new file; `a11y-coverage` fails if it can't see the import.

## Conventions

- **Registry-driven by default.** Every primitive except the four window-chrome files takes `id: SettingId` first, reads
  its metadata from the registry, subscribes via `onSpecificSettingChange(id, …)`, and writes via `setSetting(id, …)`.
  Passing label / options / min / max as props from a section means the setting isn't registered yet.
- **`SettingRow.split`** enforces a 50-50 grid so control left-edges align across rows. Use it for select / text /
  password / slider / number / radio / combobox rows, not for switches, toggle groups, or full-width custom layouts.
- **Card groups: wrap each row run in `{#if anyVisible(shouldShow, ...ids)}<SectionCard>`** (no wrapper component). The
  frame guard and each row's `{#if shouldShow(id)}` MUST read the SAME `shouldShow`, so an all-filtered-out card hides
  its frame. Visibility is section-owned, never re-derived from the registry `card` field. Why: `sections/DETAILS.md`.

## Gotchas

- **Don't classify state by label / option string.** The `id` is the contract, the label is documentation: branch on the
  value (`getSetting(id) === 'compact'`).
- **`SettingSelect`'s custom-value mode focuses the inline input via `setTimeout(0)`, not `tick()`.** Ark's `Select`
  closes on a microtask, so a same-tick focus call gets eaten by the trigger's returning focus. Changing this needs the
  a11y test plus a manual keyboard run.
- **`SettingColorSwatchPicker` keyboard nav stays in `swatch-keyboard.ts`**, the pure key-index resolver, so the
  traversal table is testable without a DOM. The component owns open/close, focus, and outside-click.
- **`SettingsSection`'s borderless title is intentional** (System Settings style). Don't add a `border-bottom`.
- **`SettingPasswordInput`'s controlled mode skips the store subscription** so secret-store updates aren't clobbered by
  stale store reads. DETAILS § Password-input modes.

Per-primitive catalog, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
