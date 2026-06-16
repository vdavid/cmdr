# Settings card groups (third grouping level)

Created 2026-06-16.

## Problem and intent

Settings has two structural levels today: a **group** (top-level sidebar entry, e.g. `Behavior`) and a **subsection**
(the sidebar leaf you click, which renders as a page of rows, e.g. `File system watching`). These are
`section: [group, subsection]` in `settings-registry.ts`, and the sidebar is built from that array by
`buildSectionTree()`. Note "subsection" is already taken: it is the level-2 nav entry
(`SettingsSection.subsections`).

We want a **third level**: visual **card groups** within a page, so related rows render inside a `SectionCard` (the
macOS System Settings look). One page can hold several cards (e.g. `File system watching` → `Drive indexing`,
`Downloads notifications`, `Go to latest download`, `Low disk space`), a single card, or none.

`FileSystemWatchingSection.svelte` already hand-renders four `SectionCard`s, and that exposes two real bugs this plan
must fix and then prevent from recurring:

- **Empty cards in search.** Searching "drive index" filters the page to one matching row, but the other three cards
  still render as empty boxes: each `SectionCard` is drawn unconditionally and doesn't know its rows were all filtered
  out.
- **Blank page for a non-registry row.** Searching "index size" yields a blank pane and an empty sidebar. "Index size /
  Clear index" is a hand-rendered action row, not a registry setting, so search (which indexes only the registry)
  matches nothing, hides the whole section, and shows nothing.

The goal: cards group consecutive same-group rows **on normal pages AND in search results**, empty cards auto-hide, and
search stays correct, without every section reinventing the logic.

### What is automated vs manual (be precise — this is the crux)

This is **not** a registry-driven page renderer, and card visibility is **not** re-derived from the registry. Sections
keep hand-rendering their rows (they carry bespoke content: action buttons, the FDA hint, AI gauges, the index-size
readout) and they already own each row's visibility via `shouldShow(id)` (`createShouldShow(searchQuery)`,
settings-search.ts). What the system adds:

- **Manual (the section author):** wrap each run of rows in a card-group wrapper, and tell that wrapper whether it is
  visible, computed from the **same `shouldShow` predicate the rows use** (e.g. `visible={anyVisible('id1','id2')}`).
- **Automated:** the wrapper draws its `SectionCard` frame (background, border, `<h3>` label, anchor) **only when
  `visible`**, so empty cards vanish; and the card-group name is in the search index so the card title is findable.

So "search grouping works automatically" means: the author writes each card once (rows + one `visible` expression), and
both normal and search modes render correctly with no separate search-mode code path. The single source of truth for
"what is in this card and whether it shows" is the section markup, never a second registry-derived computation. (The
earlier draft had the wrapper re-derive visibility from a registry `card` field; the review showed that double-sources
visibility and re-creates the empty-card bug for non-registry and mirrored rows. Corrected below.)

## Design decisions (the why)

### D0. i18n: strings are catalog keys now (this constrains every string below)

The i18n runtime (i18n-readiness step 2) merged to `main` before this work. It changes the ground rules:

- The registry is authored as `SettingDefinitionSource[]` with `labelKey` / `descriptionKey` (`MessageKey`, from
  `$lib/intl/keys.gen`), and `resolveDefinition` (settings-registry.ts:1333) maps each to a `SettingDefinition` whose
  `label` / `description` are **lazy getters** calling `tString(key)` for the current locale. `keywords` and the
  `section` array stay plain English (not yet keyed).
- `buildSearchableText` (settings-search.ts) already concatenates the **resolved** `label` / `description` plus
  `section` + `keywords`, so the search index holds translated text. **Caveat (don't oversell):** the index snapshots
  resolved strings at build time and is **not** invalidated on locale change — `setLocale()` (messages.svelte.ts) clears
  the compiled-message cache but does **not** call `clearSearchIndex()`, which currently has no non-test callers. This is
  a pre-existing latent gap, harmless today because there is no in-app locale picker. This plan does not fix it and must
  not claim card titles re-translate live in search; if a picker ships, `setLocale()` must also `clearSearchIndex()`.
- A `no-raw-user-facing-string` ESLint rule (`apps/desktop/eslint-plugins/`) now **forbids literal UI strings**. Card
  titles are user-facing, so they must come from the catalog, never a literal.
- FSW already proves the pattern: `<SectionCard label={tString('settings.fileSystemWatching.cardDownloads')}>`,
  `...cardGoToLatest`, etc., and section titles via `tString('settings.section.fileSystemWatching')`. So card-title
  catalog keys for the reference page already exist and get reused.

Consequence: the new card field is a **`cardKey: MessageKey`**, not a plain string, and any new card title or anchor
label is a catalog entry plus a `MessageKey` codegen run, gated by `no-raw-user-facing-string`.

### D1. A separate `cardKey?: MessageKey` field, not a third element in `section[]`; metadata, not the visibility source

`buildSectionTree()` walks the **entire** `section` array (settings-registry.ts:1430), so adding `section[2]` would
create a spurious **third sidebar level**. The card axis is orthogonal to navigation, so it gets its own optional field.

- **Field is `cardKey?: MessageKey`** on `SettingDefinitionSource`, resolved to a lazy `card?: string` getter on
  `SettingDefinition` (mirroring `labelKey`→`label` at settings-registry.ts:1338). Name `cardKey` (not `subsection`,
  which already means the level-2 nav entry; if David prefers `cardGroupKey`, trivial to rename). **Implementation note
  (easy to miss):** add `card?: string` to `SettingDefinition`, AND add `'card'` to the source's `Omit`
  (`Omit<SettingDefinition, 'label'|'description'|'constraints'|'card'>`, types.ts:54) so the source carries only
  `cardKey`, not a stray `card`. In `resolveDefinition`, destructure `cardKey` out and add
  `get card() { return cardKey === undefined ? undefined : tString(cardKey) }`, paralleling the optional `description`
  getter (settings-registry.ts:1341). `keywords` / `section` stay on the source via `...rest` (plain, unresolved).
- **Its only jobs are descriptive:** (a) contribute the resolved card title to the setting's searchable text (D3), and
  (b) document membership / leave room for future tooling. It is explicitly **not** read to decide whether a card
  renders. Visibility is owned by the section (D2). This separation is what dodges the double-source bug.
- **Considered: no field, just `keywords`.** Pre-i18n, the card name could have ridden in each member's `keywords`. But
  `keywords` are plain English and untranslated, so they would not make a card title findable in another locale, whereas
  a resolved `cardKey` is translation-aware. With i18n, `cardKey` is the correct mechanism, not just the tidier one — so
  the field earns its keep more clearly now. (It still isn't authoritative for rendering; the section markup is.)
- **Mirroring (the "why", corrected):** a mirrored setting (e.g. `appearance.sizeColors`, canonical in
  `Colors and formats`, mirrored in `File and folder sizes`, sections/DETAILS.md) carries one `card` value for its
  canonical home. The subtle part: whether a **section** renders at all is decided one level up by
  `SettingsContent.sectionHasMatchingSettings`, which uses the **section-scoped** `getMatchingSettingIdsInSection`, while
  cards and rows use the **global** `shouldShow` (`getMatchingSettingIds`). So a mirror row on its non-canonical page is
  still governed by that outer section-scoped gate: searching the row's term makes the card's `visible` true, but the
  mirror page's section stays hidden (the term doesn't match that section's prefix), so the card never mounts. Net: a
  mirror row is **not** surfaced in search on its mirror page, exactly as today. This is a pre-existing limitation, not
  changed here. Surfacing mirrors in search is separate scope (it would need `getMatchingSettingIdsInSection` to honor
  mirror membership). The takeaway for this plan: don't claim the mirror "just works", and don't rely on a mirror row
  appearing in a card during search.

### D2. Card visibility is an inline `{#if anyVisible(...)}` guard around the existing `SectionCard` — no new component

**Terminology (settled, see D10):** the nav vocabulary `section` (any nav node / the registry path) and `subsection`
(a child section) is sound and stays. The third level is **not** a nav node; it is a visual grouping of rows into a card
inside a leaf section, so it stays in the *visual* vocabulary (`SectionCard`) and gets **no** `*Section*`-flavored name.
We do **not** add a `SettingCardGroup` / `CardSection` wrapper: it would re-overload "section" and transpose with the
existing `SectionCard` primitive. Instead the section conditionally renders the existing `lib/ui/SectionCard` directly:

```svelte
{#if anyVisible(shouldShow, 'indexing.enabled', 'indexing.indexSize')}
  <SectionCard label={tString('settings.indexing.enabled.label')} id={INDEXING_ANCHOR} gated={fdaClosed}>
    {#if shouldShow('indexing.enabled')}<SettingSwitch id="indexing.enabled" />{/if}
    {#if shouldShow('indexing.indexSize')}<!-- index-size readout + Clear index action -->{/if}
  </SectionCard>
{/if}
```

New surface is minimal: a one-line pure helper `anyVisible(shouldShow, ...ids) => ids.some(shouldShow)`, plus a `gated`
prop on `SectionCard` (D6) for the FDA dimming. `label` is a resolved/keyed title (`tString(cardKey)`, never a literal —
`no-raw-user-facing-string`, D0). `SectionCard` stays pure presentation (no store/IPC/registry read), safe in any
window including the restricted viewer window.

The `{#if anyVisible}` guard and each row's `{#if shouldShow}` read the **same** `createShouldShow(searchQuery)`
predicate, so the frame and its contents can never disagree: no empty frames, no hidden-but-non-empty cards. This is the
C2 fix and it removes any need for `getVisibleCards`/`cardHasVisibleRows` registry-derived helpers (a second source of
truth). (FSW's Drive-indexing card has no dedicated `card*` key today, so it reuses the row-label key; M2 may add
`settings.fileSystemWatching.cardDriveIndexing` for naming consistency with its `cardDownloads` siblings.)

**Rejected alternative:** a pure-CSS `:has()` auto-hide (frame `display:none` when it has no rendered rows). Elegant, but
`:has()` is unreliable in happy-dom (breaks unit tests) and adds a WebKit-version dependency; explicit `visible` is
predictable and testable. Noted, not chosen.

**No contiguity check.** The earlier draft proposed a registry-order contiguity guard. The review showed search grouping
is driven by **markup order**, not registry order (the page renders a flat match `Set` gated per-id in the author's
markup order), so a registry-contiguity invariant guards the wrong axis. With section-owned `visible`, the section is the
single source of both order and visibility, so the guard is unnecessary. Dropped.

### D3. Search indexes the card name; the match predicate is built once per page (already is)

- `buildSearchableText()` (settings-search.ts) gains the **resolved** `setting.card` (from `cardKey`, D1), so searching
  a card title (e.g. "downloads") surfaces its rows, translation-aware. **Append `card` at the END of the parts array
  (after `...keywords`).** `getMatchIndicesForLabel` (settings-search.ts:294) reconstructs label-highlight offsets
  assuming `section › … + ' ' + label` at the front; inserting `card` between section and label would silently
  mis-highlight labels. Appending is offset-safe. The card-title catalog keys for the reference page already exist (FSW's
  `cardDownloads` etc., D0); reuse them as the members' `cardKey`. Keep the member-`cardKey` and the `SectionCard`
  display key the **same** catalog key (a check could later assert it; not required for v1).
- The page already builds its match signal once: `SettingsContent.sectionHasMatchingSettings` calls
  `getMatchingSettingIdsInSection`, and each section builds `createShouldShow(searchQuery)` once. No per-card search
  pass is added (the previous draft's per-card recompute is avoided by construction).

### D4. Non-registry searchable rows get a hidden registry anchor (replaces the keyword band-aid)

"Index size / Clear index" has no registry entry, so it cannot be a search hit and its card cannot know to show. Fix it
the way the registry already models known-but-special state: add a `hidden: true` registry entry (working name
`indexing.indexSize`) with `labelKey` pointing at the existing catalog key `settings.fileSystemWatching.indexSize` (D0,
already used by the markup, so no new string) and `keywords: ['clear index', 'index database']` (English, like all
keywords today).

Why this is the right seam, not a band-aid:
- `buildSearchIndex` filters only `showInAdvanced`, **not** `hidden` (settings-search.ts:39), so a hidden entry **is**
  searchable; `buildSectionTree` skips `hidden` (settings-registry.ts:1425), so it never adds a nav entry. Hidden =
  "searchable identity without its own rendered control" is exactly what an action row needs.
- It makes the Drive-indexing card a first-class search hit: searching "index size" matches `indexing.indexSize` →
  `shouldShow('indexing.indexSize')` true → the action row renders and the card's `visible` is true → no blank page.
- **Sidebar lights correctly too:** the anchor flows into `getMatchingSections` (it isn't `showInAdvanced`), so "index
  size" now adds `Behavior` and `Behavior/File system watching` to the sidebar match `Set` (deduped) — exactly the bug
  fix; no mislighting, because the anchor's `section` is specific.
- Precedent: `behavior.fileSystemWatching.downloadsToastCollapsed`, `…globalGoToLatestShortcut.acknowledged` are hidden
  registry rows for state that isn't a normal control. An anchor extends that to "a searchable UI element that isn't a
  setting."

**Implementation reality (don't under-model it):** a hidden anchor is a **fully-modeled registry setting that happens to
be `hidden` and is never read or written**, not a free-floating search token. `SettingDefinition.id: SettingId` and
`SettingId = keyof SettingsValues` (types.ts), so the anchor needs:
- a `SettingsValues` entry: `'indexing.indexSize': boolean`,
- a concrete `type: 'boolean'` and `default: false` (it gets seeded into `settings.json` defaults like any key; that's
  fine, it's just never changed), `component` omitted, no `settings-applier` case.
- **No `SCHEMA_VERSION` bump / migration.** Adding a new key is backward-compatible: defaults are rebuilt from the
  registry and old files simply lack the key. The `SCHEMA_VERSION` must-know in `settings/CLAUDE.md` is about *format
  changes / renames* (e.g. the dateColors migration), not additive keys. State this in M1 so it doesn't stall.
- **Guardrail: the anchor's `section` MUST equal its hosting page's section.** The blank-page fix works precisely
  because `indexing.indexSize.section === ['Behavior','File system watching']`, so it lands in that page's
  section-scoped match set. An anchor under a different section would re-create the blank page.

This closes the "card made entirely of non-registry content can't be a search hit" hole: such a card gets an anchor.

### D5. `card` is ignored by the Advanced section (stated, not an oversight)

`AdvancedSection` renders a flat list from `getAdvancedSettings()` / `searchAdvancedSettings` with bespoke markup and
ignores `section`. It will also ignore `card`. Several M3-breakdown settings are `showInAdvanced` mirrors
(`fileOperations.maxConflictsToShow`, `progressUpdateInterval`, `network.smbConcurrency`): they keep their real `section`
+ `card` for their main page and remain an ungrouped flat row in Advanced. Document this so a reader doesn't expect
Advanced to grow cards.

### D6. Anchors and the FDA dimming wrapper must survive the migration

FSW today wraps cards in outer `<div id={…_ANCHOR_ID}>` for toast deep-links, and dims FDA-gated cards via a wrapper
whose rule targets the **inner** `.section-card` (`[data-gated='true'] :global(.section-card){opacity:.5}`). The
migration must preserve both:
- **Anchor `id`:** pass it to `SectionCard`'s `id` prop (rendered on `<section class="section-card-wrap" {id}>`) — the
  deep-link `getElementById(anchorId).offsetTop` scroll path (routes/settings/+page.svelte) is compatible. Note: under
  the `{#if anyVisible}` guard, the anchor element doesn't exist when `visible=false` during search; harmless (deep-links
  only fire when not searching).
- **`gated` prop on `SectionCard`:** the dimming selector targets the inner `.section-card`, and `data-gated` must sit on
  an element that **wraps** the card (still contains `.section-card`). Add a `gated` prop to `SectionCard` that emits the
  `data-gated` wrapper around its own frame (or keep the existing outer gating div in the section markup). Either way,
  don't break the `[data-gated] :global(.section-card)` relationship.

Verify the `navigate-to-section` deep-link still lands after the change.

### D7. Cross-section placement cleanup is a sign-off gate before rollout (M3)

Grouping rows into cards on top of misplacement paints over it. These change navigation, so they are David's call, not
assertions:

- **Recommend:** `fileViewer.suppressBinaryWarning` (Advanced → `Viewer`). Literally a viewer setting; `Viewer` has one
  row today (so this also softens the D8 single-row-card concern for that page).
- **Recommend:** split `Updates & privacy` into two cards (no move): `Updates` and `Privacy and data sharing`. Strongest
  win.
- **Ask David (do not assume):** are `appearance.showFunctionKeyBar` / `listing.directorySortMode` misplaced under
  `Appearance` vs `Behavior`? Arguable; moving either splits a coherent listing page. Default = leave, group within
  Appearance.

M1/M2 don't depend on these; M3 does.

### D8. Single-card and no-card pages: default to a card, verify visual weight in-app

macOS wraps even a single group, so default: a page with `cardKey`-tagged settings renders one `SectionCard` per group;
a page with none renders its rows in one default unlabeled `SectionCard` for consistency. **Open risk (verify in the
running app during M3):** a single short row (e.g. `Viewer` → "Word wrap", unless D7 adds a second row) may look heavier
inside a big card than bare. If so, allow a per-page opt-out (bare rows, no card). Not pre-committed.

### D9. Heading order is fine (resolved)

Page is `h1` (sr-only "Settings") → `h2` (`SettingsSection` title) → `h3` (`SectionCard` label). No skipped level; the
card `<h3>` is correct. `SectionCard` already has an a11y test; just ensure the FSW migration keeps the tier-3 axe pass.
Not an open risk.

### D10. Keep section/subsection; the third level is "a card", not a nav term

Audit result: the nav vocabulary is sound. `section` means any nav node / the registry path; `subsection` means a child
section (the i18n catalog ratifies this — every nav node at both levels is keyed flat as `settings.section.*`). The leaf
page being called a "section" (in `data-section-id`, the `SettingsSection.svelte` wrapper, and David's usage) is correct
under this model. So **no rename** of the existing levels. The one minor overload (`SettingsSection` is both the
recursive tree-node type and the leaf-page component) is left as-is (different namespaces). The third level is a *visual*
card, so it is named `SectionCard` (the existing primitive) and never gets a `*Section*`/`CardSection` name that would
re-overload the nav vocabulary or transpose with the primitive.

## Proposed card breakdown per page

Most pages stay one card. Splits worth doing (final groupings confirmed in M3 against the live app):

- **Appearance › Colors and formats** (10 rows): **Theme** (theme mode, app color) · **List coloring** (size colors,
  date colors, striped rows) · **Date and time** (date/time format, custom format) · **Pane tints** (local, SMB, MTP).
- **Appearance › Listing**: **Names and icons** (app icons, show extensions; plus sort directories / function key bar
  unless moved per D7) · **Brief mode** (column width mode, max width). (Weakest grouping; revisit after D7.)
- **Appearance › File and folder sizes**: single card. **Appearance › Zoom and density**: single card.
- **Behavior › File operations**: **Renaming** (extension changes) · **Conflicts and progress** (max conflicts,
  progress interval).
- **Behavior › File system watching**: keep the four — **Drive indexing** · **Downloads notifications** ·
  **Go to latest download** · **Low disk space**. (Reference migration.)
- **Behavior › Search**: single card.
- **File systems › SMB/Network shares**: **Connection** (enable networking, direct SMB) · **Performance and timeouts**
  (share cache, timeout mode, custom timeout, concurrency).
- **File systems › MTP / Git**, **Viewer**, **Developer › MCP server / Logging**: single card each.
- **Updates & privacy**: **Updates** · **Privacy and data sharing** (per D7).
- **AI**: leave custom (own state machines); optional single card later, out of scope.

## Milestones

### M1. Registry + search foundation (no visible UI change)

Intent: land the data model and search wiring with zero visual change.

- Add `cardKey?: MessageKey` to `SettingDefinitionSource` and a resolved lazy `card?: string` getter in
  `resolveDefinition` (`types.ts` + `settings-registry.ts`), mirroring `labelKey`→`label`. Doc comment:
  **descriptive/searchable only, not read for visibility** (visibility is section-owned, D2); distinct from `section`.
- Add the resolved `setting.card` to `buildSearchableText()` (D3).
- Add the hidden `indexing.indexSize` anchor (D4): a fully-modeled `hidden:true` setting — add its key to
  `SettingsValues`, `type:'boolean'`, `default:false`, `labelKey: 'settings.fileSystemWatching.indexSize'` (existing),
  English `keywords`, no `component`, no applier case, **no `SCHEMA_VERSION` bump** (additive key). Confirm
  hidden-but-searchable (`buildSearchIndex` keeps `hidden`).
- Set `cardKey` on `File system watching`'s settings (reuse existing FSW card-title keys); other pages in M3.
- i18n: any genuinely new card title (none for FSW; some in M3) needs a catalog entry + a `MessageKey` codegen run.

Tests (test-first, red→green where it's logic):
- `settings-search.test.ts`: a setting's resolved card title appears in its searchable text; searching a card title
  returns the setting; searching "index size" returns the `indexing.indexSize` anchor.
- `settings-registry.test.ts`: existing invariants hold with the new optional field; `resolveDefinition` resolves
  `cardKey`→`card`; the hidden anchor is excluded from `buildSectionTree` and included in the search index.

Docs: `settings/CLAUDE.md` one-line must-know (the `cardKey` field is metadata; **card visibility is owned by the
section's `visible`, never re-derived from `card`** — guardrail against reintroducing the empty-card bug);
`settings/DETAILS.md` Decision/Why for D0/D2/D4; `docs/guides/adding-a-new-setting.md` (set `cardKey` when the page
groups; add a hidden anchor for a searchable non-setting row).

Checks: `pnpm check --fast`; then `pnpm check svelte` (incl. `no-raw-user-facing-string` and the i18n catalog/codegen
checks) for the touched TS.

### M2. `anyVisible` helper + `SectionCard` `gated` prop + reference migration (FileSystemWatchingSection)

Intent: prove the pattern on the broken page, fixing both bugs, with no new component (D2).

- Add the pure `anyVisible(shouldShow, ...ids) => ids.some(shouldShow)` helper (in `settings-search.ts` or a small
  sibling), unit-tested.
- Add a `gated?: boolean` prop to `lib/ui/SectionCard.svelte` that emits the `data-gated` wrapper (D6) so callers stop
  hand-rolling the dimming div. Keep `SectionCard` pure presentation.
- Migrate `FileSystemWatchingSection.svelte`: wrap each card's rows in `{#if anyVisible(shouldShow, ...memberIds)}` and
  render the existing `SectionCard` (with `id` anchor + `gated`), preserve the FDA dimming, and gate the index-size
  action row on `shouldShow('indexing.indexSize')`.

Tests (bug fixes → real red→green per David's user-level `tdd-red-green` rule; the two bugs live at **different layers**,
so split them):
- FSW section level (`FileSystemWatchingSection.svelte.test.ts`, reuse its IPC mock): mount with
  `searchQuery="drive index"` → Drive-indexing card renders, the other three do **not** (pre-fix: assert the empty cards
  exist, then flip). This bug is owned by the section/cards.
- Page level (`SettingsContent` test): searching "index size" keeps the `File system watching` section visible and shows
  the Drive-indexing card (not a blank pane). This bug is `sectionHasMatchingSettings` hiding the section; its red→green
  belongs here, not in the FSW section test.
- `anyVisible` pure-helper unit tests (match, no-match, empty query).
- a11y: extend `SectionCard`'s existing a11y test if needed for the `gated` prop; confirm the FSW tier-3 axe pass holds
  (D9 heading order). No new catalog component.

Docs: `components/CLAUDE.md` and/or `lib/ui` DETAILS (the `{#if anyVisible}` + `SectionCard` pattern; **the `anyVisible`
guard reads the same `shouldShow` as the rows**; `gated` prop); `sections/DETAILS.md` FSW entry notes the inline-guard
pattern; record D2/D4/D10 as Decision/Why.

Checks: `pnpm check --fast` while iterating; `pnpm check desktop`; FSW functional + a11y suites; settings E2E
(`settings.spec.ts`).

### M3. Roll out to the remaining pages (gated on D7 decisions)

Intent: apply the breakdown mechanically after placements are settled.

- Resolve D7 placement decisions with David first.
- For each page: set `cardKey` on its settings (add catalog entries + a `MessageKey` codegen run for any new card
  title), wrap each card's row run in `{#if anyVisible(...)}<SectionCard>` (D2). Add a hidden anchor for any non-registry
  searchable row (none known beyond index-size today). Single-card pages get one unlabeled `SectionCard`, subject to the
  D8 visual check.
- Visual pass in the running app (D8): confirm single-row pages don't read as too heavy; opt out per-page if they do.

Tests: each page's `*Section.a11y.test.ts` updated; add a search-empty-card assertion to a representative multi-card page
(e.g. `Updates & privacy`); E2E `settings.spec.ts`: a cross-card search groups correctly; sidebar order unchanged.

Docs: each touched `sections/DETAILS.md` entry notes its card breakdown.

Checks: full `pnpm check` per milestone; `pnpm check --include-slow` before wrapping (E2E).

## Parallelization

M1 → M2 sequential (M2 needs the field, anchor, and search wiring). Within M3, per-page migrations are independent but
touch sibling files and the shared registry; do the M3 registry `cardKey` edits + catalog additions in one pass, then
wrap pages one at a time. Sequential is fine and lower-risk.

## Definition of done

- Cards group same-group rows on normal pages and in search; empty cards never render; card titles are searchable.
- Card visibility is computed from the same `shouldShow` the rows use (single source) — no registry-derived second path.
- The two reported FSW bugs have regression tests that were red before the fix, at the correct layers.
- The index-size row is searchable via its hidden anchor; the "pure-action card" hole is closed by the anchor pattern.
- `cardKey` is documented as metadata-only and ignored by Advanced; card titles come from the catalog (no raw strings,
  D0); D7 placements resolved with David; D8 single-card visual check done in-app.
- No new wrapper component; the third level is an inline `{#if anyVisible}<SectionCard>` (D2/D10); section/subsection
  vocabulary unchanged.
- `pnpm check` (incl. a11y and E2E) green; docs in sync, with the D2/D4/D10 decisions captured as Decision/Why.
