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
| `StepAi.svelte`              | Step 2: AI provider picker. M2 ships a stub; M3 lands the provider list + per-provider setup + FDA-outcome banner.                 |
| `StepOptional.svelte`        | Step 3 (optional): networking, indexing, updates, MTP toggles. M2 ships a stub; M4 wires the toggles.                              |
| `onboarding-state.svelte.ts` | Wizard state machine: step cursor, step-1 variant, step-1 footer mode, step-2 banner mode, `openWizard()` / `resumeStepFor()` etc. |

## Status

M2-shipping. Step 1 (FDA) is real and is the single production FDA path — the legacy `FullDiskAccessPrompt.svelte` modal
has been removed. Steps 2 and 3 are stubs (M3 / M4 land their content). `CMDR_FORCE_ONBOARDING=1` forces the wizard
regardless of persisted state for dev / E2E iteration.

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
