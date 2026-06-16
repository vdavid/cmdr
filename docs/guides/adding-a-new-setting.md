# Adding a new setting

How to add a user setting end to end. The trap that bites everyone is step 2: a registry entry alone does **not**
render. Most sections hand-render their rows, so a setting with no matching row is invisible in the UI even though it
exists, persists, and is searchable in theory.

## 1. Declare it in the registry

Add an entry to `settings-registry.ts`. Name the id after the UI vocabulary (`whatsNew.showOnUpdate`, not
`changelog.popupEnabled`):

- `id`, `section` (the sidebar path, for example `['Updates & privacy']` or `['Appearance', 'Listing']`)
- `label`, `description`: sentence case, friendly, per the style guide
- `keywords`: extra search terms so the setting is findable (for example `['changelog', 'release notes']`)
- `type` + `default` + `component` (`switch`, `number`, `select`, `slider`, `radio`, `toggle-group`, `checkbox`,
  `color`, `text-input`)
- `hidden: true` for internal state with no UI (for example a "last seen version" stamp). Hidden settings live in the
  same store and sync across windows but never render, so **skip step 2 for them**.
- `cardKey` when the page groups its rows into `SectionCard`s: set it to the SAME catalog key the card's title displays,
  so searching the card title surfaces the row. It's metadata only — it never decides whether the card renders (the
  section owns that via `visible`). See `lib/settings/DETAILS.md` § Card groups.

**Searchable non-setting rows (hidden anchor).** A hand-rendered action row that isn't a real control (for example
"Index size / Clear index") can't be a search hit on its own, so its card can't know to show. Give it a `hidden: true`
registry anchor whose `section` EQUALS the hosting page's section, reusing the row's existing label key. It's searchable
(`buildSearchIndex` keeps hidden entries) but adds no nav row (`buildSectionTree` skips them). `indexing.indexSize` is
the reference example.

Also add the key and its value type to the `SettingsValues` interface in `types.ts`. This isn't optional bookkeeping:
`SettingDefinition.id` is typed as `SettingId` (= `keyof SettingsValues`), so a registry entry whose id is missing from
`SettingsValues` fails `svelte-check` right at the registry entry. That compile error is the prompt to add the key (it's
what makes `getSetting`/`setSetting` type-safe for the new id, no cast needed).

## 2. Render it in the section component (the step that's easy to miss)

`components/SettingsContent.svelte` routes each sidebar entry to one `sections/*Section.svelte` by
`getSettingDefinition(id).section`. **Most sections hand-render every row**, so add yours there, wrapping the control
that matches the registry `component` in a `SettingRow`, guarded by `shouldShow(id)` so search filtering works:

```svelte
const fooDef = getSettingDefinition('group.foo') ?? { label: '', description: '' }
...
{#if shouldShow('group.foo')}
    <SettingRow id="group.foo" label={fooDef.label} description={fooDef.description} {searchQuery}>
        <SettingSwitch id="group.foo" />
    </SettingRow>
{/if}
```

The control components live in `settings/components/` (`SettingSwitch`, `SettingNumberInput`, `SettingSelect`,
`SettingSlider`, `SettingRadioGroup`, `SettingToggleGroup`, `SettingCheckbox`, `SettingColorSwatchPicker`); pick the one
matching your `component`.

**The one exception:** `AdvancedSection` auto-renders any setting tagged `showInAdvanced: true` from the registry, so
don't hand-render those there. Every other section is bespoke: no row, no render.

## 3. If it changes backend behavior, wire live-apply

Settings apply immediately, never on restart. A setting that changes Rust-side behavior needs all three: (a) a Tauri
command, (b) a typed wrapper in `$lib/tauri-commands/settings.ts`, (c) an `onSettingChange` case in
`settings-applier.ts` that calls it. Pure frontend settings (read via `getSetting` where they're used) skip this.

## 4. If you're adding a whole new section

Register the route in `components/SettingsContent.svelte`, add the entry to `TOP_LEVEL_ORDER` in
`sections/SettingsSidebar.svelte`, and mirror the order in `apps/desktop/test/e2e-playwright/settings.spec.ts` (it
asserts the section list).

## 5. If you change the stored format

Bump `SCHEMA_VERSION` and add a `migrateSettings()` case, or old `settings.json` files may fail to load.

## Verify

Open Settings, confirm the row shows in its section, and confirm searching one of its keywords surfaces it. Both, not
just one: a registry entry alone passes the search-index test but renders nothing.
