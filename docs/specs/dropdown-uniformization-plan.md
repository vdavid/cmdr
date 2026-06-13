# Dropdown uniformization — implementation plan

Status: draft for execution. Hub worktree: `.claude/worktrees/dropdowns` (branch `dropdowns`, off local `main`).
Conversion milestones run on their own sub-worktrees branched off `dropdowns`, reconciled back into it.

## Why this exists

The app grew several unrelated dropdown idioms with no shared base: a native `<select>` here, an Ark UI `Select` wrapper
there (`SettingSelect`), and two byte-for-byte-duplicated hand-rolled model comboboxes (AI settings + onboarding). The
AI Settings model picker is the trigger: it never loads its model list on open (only on a key/URL edit), its tiny `▾`
glyph is barely clickable, and its control shape morphs between a plain input and a custom listbox. David wants one
clean, reusable, macOS-y dropdown architecture, and the whole app converged onto it wherever reasonable.

**Mental model:** there are exactly two value-picker primitives worth having, both built on Ark UI (already a project
dep, the basis of `SettingSelect`) and both styled to our macOS-y house look:

- **`lib/ui/Select.svelte`** — pick one of a fixed list. No free text. (Native `<select>` replacement.)
- **`lib/ui/Combobox.svelte`** — pick from a list OR type your own; list can load async. (Model-picker replacement.)

Everything else stays. Category-D popovers/menus (`Dropdown`, `FilterDropdown`, `VolumeBreadcrumb`, context menus,
swatch picker) are a different primitive — not in scope. `CommandPalette` is a bespoke fuzzy+recents+two-cursor launcher;
forcing it onto Ark buys risk, not maintainability — explicitly out of scope.

**Design values that drive the choices** (from `docs/design-principles.md`, `AGENTS.md` § Principles): platform-native
macOS look, keyboard-first but full mouse support, AA+ contrast in dark and light, respect `prefers-reduced-motion` and
system text size, radical transparency (show the model list loading, don't hide it). Every user-facing string follows
`docs/style-guide.md` (sentence case, active voice, no "error/failed", en dashes not em). Keyboard a11y and ARIA come
from Ark for free — that's a primary reason to adopt it over hand-rolled listboxes.

## Root-cause findings (verified against the code, not assumed)

1. **Model list never loads on open.** `AiCloudSection.svelte` populates `availableModels` only inside
   `triggerConnectionCheck()`, which fires on an API-key save, a base-URL edit, or a manual "Test connection"/"Recheck".
   On mount it runs `loadCloudProviderConfig()` (loads the saved key) but schedules **no** check. So `availableModels`
   stays `[]` and the Model row renders its plain-`<input>` fallback (`AiCloudSection.svelte:503`). The combobox
   literally cannot appear on open. **Fix:** on mount, if the config is checkable (has key/URL) and there's no fresh
   cached list, kick off the check. `GET /models` does not consume provider token credits on the major providers, so
   the existing 1000 ms debounce is enough — no need for lazier triggers. (David, this turn.)

2. **The model list is component-local and lost on every close.** `availableModels` is `$state` inside the section, so
   leaving and reopening Settings refetches. **Fix:** a shared, session-scoped cache keyed by a config fingerprint. The
   fingerprint is a **SHA-256 hex digest of `providerId + "\0" + baseUrl + "\0" + apiKey`** via Web Crypto
   (`crypto.subtle.digest`, available in the webview). NEVER key on the key's length (two equal-length keys collide and
   serve a stale/wrong list — revoked-vs-new is the exact bug the cache must avoid), and NEVER store or log the raw key
   or the digest input. Put it in a small module under `lib/settings/` (e.g. `ai-model-cache.ts`), a process-lifetime
   in-memory `Map`. A successful refetch updates the cache; a key/URL change is a different digest, so it misses and
   refetches.

3. **Two hand-rolled model comboboxes are duplicates.** `AiCloudSection.svelte:433–502` and
   `CloudProviderSetup.svelte:518–576` are the same input+listbox+keyboard-state-machine. Both collapse into one
   `lib/ui/Combobox` usage — this is the biggest maintainability win. **But note:** only the *listbox markup* is
   duplicated. `CloudProviderSetup` ALREADY loads its model list on open (`loadApiKeyForProvider` triggers
   `triggerConnectionCheck()` immediately on stored-key load, `CloudProviderSetup.svelte:126–130`). The "never loads on
   open" bug (finding #1) is **specific to `AiCloudSection`**. So M1 adds the mount-trigger to `AiCloudSection` ONLY;
   adding one to onboarding would double-fire.

4. **Ark Combobox value model — the load-bearing correction.** Ark's Combobox (zag `@1.40.0`) defaults to
   `selectionBehavior: "replace"`, and on any `value` change it runs `syncSelectedItems` → `stringifyMany`, which
   **drops any value not present in the collection** and writes `inputValue = ""`. So driving the model picker as a
   plain `value` + `items` select would BLANK the field whenever the list is empty (cold start / mid-fetch) or the model
   is a custom name not in `/models` — exactly the regression we forbid. Therefore the model `Combobox` must be a
   **text-field-with-suggestions**, not a select:
   - Control `inputValue` + `onInputValueChange` separately from `value` (both are Ark-bindable). The displayed text is
     the saved/typed model string, NEVER derived from collection membership.
   - Set `selectionBehavior="preserve"` so a typed custom value survives a list sync.
   - `allowCustomValue` only governs accepting the custom value on close; it does NOT control rendering — the
     `inputValue` axis does. Wire both.
   - `openOnClick` defaults `false`; the old UX opened on focus and on the `▾` button. Wire open-on-focus / Trigger so
     focusing the field opens the suggestions, and the empty/loading collection renders a graceful state, not a broken
     popup. `loading` is OUR overlay (Ark has no loading prop).

5. **`SettingSelect` is the de-facto house dropdown but isn't reusable outside settings.** It hard-couples Ark `Select`
   markup + styling to the registry (`getSettingDefinition(id)`). Extract the presentational Ark `Select` into
   `lib/ui/Select.svelte`; `SettingSelect` becomes a thin registry wrapper that builds the items array (including its
   `allowCustom` "Custom…" marker) and owns the inline-number-input flow, delegating rendering to `ui/Select`. The three
   `SettingSelect` consumers (`AppearanceSizesSection`, `AiLocalSection`, `NetworkSection`) must be unaffected
   (byte-identical behavior). **`ui/Select`'s contract is wider than a plain value-picker:** `SettingSelect` does
   immediate-apply-on-highlight (`handleHighlightChange`) AND toggles a `custom-highlighted` class on the content to
   suppress other items' checked state while "Custom…" is highlighted, AND renders per-item `description` text. So
   `ui/Select` must forward `onHighlightChange`, allow a content-level class hook, and accept a per-item description. The
   `__custom__` interception and the inline-number-input branch stay in `SettingSelect` (so the `setTimeout(0)`
   focus-restore quirk is untouched), but `SettingSelect`'s `handleCustomSubmit` focuses `.select-trigger` via
   `querySelector` — so `ui/Select`'s class names (`.select-trigger`, `.select-item`, `.select-content`,
   `.option-description`) and `ariaLabel`-on-trigger wiring are a **documented stable contract**, not free to rename.

5b. **Tiny chevrons everywhere.** The combobox uses `&#x25BE;` at `--font-size-sm` (12px); `SettingSelect`'s indicator is
   `▼` at `--font-size-xs` (10px). Standardize on a single Lucide `chevron-down` (`~icons/lucide/chevron-down`) at a
   readable size with a real hit-area, shared by both `ui/Select` and `ui/Combobox`.

6. **`@ark-ui/svelte/combobox` is available but unused.** Confirmed in `node_modules`. Mirror `SettingSelect`'s
   `@ark-ui/svelte/select` usage (`createListCollection`, `Root`/`Control`/`Trigger`/`Content`/`Item`). Read
   `apps/desktop/node_modules/@ark-ui/svelte/dist/components/combobox` for the exact API before coding (input value vs.
   selected value, `allowCustomValue`/open-on-click, highlight events).

7. **No new IPC.** `checkAiConnection` already returns the model list. No `bindings:regen`, no Rust change.

## Conventions for this work

- TDD where marked **[TDD]** (per `tdd-red-green.md`): write the failing test, SEE it fail for the right reason, then
  implement. Mandatory for the model-cache fingerprint/invalidation logic and the load-on-mount trigger.
- Each new `lib/ui/` primitive ships a `*.a11y.test.ts` (axe tier-3) like the other primitives, plus a Debug > Components
  catalog section (`routes/dev/components/sections/`) wired into `routes/dev/components/+page.svelte`'s `SUB_IDS` and
  mirrored in the catalog E2E spec. See `docs/specs/component-catalog-plan.md` for the catalog contract.
- Keep colocated `CLAUDE.md` (must-knows) + `DETAILS.md` (depth) in sync (`docs-maintenance.md`): `lib/ui/` gets the two
  new primitives documented; `settings/components/CLAUDE.md` updates the `SettingSelect` line to note it wraps
  `ui/Select`.
- The a11y-contrast checker models select highlighted/checked states in `scripts/check-a11y-contrast/dropdown_states.go`.
  Reuse the existing `--color-accent` / `--color-accent-fg` token pairs that `SettingSelect` already uses so the matrix
  still passes; if you introduce a new state color, extend the matrix.
- Don't string-match on labels/option text to classify state (`no-string-matching` rule); the value/`id` is the
  contract.
- Run `pnpm check --fast` every few edits; `pnpm check` before each commit; `pnpm check --include-slow` before declaring
  a milestone done; always include `oxfmt`. Never tail/head checker output. Smoke-test 1–2 specs after touching test
  infra before a full run.
- Commit per milestone (or finer), lead-with-impact messages, no co-author lines. Don't push.
- Don't add to / raise `file-length-allowlist.json`, `claude-md-length-allowlist.json`, or `docs-reachable-allowlist.json`
  without surfacing it. Extracting the comboboxes should SHRINK `AiCloudSection`; let the checker shrink-wrap entries.

---

## Milestone M0 — Foundation primitives (runs first, on the hub branch; orchestrator reviews diff in full)

**Scope:** Create `lib/ui/Select.svelte` and `lib/ui/Combobox.svelte`; refactor `SettingSelect` to wrap `ui/Select`;
add the shared Lucide chevron; add both Debug > Components catalog sections; write a11y tests; document in
`lib/ui/CLAUDE.md` + `DETAILS.md`.

**Intentions:**

- `ui/Select`: presentational, items-driven. Props (final names at the implementer's discretion, documented in
  `DETAILS.md`): `items` (value + label + optional `description` + optional `group`/optgroup label), `value`, `onChange`,
  **`onHighlightChange`** (SettingSelect applies on highlight), `disabled`, `placeholder`, `ariaLabel`, plus a
  **content-level class hook** (SettingSelect needs to set `custom-highlighted` on the content). Supports grouped items
  (Ark `ItemGroup`/`ItemGroupLabel`, for `EncodingPicker`) and a per-item `description` (for `SettingSelect`'s option
  descriptions — NOT for TransferDialog, see M2). Standardized chevron. **Stable class contract** (`.select-trigger`,
  `.select-item`, `.select-content`, `.option-description`) and `ariaLabel`-on-trigger: SettingSelect's
  `handleCustomSubmit` focuses `.select-trigger` by `querySelector`, and the contrast matrix + consumer a11y tests key on
  these names. Reuses `SettingSelect`'s existing `.select-*` styling/token choices so contrast holds.
- `ui/Combobox`: presentational, **text-field-with-suggestions** (per finding #4, NOT a value-bound select). Controls
  `inputValue` + `onInputValueChange` separately from `value`; `selectionBehavior="preserve"`; `allowCustomValue` true;
  open-on-focus wired (`openOnClick`/Trigger). Props like: `items`, `inputValue`, `onInputValueChange`, `loading`
  (our own in-field spinner overlay — Ark has no loading prop), `placeholder`, `ariaLabel`. The control shape never
  morphs — it's always the combobox, showing the current `inputValue` even with an empty/loading list, and a typed
  custom value never snaps back or blanks.
- `SettingSelect` keeps its registry reads + `allowCustom` "Custom…" inline-number flow (incl. the `setTimeout(0)`
  focus-restore), builds an items array, and renders `ui/Select`; the `__custom__` sentinel + the inline-number-input
  branch stay in `SettingSelect`, so `ui/Select` never sees `__custom__`.

**Landmines:**

- Finding #4 is the trap: do NOT bind the model picker's text to `value`/collection membership, or it blanks on cold
  start, mid-fetch, and custom names. Verify in the catalog with an empty `items` list that the field still shows its
  `inputValue` and focusing opens a graceful empty state.
- Keep `SettingSelect`'s `setTimeout(0)` custom-input focus quirk (documented in `settings/components/CLAUDE.md`) — it's
  load-bearing against Ark's close animation. Don't rename the stable `.select-*` classes.
- Don't regress the contrast matrix: keep the highlighted/checked item colors on the existing accent tokens. If class
  names change at all, update the `Selector` strings in `scripts/check-a11y-contrast/dropdown_states.go` (it keys on
  literal selector strings and would otherwise silently validate a dead selector).
- `prefers-reduced-motion`: `SettingSelect` has NO entrance animation today — match that (no animation) as the safe
  default. Native `<select>`s being replaced had an OS open animation; we now own the transition, so any polish anim
  must be gated behind `prefers-reduced-motion`.
- Ark Select/Combobox don't portal to `document.body` by default (SettingSelect doesn't wrap in `Portal`) — keep it that
  way so the viewer's restricted capability set is unaffected.
- Catalog wiring lives in TWO files: `routes/dev/components/+page.svelte` (`SUB_IDS` + import + render) AND
  `routes/debug/+page.svelte` (the component-id union type + sidebar order). M0 edits both. (M3 must touch neither.)

**Test plan:** `ui/Select.a11y.test.ts` + `ui/Combobox.a11y.test.ts` (axe tier-3); a functional test for Combobox
free-text persistence + empty/loading state (assert the field keeps `inputValue` with empty `items`); the three existing
`SettingSelect` consumers still pass their a11y/behavior tests; `pnpm check` green. Manual: open Debug > Components, both
new sections render and are keyboard-navigable.

**DONE:** Both primitives exist, cataloged, documented, tested; `SettingSelect` delegates to `ui/Select` with identical
behavior; chevron standardized; `pnpm check` + relevant a11y green on the hub branch.

---

## Milestone M1 — AI sections: model combobox + load-on-mount + shared cache (parallel sub-worktree `dropdowns-ai`)

**Scope (files): `AiCloudSection.svelte`, `onboarding/CloudProviderSetup.svelte`, new `lib/settings/ai-model-cache.ts`,
plus `AiCloudSection`'s provider `<select>`.** This milestone owns ALL AiCloudSection dropdowns to avoid file conflicts
with other milestones.

**Intentions:**

- Replace both hand-rolled model comboboxes with `ui/Combobox` (text-field-with-suggestions per finding #4). Free-text
  preserved. Drive the field's text off `inputValue` = the saved/typed model; wire `loading` to the `'checking'` state.
- `ai-model-cache.ts`: session-scoped `Map<digest, string[]>`, digest = SHA-256 hex of `providerId\0baseUrl\0apiKey`
  (Web Crypto, async). On mount of `AiCloudSection`, if checkable: serve cache instantly on hit, else trigger the check
  (fills cache on success). Keep the 1000 ms debounced refetch on key/URL edits; a successful refetch updates the cache.
  Never store/log the raw key or the digest input. **[TDD]** the fingerprint (collision-free for different keys) +
  invalidation (key/URL change → miss).
- **`triggerConnectionCheck()` currently zeroes `availableModels` at the start of every check
  (`AiCloudSection.svelte:133`, `CloudProviderSetup.svelte:173`). Stop zeroing during a refetch** — keep the prior list
  (or, since the field text is driven by `inputValue` now, decouple entirely) so the combobox never blanks mid-check.
- Replace the provider `<select>` with `ui/Select` fed from `cloudProviderPresets`; preserve the provider-description
  paragraph and `handleCloudProviderChange` (which itself fires a `setTimeout(0)` check — ensure `onChange` fires once,
  no double-trigger with the mount-trigger).

**Landmines:**

- The model field shows the saved model immediately even before the list loads — never blank it during fetch (finding
  #4 + the zeroing fix above).
- **Add the mount-trigger to `AiCloudSection` ONLY.** `CloudProviderSetup` already loads on open (`:126–130`); a second
  trigger double-fires. Gate the `AiCloudSection` mount-trigger on warm-cache-miss AND no in-flight
  `connectionCheckTimer`, and ensure it doesn't race `handleCloudProviderChange`'s check.
- **New network behavior in dev/E2E:** today nothing fires on mount. For no-key providers (`custom`/`ollama`/`lm-studio`)
  `hasCheckableConfig` is true whenever the preset base URL is set, so a mount-trigger WOULD fire a real request in
  dev/E2E. Add an explicit dev/E2E suppression for the mount-trigger (match how analytics/indexer suppress in dev), or
  scope the auto-check to providers that already had a successful check / a stored key. `hasCheckableConfig` alone is
  insufficient.
- API keys stay in the secret store; the cache must never persist or log a key (redaction-adjacent; see `redact/`).

**Test plan:** `ai-model-cache` unit tests (warm/cold/invalidate); a11y tests for both converted sections still pass;
manual via MCP — open Settings → AI (cloud) with a configured key and confirm the list is present on open and instant on
reopen; same in onboarding. `pnpm check --include-slow` for the AI a11y/e2e surface.

**DONE:** One shared combobox + one shared cache; list populates on open and survives reopen; provider select is
`ui/Select`; duplication gone; checks green.

---

## Milestone M2 — Viewer + transfer selects (parallel sub-worktree `dropdowns-views`)

**Scope (files): `routes/viewer/EncodingPicker.svelte`, `routes/viewer/ViewModePicker.svelte`,
`lib/file-operations/transfer/TransferDialog.svelte`.**

**Intentions:** Convert each native `<select>` to `ui/Select`. `EncodingPicker` keeps its Unicode/Western grouping (use
`ui/Select`'s group support; the "(Detected)" suffix is just label text). **`TransferDialog`'s space info is NOT a
per-item description** — it's a separate sibling `<span class="space-info">` outside the select, driven by a `$effect`
on `selectedVolumeId` that refetches `getVolumeSpace`. Keep that span exactly as-is; `ui/Select` just needs a
`value`+`onChange` the consumer can two-way wire (replacing `bind:value`). `ViewModePicker` is a near-trivial
single-option placeholder — convert for consistency, keep it disabled.

**Landmines:** The viewer window renders hostile content and has a restricted capability set — `ui/Select` must use only
already-permitted APIs (pure DOM/Ark, no portal-to-body, so fine, but verify the viewer route still mounts and the popup
isn't clipped by the toolbar `overflow` or swallowed by `data-tauri-drag-region`). `TransferDialog`: keep the
`$effect`-driven `getVolumeSpace` refetch wired to the new `onChange`; don't regress the space-info span.

**Test plan:** existing viewer + transfer dialog tests pass; manual: open the viewer encoding picker (grouped options
render, keyboard nav works) and a copy/move dialog (volume switch refetches + shows free space). `pnpm check` green.

**DONE:** Three native selects gone, replaced by `ui/Select`; grouping verified; TransferDialog space-info intact; checks
green.

---

## Milestone M3 — Debug panel selects (parallel sub-worktree `dropdowns-debug`)

**Scope (files): `routes/debug/DebugSmbDiagnosticsPanel.svelte` (volume + interval selects),
`routes/debug/DebugErrorPreviewPanel.svelte` (4 provider selects).**

**Intentions:** Convert all debug-panel native `<select>`s to `ui/Select`. These are dev-only, low-risk, and make good
soak coverage for the new primitive. **M3 must NOT edit `routes/debug/+page.svelte`** (M0 owns it for catalog wiring) —
only the two panel files.

**Landmines:** Debug panels are gated to the debug window; just ensure they still render and the selects still drive
their state. No a11y test required for debug-only surfaces, but don't break the build.

**Test plan:** debug window mounts; each select drives its state. `pnpm check` green.

**DONE:** All debug-panel selects use `ui/Select`; checks green.

---

## Reconciliation (orchestrator)

Merge `dropdowns-ai`, `dropdowns-views`, `dropdowns-debug` into the hub branch. File sets are disjoint by design:
M0 owns `lib/ui/Select.svelte` + `Combobox.svelte` + `SettingSelect.svelte` + both catalog files
(`routes/dev/components/+page.svelte` AND `routes/debug/+page.svelte`) + the new catalog sections; M1 owns all
`AiCloudSection`/`CloudProviderSetup`/`ai-model-cache.ts`; M2 owns viewer + transfer; M3 owns ONLY the two debug
*panel* files (not `routes/debug/+page.svelte`). Expect clean merges. Run `pnpm check --include-slow` on the reconciled
hub. Then hand to David for the macOS-y look review (he explicitly wants to eyeball the visual result) BEFORE any FF to
local `main`.

## Invariants register (conformance review checks these at phase end)

1. Exactly two new value-picker primitives in `lib/ui/` (`Select`, `Combobox`); both Ark-based, both cataloged, both
   a11y-tested.
2. `SettingSelect` behavior is byte-identical for its three consumers; it now delegates rendering to `ui/Select`.
3. No remaining native `<select>` in scope (AI provider, viewer ×2, transfer, debug ×6) — all on `ui/Select`. Out of
   scope and untouched: CommandPalette, all category-D popovers/menus, VolumeBreadcrumb, swatch picker.
4. Exactly one model combobox implementation, used by both AI settings and onboarding; zero duplicate listbox code.
5. The AI model list loads on `AiCloudSection` open (warm cache → instant; cold + checkable → one debounced fetch,
   suppressed in dev/E2E) and survives close+reopen via the shared session cache. The combobox never morphs shape and
   never blanks: its text is `inputValue`-driven (saved/typed model), independent of collection membership; a custom
   model name persists.
6. The cache fingerprint is a SHA-256 digest; the raw API key (and the digest input) is never stored or logged. No
   second mount-trigger added to onboarding (it already loads on open).
7. One standardized chevron (Lucide `chevron-down`) across both primitives; no tiny font-glyph arrows remain in scope.
8. No new Tauri command / IPC / `bindings.ts` change. No allowlist bumped without consent.
9. All user-facing strings follow the style guide; dark/light + reduced-motion + system-text-size respected.
