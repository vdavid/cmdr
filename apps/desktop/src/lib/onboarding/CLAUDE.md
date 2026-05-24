# Onboarding module

Owns first-launch consent: Full Disk Access (macOS only), AI provider, and a small optional-settings step. Renders the
`OnboardingWizard` — a soft-sheet that covers ~90% of the viewport over the running app — as the single first-launch
path.

## Key files

| File                         | Purpose                                                                                                                            |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `OnboardingWizard.svelte`    | Soft-sheet wizard shell: backdrop, step-dot indicator, Back button, primary footer button, focus trap, Escape-swallow.             |
| `OnboardingStepShell.svelte` | Per-step inner frame (padding, scroll container). Steps render their body inside.                                                  |
| `StepFda.svelte`             | Step 1 (macOS only): Full Disk Access. Three variants — first-ask, revoked, already-granted.                                       |
| `StepAi.svelte`              | Step 2: AI provider picker. Three FDA-outcome banners, three radio choices, dual-button footer (Start vs Continue).                |
| `CloudProviderPicker.svelte` | Step 2 left column: scrollable listbox of all 15 cloud providers. Arrow / Home / End / type-to-jump keyboard nav.                  |
| `CloudProviderSetup.svelte`  | Step 2 right column: per-provider numbered tutorial with API-key persist + auto-check + model combobox.                            |
| `StepOptional.svelte`        | Step 3 (optional): networking, indexing, updates, MTP toggles bound to existing registry settings.                                 |
| `onboarding-state.svelte.ts` | Wizard state machine: step cursor, step-1 variant, step-1 footer mode, step-2 banner mode, `openWizard()` / `resumeStepFor()` etc. |

## Status

M5-shipping. All three steps are real, and the wizard is re-openable from the macOS app menu and the command palette
(both platforms). Existing users on upgrade see a one-time `info` toast pointing at the new menu item. The legacy
`FullDiskAccessPrompt.svelte` modal is gone — the wizard is the single first-launch path on macOS.
`CMDR_FORCE_ONBOARDING=1` forces the wizard regardless of persisted state for dev / E2E iteration.

## Re-entry points

Three surfaces open the wizard after first launch:

| Surface         | macOS                                             | Linux                           | Internal command id   |
| --------------- | ------------------------------------------------- | ------------------------------- | --------------------- |
| Menu item       | `Cmdr > Onboarding…` (under "Check for updates…") | (none — palette-only by design) | `cmdr.openOnboarding` |
| Command palette | "Onboarding…" (both platforms)                    | "Onboarding…"                   | `cmdr.openOnboarding` |
| MCP             | `dialog` tool with `type: "onboarding"`           | same                            | (none — direct event) |

All three surfaces route through the same handler (`+page.svelte::openOnboardingFromMenuOrPalette`), which opens the
wizard at the first reachable step (step 1 on macOS, step 2 on Linux) regardless of `isOnboarded`. The plan's round-3 #1
codifies "menu re-entry always opens at step 1"; `openWizard()` enforces this by checking the `source` argument.

**Why no Linux menu entry**: the wizard's design language is macOS-centric (frosted backdrop matches macOS sheets,
"Restart Cmdr" copy assumes the Quit & Reopen flow, FDA-relevance). Adding a redundant menu entry next to the palette
command would clutter Linux's GTK menu bar for marginal benefit. Palette discovery is good enough; the upgrade-nudge
toast names it on first launch after upgrade.

### Upgrade nudge

Existing users (anyone with `isOnboarded === true` and `onboarding.upgradeNudgeShown === false`) see one `info` toast on
the first launch after they update past the wizard revamp:

- macOS: "We've added new onboarding options. Open Cmdr > Onboarding… to review them."
- Linux: "We've added new onboarding options. Open the command palette and run Onboarding… to review them."

The toast fires from `resolveOnboardingMount()`'s `showApp = true` branches (so it only runs when the wizard is NOT
mounting; no need for an extra `onboardingShowing` check). It writes `onboarding.upgradeNudgeShown = true` synchronously
after firing, so it never appears again on the same machine. The hidden setting was added in M1; M5 wires the firing.

The toast is suppressed under `getAppMode() === 'e2e'` so it doesn't leak into Playwright's first-spec-of-the-run state
(each E2E shard gets its own fresh data dir, so the nudge would otherwise fire once per shard launch and trip the
fixture safety net). The firing logic itself stays unit-tested in Vitest; the E2E suppression is a target-mode gate, not
a behaviour change.

### MCP

The MCP `dialog` tool's open path accepts `type: "onboarding"`. It emits the standard `execute-command` Tauri event with
`commandId: "cmdr.openOnboarding"` — the same path the menu and palette use — and acks on
`SoftDialogAppeared("onboarding")` within the standard 1500 ms budget. The wizard calls
`notifyDialogOpened('onboarding')` on mount, so `SoftDialogTracker` reflects it.

No dedicated `open_onboarding` MCP command was needed: the existing generic `dialog` tool's open switch is hard-coded
per dialog type, but adding one case is cheaper than a new tool and keeps the agent API consistent with
`dialog open about` / `dialog open settings`. Close / focus actions aren't wired for `onboarding` (the wizard has no
rivals to focus above, and closing requires committing to a step per round-3 #9 — the design forbids
dismiss-without-decision).

## Step 1 (Full Disk Access)

macOS only — Linux skips the step entirely (the resume rule lands Linux users on step 2).

The step has three opening copy variants, picked by `step1VariantFor()` in `onboarding-state.svelte.ts`:

- **first-ask** (`fullDiskAccessChoice === 'notAskedYet'`): welcome + pros/cons + how-to + Allow / Deny.
- **revoked** (`'allow' && !hasFda && isOnboarded`): "Cmdr previously had FDA but you revoked it…" framing.
- **already-granted** (`hasFda === true`, menu / palette re-entry): single line + a Next footer button.

The buttons inside the step body (`Open System Settings`, `Deny`) own the Allow / Deny flow; the wizard's footer primary
button is hidden in `decide` mode and reads `Restart Cmdr` in `restart` mode (set after Allow). The `already-granted`
variant has no in-body buttons; the wizard's footer renders a single `Next`.

### Allow path requires a restart

Per `docs/specs/onboarding-revamp-plan.md` § "FDA gate clear-on-Allow": after the user clicks Allow, the wizard does NOT
advance to step 2 in-session. The footer's primary button flips to "Restart Cmdr" (calls `relaunch()` from
`@tauri-apps/plugin-process`). Reason: the FDA gate (`fda_gate::FDA_PENDING`) is set once at boot from
`(fda_choice, os_fda_granted)`; clearing it at runtime would race the TCC popups the gate was built to suppress (we hit
5–10 stacked popups once already). The user's choice persists, and the resume rule lands them on step 2 immediately
after relaunch.

The Allow / Deny buttons stay live in restart mode so the user can change their mind to Deny without restarting (Deny
advances normally).

### Deny path

`StepFda.svelte::handleDeny`:

1. `saveSettings({ fullDiskAccessChoice: 'deny' })`.
2. `startIndexingAfterFdaDecision()` — clears the runtime FDA gate, starts the MTP watcher, kicks off the indexer. The
   scan walks `~/Downloads`, `~/Documents`, `~/Desktop`, etc., firing one TCC popup per folder. Those are the per-folder
   prompts the user opted into by denying FDA. Folders the user denies stay unindexed.
3. `setStepTwoBanner('denied')` + advance to step 2.

## Step 2 (AI provider)

Three pieces stacked top to bottom:

1. **FDA-outcome banner** — on step-2 entry, `StepAi.svelte` fires a fresh `checkFullDiskAccess()` + reads
   `fullDiskAccessChoice` and writes one of three modes via `setStepTwoBanner()`:
   - `granted` ("Thanks for granting full disk access!")
   - `denied` ("You chose not to enable full disk access.")
   - `stuck` ("Cmdr doesn't seem to have full disk access yet" — surfaces a deep link to System Settings) Linux
     short-circuits with `linux` (no banner; the step opens with the Welcome line instead).
2. **Comparison table** — verbatim from David's spec, "with AI vs without" for Search, Mass-rename, Select.
3. **Three radio choices** — cloud / local / no AI. Pre-selected from the persisted `ai.provider` so a crash-then-resume
   user lands on their previous pick. Picking cloud reveals `CloudProviderPicker.svelte` (left) and
   `CloudProviderSetup.svelte` (right). Picking local kicks off `startAiDownload()` in the background; switching away
   cancels (HTTP-Range resume picks up on switch-back). Intel Macs see the local radio disabled with a tooltip ("Local
   LLM requires Apple Silicon. Cloud works on Intel.") driven by `getAiRuntimeStatus().localAiSupported`.

### Dual-button footer

Step 2 owns its own footer via `setFooterOverride([...])` (the wizard's right slot supports an array of buttons):

- **Start using Cmdr!** (secondary): persists + `pushConfigToBackend()`, then bumps the wizard's `finishRequestTick` so
  the wizard fires `onComplete()` (skipping step 3 entirely).
- **One more optional setup step** (primary, accent-colored): persists + `pushConfigToBackend()`, then `nextStep()` to
  step 3. The primary color is intentional, to nudge users toward the optional setup without forcing them.

Both buttons stay enabled regardless of API-key validity per the **no-key-blocks-advance** rule: the auto-check status
in the right column is feedback enough; forcing valid key entry as a precondition would fight users who want to grab the
key later. The user can re-enter via `Cmdr > Onboarding…` or fix it in Settings; first AI use surfaces the standard
`NotConfigured` error path.

### Connection-check pipeline

`CloudProviderSetup.svelte` mirrors `lib/settings/sections/AiCloudSection.svelte`'s pipeline rather than forking it: 300
ms debounce on API-key persist, 1 s debounce on `checkAiConnection(baseUrl, apiKey)`. On `connected`, the right column
reveals the model combobox populated from `/models` and the API-key step gets a green check. The wizard never disables
advance based on connection status; the auto-check is purely informational.

### `pushConfigToBackend()` belt-and-braces

The `settings-applier.ts` listener wired in M1 also calls `pushConfigToBackend()` on any `ai.provider` /
`ai.cloudProvider` / `ai.cloudProviderConfigs` change, so the wizard's explicit `await` is redundant in the steady
state. The reason it's there: the listener fires per-setting-change, so if the user flips three settings in one tick we
get three async invocations racing the wizard's `onComplete()`. The explicit `await pushConfigToBackend()` in
`StepAi.persist()` orders the backend reconfigure before the user lands in the app deterministically.

## Step 3 (optional setup)

Four toggle blocks, each bound to an existing registry setting via `<SettingSwitch>`. Defaults stay ON; the step is
about letting the user turn things OFF with full context, not about asking for opt-in.

| Toggle            | Setting ID                  | Live-apply wiring                                                                           |
| ----------------- | --------------------------- | ------------------------------------------------------------------------------------------- |
| Networking        | `network.enabled`           | `passthroughBackendHandlers` → `setNetworkEnabled` (pre-existing)                           |
| Drive indexing    | `indexing.enabled`          | `passthroughBackendHandlers` → `setIndexingEnabled` (pre-existing)                          |
| Automatic updates | `updates.autoCheck`         | `passthroughBackendHandlers` → `applyAutoCheckEnabled` in `updater.svelte.ts` (added in M4) |
| MTP               | `fileOperations.mtpEnabled` | `passthroughBackendHandlers` → `setMtpEnabled` (pre-existing)                               |

Because `<SettingSwitch>` writes via `setSetting()` on every flip, the toggles take effect the moment the user clicks
them — the wizard doesn't need its own persist queue. The footer's single primary button (`Start using Cmdr`, registered
via `setFooterOverride()`) just bumps `finishRequestTick`; the wizard shell's `onComplete` callback then runs
`notifyOnboardingComplete()` (which flips `isOnboarded: true`) and closes the sheet.

`updates.autoCheck` live-apply was the M4 net-new wiring. Before M4 the setting existed in the registry and the UI but
no listener watched it, so flipping it required an app restart. M4 added `applyAutoCheckEnabled(enabled)` to
`updates/updater.svelte.ts` (lifts the poll-loop interval handle to module scope, starts/stops it in place, fires one
immediate `checkForUpdates()` on re-enable so users don't wait the full cadence) plus an entry in
`settings-applier.ts`'s `passthroughBackendHandlers` table so the toggle works from anywhere (wizard, Settings UI, MCP).

## Resume rule

`onboarding-state.svelte.ts::resumeStepFor(ctx)` picks the right step from the persisted flags + an FDA probe:

| macOS state                              | Resume step | Step 1 variant | Step 2 banner |
| ---------------------------------------- | ----------- | -------------- | ------------- |
| `notAskedYet`                            | 1           | first-ask      | (stuck)       |
| `allow` && `!hasFda` && `isOnboarded`    | 1           | revoked        | (stuck)       |
| `allow` && `hasFda` (and `!isOnboarded`) | 2           | (n/a)          | granted       |
| `allow` && `!hasFda` && `!isOnboarded`   | 2           | (n/a)          | stuck         |
| `deny`                                   | 2           | (n/a)          | denied        |

Linux always resumes at step 2 (no FDA gate).

## FDA gate

Two things stay gated on the FDA decision at app launch:

1. **Drive indexer** (recursive scan from `/` would touch iCloud, Photos, ...).
2. **Path-based icon fetches** in `volumes::list_locations` (NSWorkspace.iconForFile on `/Applications`, `~/Desktop`,
   etc. cascades into adjacent TCC services).

Both gates use the same predicate via `crate::fda_gate::is_fda_pending(fda_choice, os_fda_granted)`.

After the user decides:

- **Deny**: `startIndexingAfterFdaDecision()` clears the runtime gate, starts the MTP hotplug watcher, and starts the
  indexer. As the scan walks protected paths, macOS fires one TCC popup per folder.
- **Allow**: the user grants FDA in System Settings, then clicks `Restart Cmdr`. On next launch the OS check returns
  true, the gate is open at boot, and both the indexer and icon fetches run normally with no popups.

The Tauri command is idempotent. See `src-tauri/src/fda_gate.rs`, `src-tauri/src/volumes/CLAUDE.md` § "FDA gate", and
`src-tauri/src/indexing/CLAUDE.md` § "Defer indexer auto-start".

## Mount + onboarding flag

`routes/(main)/+page.svelte` decides whether to mount the wizard:

- `CMDR_FORCE_ONBOARDING=1` → mount wizard.
- `hasFda && isOnboarded` → no wizard; mirror `fullDiskAccessChoice` to `'allow'` if needed.
- `hasFda && !isOnboarded` → no wizard; mirror setting + call `notifyOnboardingComplete()` (covers pre-wizard users who
  already granted FDA).
- `deny && isOnboarded` → no wizard (user denied and finished onboarding).
- Anything else → mount wizard.

The `isOnboarded` boolean lives in `$lib/settings-store.ts`. It flips to `true` on full wizard completion via
`notifyOnboardingComplete()` (from `$lib/updates/updater.svelte`), so the auto-update "restart to apply" toast doesn't
fire during first-launch onboarding.

While the wizard is up, `+page.svelte` also calls `setOnboardingShowing(true)` so the updater suppresses the deferred
toast; `handleWizardComplete` flips it back. See `$lib/updates/CLAUDE.md` § "Onboarding gating".

## Testing

Two env vars (mirror `CMDR_MOCK_LICENSE`):

- `CMDR_FORCE_ONBOARDING=1` (read by `is_force_onboarding()` Tauri command in the backend): opens the wizard regardless
  of persisted state. Useful for design iteration without touching settings.
- `CMDR_MOCK_FDA=granted|denied|notgranted` (read in `permissions.rs::check_full_disk_access`): overrides the TCC probe
  so all banner branches can be tested without ever opening real System Settings. `granted` → `true`; `denied` /
  `notgranted` → `false`. The wizard distinguishes them via the persisted setting + a fresh probe on step-2 entry (M3).

Run with both: `CMDR_FORCE_ONBOARDING=1 CMDR_MOCK_FDA=notgranted pnpm dev`.

## Key decisions

**Decision**: Three-state setting (`notAskedYet` / `allow` / `deny`) instead of a boolean. **Why**: The app needs to
distinguish "never asked" (show first-ask), "granted but later revoked" (show revoked copy), and "user explicitly
declined" (don't re-prompt once onboarded). A boolean would conflate "not asked" with "denied".

**Decision**: No Escape handler on the wizard. **Why**: The wizard owns first-launch consent; dismissing without
choosing leaves the app with no recorded preference. The user must commit to Allow / Deny / Next on each step.

**Decision**: Allow requires a restart before advancing past step 1. **Why**: The FDA gate is set once at boot; clearing
it at runtime races background threads that resolve icons / scan paths into the TCC popups the gate suppresses. We hit
5–10 stacked popups once already — the restart costs the user one click and keeps the gate's invariant intact. See plan
§ "FDA gate clear-on-Allow".

**Decision**: Step 1 footer button hidden in `decide` mode (body owns Allow / Deny). **Why**: The Allow / Deny choice is
the meat of step 1; placing the buttons inside the body groups them with the explanatory copy they belong to. The
wizard's footer remains consistent for the other steps (Back + Next / Finish / Restart Cmdr).

## Key gotchas

- **TCC's registration hook fires on `open()`, not `opendir()`.** Without a `read()` attempt on a protected file, Cmdr
  never enters the Full Disk Access list. `StepFda` re-runs `checkFullDiskAccess()` right before `openPrivacySettings()`
  so the registration is fresh when the Settings pane loads. See `permissions.rs` for the per-file probe list.
- **Deep-link host changed in Ventura.** macOS 13+ uses `com.apple.settings.PrivacySecurity.extension`; older macOS uses
  `com.apple.preference.security`. `openPrivacySettings()` picks via `get_macos_major_version`. The same version informs
  the modal copy: macOS 12 and older append new FDA entries at the end of the list (instead of alphabetical).
- **macOS 26 (Tahoe) FDA auto-add is broken.** Even with a notarized Developer ID build at `/Applications/Cmdr.app`, the
  kernel / sandbox can short-circuit `read()` denials on TCC-protected paths without consulting `tccd`, meaning Cmdr may
  not enter the FDA list automatically. The "+" button fallback (documented in step 1's `step-tip`) is the user-side
  workaround. References: [Apple Developer Forums #809549](https://developer.apple.com/forums/thread/809549),
  [Backrest issue #986](https://github.com/garethgeorge/backrest/issues/986),
  [Apple Developer Forums #757768](https://developer.apple.com/forums/thread/757768).
- **The wizard renders the app behind it.** First launch lands on `~`, so what peeks through the backdrop is friendly.
  No "white screen until wizard done" code path.
- **Linux skips step 1.** `isAtFirstStep()` returns `true` on step 2 for Linux so the Back button disables there. Step 1
  returns `null` on Linux as a safety net.

## Dependencies

- `$lib/tauri-commands`: `checkFullDiskAccess`, `getMacosMajorVersion`, `openPrivacySettings`,
  `startIndexingAfterFdaDecision`, `openExternalUrl`, `notifyDialogOpened`, `notifyDialogClosed`, `isForceOnboarding`
- `$lib/settings-store`: `saveSettings`, `loadSettings`
- `$lib/shortcuts/key-capture`: `isMacOS`
- `$lib/system-strings.svelte`: localized system pane names
- `$lib/ui`: `Button`, `LinkButton`
- `@tauri-apps/plugin-process`: `relaunch` (Allow-path footer button)
