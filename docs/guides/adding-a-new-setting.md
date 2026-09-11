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
- `macOSOnly: true` when the row renders only on macOS (the section gates its markup on `isMacOS()`). The setting then
  joins the search index only on macOS, so a Linux search can't land on a card that isn't there. A `hidden` setting
  still enters search, so a macOS-only feature's hidden flags take it too. See `lib/settings/DETAILS.md` § Searchable
  rows.
- `cardKey` when the page groups its rows into `SectionCard`s: set it to the SAME catalog key the card's title displays,
  so searching the card title surfaces the row. It's metadata only — it never decides whether the card renders (the
  section owns that via `visible`). See `lib/settings/DETAILS.md` § Card groups.

**Not a setting? Then it isn't a registry entry.** An action row that isn't a control ("Clear index", "Open log file",
"Get a license") is a `SearchableRow`, not a setting: see § "Adding a searchable non-setting row" at the end of this
guide. ❌ Never model one as a `hidden: true` setting to make it findable — that's a `SettingsValues` key nothing ever
reads or writes, and it's the pattern this replaced.

**A whole PAGE that isn't settings.** A page whose entire content is action rows (the trusted host keys, each with a
Forget button) has no control whose `section` would put it in the sidebar. ❌ Don't invent an entry here for it: one of
the page's own `SearchableRow`s carries `anchorsSection: { after: '<the sibling it follows>' }`, and the section tree
creates the node from that. `ServersSection.rows.ts` is the reference example. A page that does have a control needs
none of this.

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
matching your `component`. A slider whose stops span orders of magnitude (5 seconds to 2 hours) adds
`constraints.stopsAreDiscrete` so the track runs over the stops' indices; see `lib/settings/components/DETAILS.md` §
Index-mapped stops.

A row whose description has to name a live value (the Ask Cmdr cadence names both waits its setting produces) can't get
it from `descriptionKey`, which resolves to a static string. Pass a computed `tString(...)` to `SettingRow`'s
`description` prop instead, and keep the static registry text for the search index.

**The one exception:** `AdvancedSection` auto-renders every `section: ['Advanced']` setting from the registry. To make a
setting obscure, give it `section: ['Advanced']` plus a `cardKey` (its Advanced card group) and skip step 2 entirely —
don't hand-render it. A setting's `section` is its ONE home: it's either an Advanced auto-rendered row OR a
hand-rendered feature-page row, never both. Every non-Advanced section is bespoke: no row, no render.

## 3. If it changes backend behavior, wire live-apply

Settings apply immediately, never on restart. A setting that changes Rust-side behavior needs all three: (a) a Tauri
command, (b) a typed wrapper in `$lib/tauri-commands/settings.ts`, (c) an `onSettingChange` case in
`settings-applier.ts` that calls it. Pure frontend settings (read via `getSetting` where they're used) skip this.

## 4. If you're adding a whole new section

Register the route in `components/SettingsContent.svelte`, add the entry to `TOP_LEVEL_ORDER` in
`components/SettingsSidebar.svelte`, and mirror the order in `apps/desktop/test/e2e-playwright/settings.spec.ts` (it
asserts the section list).

## 5. If you change the stored format

Bump `SCHEMA_VERSION` and add a `migrateSettings()` case, or old `settings.json` files may fail to load.

## Verify

Open Settings, confirm the row shows in its section, and confirm searching one of its keywords surfaces it. Both, not
just one: a registry entry alone passes the search-index test but renders nothing.

## Adding a searchable non-setting row

Which mechanism? **A setting** stores a user choice: it has a value, a default, and someone reads it. **A searchable
row** is a button or a readout the section hand-renders, with nothing to store: "Clear index", "Check for updates",
"Forget everything". If you can't name what `getSetting(id)` would return, it's a row. **An OS-backed row** is the third
answer, for a control whose value is machine state Cmdr doesn't own and can't be told about (the "Show in Finder"
switch): no registry entry, no store key, read through to the OS on every mount, and not searchable. It's the rarest of
the three and the easiest to reach for wrongly, so before building one read `lib/settings/DETAILS.md` § OS-backed rows.

1. Add a `SearchableRow` to the `<Component>.rows.ts` beside the section component (create the file and add it to
   `lib/settings/sections/searchable-rows.ts` if the section has none yet). Give it a `row:`-prefixed id, the hosting
   page's `section`, the `labelKey` the row ALREADY renders (no new copy), the `cardKey` of its `SectionCard` if it sits
   in a titled one, and `keywords`.
2. Gate the markup on `shouldShow('row:…')`, and add the id to the card's `anyVisible(...)` guard. Skip the guard and a
   hit on the row opens the page with every card filtered away.
3. Nothing else: no `SettingsValues` key, no `SCHEMA_VERSION` bump, no applier case.
4. Only if the row's page has NO setting at all: add `anchorsSection: { after: '<the sibling subsection it follows>' }`
   to one of its rows, which is what puts the page in the sidebar. `ServersSection.rows.ts` is the one example.

Skip the row entirely when it only renders under some runtime state (a model being installed, a master toggle being on):
a search hit that scrolls to a row that isn't there is worse than no hit.

`lib/settings/DETAILS.md` § "Searchable rows" holds the rationale and the guardrails; `sections/searchable-rows.test.ts`
enforces them.
