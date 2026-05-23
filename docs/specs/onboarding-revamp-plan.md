# Onboarding revamp — implementation plan

Companion to [`onboarding-revamp-context.md`](onboarding-revamp-context.md). The context bundle is the frozen record of every decision and David's verbatim copy; this file is the ordered execution plan that turns those decisions into shipping code. Re-read the context bundle whenever a question feels like it might be re-litigating a decision — it almost certainly is.

Canonical copy strings live in the context bundle. This plan references them; it never restates them.

## Goal

Replace the single Full Disk Access (FDA) modal with a soft-sheet onboarding wizard that occupies ~90% of the viewport over the running app. Two mandatory steps (FDA decision, AI provider) plus one optional step (Networking, Drive indexing, Updates, MTP). The wizard becomes the single first-launch path for FDA and AI consent, replacing the existing FDA modal + post-FDA AI offer toast, and is re-openable from a new `Cmdr > Onboarding…` menu item and a command-palette command. Existing-on-upgrade users skip silently and get a one-time `info` toast pointing at the new menu item.

## Out of scope (explicit)

These came up in discussion and are deliberately deferred:

1. **Local-model auto-restart** when the wizard re-opens mid-download — the existing toast covers progress and HTTP-Range resume; no in-wizard progress UI.
2. **Subscription auth** ("Bring a Claude Code / ChatGPT subscription") — API key only on step 2. No "Coming soon" affordance.
3. **Indexing-cache cleanup** when step 3 turns indexing off — only flips `indexing.enabled`; the existing settings-applier handles runtime stop.
4. **Linux polish** beyond skipping step 1 — Linux gets the AI + optional steps via the same wizard. Linux users see "Welcome to Cmdr!" at the top of step 2 (since step 1 is skipped); the surrounding copy is the same FDA-outcome banner shape but with the FDA references stripped. Accept the slight awkwardness — Linux is the secondary platform per the context bundle.
5. **The pre-existing failing test** `File viewer selection and copy › drag within viewport selects the dragged range` — owned by another agent.
6. **Cmdr's marketing-side "what's new" surface for the wizard revamp** — separate work.

## Architecture overview

### New files

| File | Purpose |
| --- | --- |
| `apps/desktop/src/lib/onboarding/OnboardingWizard.svelte` | Top-level soft-sheet container. Owns the step machine, backdrop, step-dot indicator, Back button, keyboard contract (Tab, Enter, Escape disabled). |
| `apps/desktop/src/lib/onboarding/OnboardingStepShell.svelte` | Per-step inner frame: padding, scroll container, primary button row. Steps render their bodies inside. Centralises sheet typography so the per-step files stay copy-focused. |
| `apps/desktop/src/lib/onboarding/StepFda.svelte` | Step 1 body. Wraps the preserved FDA content from `FullDiskAccessPrompt.svelte` (see § "Preserved logic"). |
| `apps/desktop/src/lib/onboarding/StepAi.svelte` | Step 2 body. Renders the FDA-outcome banner + the AI provider radio + the cloud provider two-column picker. |
| `apps/desktop/src/lib/onboarding/StepOptional.svelte` | Step 3 body. Four toggle blocks bound to existing registry settings. |
| `apps/desktop/src/lib/onboarding/CloudProviderPicker.svelte` | The left-column scrollable provider list (arrows + type-to-jump) used inside `StepAi.svelte`. Mounts the keyboard handler and exposes `value`/`onChange`. |
| `apps/desktop/src/lib/onboarding/CloudProviderSetup.svelte` | The right-column "numbered tutorial steps" view: per-provider instructions, API-key input, auto-check, model picker. Reuses the connection-check pipeline from `AiCloudSection.svelte`. |
| `apps/desktop/src/lib/onboarding/onboarding-state.svelte.ts` | The wizard state machine. Exports `openWizard()` / `closeWizard()` / a `$state` shell consumed by `+page.svelte` and the menu / palette handlers. Implements the step-resume rule (start at first not-yet-decided step). |
| `apps/desktop/src/lib/onboarding/StepFda.a11y.test.ts`, `StepAi.a11y.test.ts`, `StepOptional.a11y.test.ts`, `OnboardingWizard.a11y.test.ts` | Tier-3 axe-core specs (one per state per file as appropriate). |
| `apps/desktop/test/e2e-playwright/onboarding-wizard.spec.ts` | Tier-2 happy path + edge branches. |
| `apps/desktop/test/e2e-playwright/onboarding-real-openai.spec.ts` | Gated real-OpenAI smoke (skips when key absent in keychain). |

### Modified files

| File | Change |
| --- | --- |
| `apps/desktop/src/lib/ui/dialog-registry.ts` | Add `{ id: 'onboarding', description: 'First-launch (and re-openable) setup wizard' }`. |
| `apps/desktop/src/routes/(main)/+page.svelte` | Replace `showFdaPrompt`/`handleFdaComplete`/`FullDiskAccessPrompt` with `showOnboarding`/`onWizardComplete`/`OnboardingWizard`. Add `CMDR_FORCE_ONBOARDING` env-var gate. Stop calling `notifyAiOnboardingComplete()`. Keep `notifyOnboardingComplete()` (still used by the updater toast gate; the wizard's `onComplete` calls it). |
| `apps/desktop/src/lib/ai/ai-state.svelte.ts` | Remove `pendingOffer`, `onboarded`, `notifyAiOnboardingComplete`, `handleDismiss`, `handleOptOut`, `handleDownload`, and the gating logic in `updateNotificationFromStatus`. Drop `'offer'` from the `AiNotificationState` union. Update `resetForTesting()` to match. |
| `apps/desktop/src/lib/ai/AiToastContent.svelte` | Delete the `offer` switch case + its actions + its styles that only apply to it. Drop the `handleDismiss`, `handleOptOut`, `handleDownload` imports that are no longer needed (downloading-onwards states keep their existing handlers). |
| `apps/desktop/src-tauri/src/commands/ai.rs` (and binding) | **Delete** the `dismiss_ai_offer` and `opt_out_ai` Tauri commands in M1 — their only frontend callers (`handleDismiss`, `handleOptOut`) get removed in the same milestone. Dead IPC surface rots; per `apps/desktop/src/lib/ipc/CLAUDE.md` regenerate bindings. |
| `apps/desktop/src/lib/settings/settings-applier.ts` | M1: add three entries to the `passthroughBackendHandlers` lookup table for `ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs`, all calling `pushConfigToBackend()` (see § "AI configure on step-2 persist"). M4: add a `updates.autoCheck` entry (currently missing) and verify the existing `network.enabled` / `indexing.enabled` / `fileOperations.mtpEnabled` entries actually fire (see M4 § "Live-apply audit"). |
| `apps/desktop/src/lib/settings/ai-config.ts` | **New file** (M1). Receives the relocated `pushConfigToBackend()` + `migrateApiKeysFromSettings()` + `describeSecretError`-using paths from `apps/desktop/src/lib/settings/sections/ai-settings-utils.ts`. The function isn't UI-component-coupled; `sections/` is for UI subcomponents. Same exports, same call sites — only the import path changes. |
| `apps/desktop/src-tauri/src/capabilities/default.json` | Allow the new `is_force_onboarding` command (M1) and (if not already permitted) the existing `start_indexing_after_fda_decision` + `relaunch`. The Allow-requires-restart path uses `relaunch` from `@tauri-apps/plugin-process` (already a dependency for the updater) — verify the capability is present, add if not. **No** new `clear_fda_gate` IPC; restart is the gate-clear mechanism (see § "FDA gate clear-on-Allow"). |
| `apps/desktop/src/lib/ai/CLAUDE.md` | Strip the "Onboarding gate suppresses the offer toast" section. Add a one-paragraph "The wizard owns AI consent" pointer. Remove the `pendingOffer` gotcha. |
| `apps/desktop/src/lib/onboarding/CLAUDE.md` | Rewrite around the wizard. Keep the FDA gotchas (TCC `open()` trigger, Ventura deep-link host, macOS 26 Tahoe regression). Add "Step persistence and resume" section. |
| `apps/desktop/src/lib/settings/settings-registry.ts` | Change `ai.provider` default from `'local'` to `'off'`. Add hidden `onboarding.upgradeNudgeShown: boolean` (default `false`) for the one-time legacy-user toast. |
| `apps/desktop/src/lib/settings-store.ts` | No additions: the wizard's partial-completion recovery rides on existing `fullDiskAccessChoice` / `isOnboarded` flags. (See § "Persistence schema".) |
| `apps/desktop/src/lib/commands/command-registry.ts` and `routes/(main)/command-dispatch.ts` | Add `cmdr.openOnboarding` command. |
| `apps/desktop/src-tauri/src/menu/macos.rs` + `menu/mod.rs` | Add `OPEN_ONBOARDING_ID` menu item under "Check for updates…" in the Cmdr app menu. macOS only — Linux re-entry is palette-only by design (see M5 § implementation step 3). |
| `apps/desktop/src-tauri/src/permissions.rs::check_full_disk_access` | Add `CMDR_MOCK_FDA=granted|denied|notgranted` short-circuit at the top, mirroring `CMDR_MOCK_LICENSE`. |
| `apps/desktop/src/lib/onboarding/FullDiskAccessPrompt.svelte` | **Delete** at the end of M2, after `StepFda.svelte` is shipping. |

### Data flow

```
+page.svelte onMount
  ├─ env CMDR_FORCE_ONBOARDING set? → openWizard({ source: 'force' })
  ├─ checkFullDiskAccess() === true
  │   ├─ ensure fullDiskAccessChoice='allow'
  │   ├─ isOnboarded === false? → openWizard({ source: 'first-launch' }) (resumes at step 2)
  │   └─ isOnboarded === true  → showApp = true; maybe-fire upgrade-nudge toast
  └─ checkFullDiskAccess() === false
      ├─ fullDiskAccessChoice === 'notAskedYet' → openWizard (resumes at step 1, first-ask)
      ├─ fullDiskAccessChoice === 'allow' && isOnboarded
      │       → openWizard (resumes at step 1 with the "wasRevoked" copy — post-onboarding revocation)
      ├─ fullDiskAccessChoice === 'allow' && !isOnboarded
      │       → openWizard (resumes at step 2 with the "didn't grant" banner — Allow + restart didn't take)
      └─ fullDiskAccessChoice === 'deny'
          ├─ isOnboarded === false? → openWizard (resumes at step 2; "we respect that" banner)
          └─ isOnboarded === true  → showApp = true; maybe-fire upgrade-nudge toast

openWizard() sets showOnboarding=true, sets setOnboardingShowing(true) (renamed from
   setFdaPromptShowing — the gate now spans steps 2 & 3 too; one-line rename in M1).

Wizard runtime
  step 1: writes fullDiskAccessChoice on Allow/Deny click (immediate persist).
          On Allow: REQUIRE A RESTART before advancing (see § "FDA gate clear-on-Allow").
                    User clicks Allow → opens Settings → toggles → returns to wizard. The wizard's
                    primary footer button becomes "Restart Cmdr" (calls relaunch). The user cannot
                    advance to step 2 without restarting.
  step 2: writes ai.provider + (if cloud) ai.cloudProvider + ai.cloudProviderConfigs + secret-store key.
          Then awaits pushConfigToBackend() so the backend's ManagerState is updated mid-session
          (see § "AI configure on step-2 persist" — uses the existing helper, not a new function).
          If local, also calls startAiDownload() in background.
  step 3: writes the four toggles directly via setSetting() (live-apply via M4's applier additions —
          see M4 § "Live-apply audit"). updates.autoCheck IS currently unwired; M4 adds it.
  onComplete: notifyOnboardingComplete() (persists isOnboarded=true), setOnboardingShowing(false),
              showApp = true, showOnboarding = false.
```

### AI configure on step-2 persist (BLOCKER fix — round-1 + round-2)

`apps/desktop/src/routes/(main)/+layout.svelte` calls into `pushConfigToBackend()` once at app start. With the default flipped to `'off'`, the backend hears "off" on launch, and **nothing re-pushes the wizard's choice** to `ManagerState`. Result: local download succeeds but server never starts, cloud config lands in settings/keychain but `ManagerState.cloud_*` stays empty, and the first AI use fails with `NotConfigured`.

**Use the existing helper, don't reinvent it.** `apps/desktop/src/lib/settings/sections/ai-settings-utils.ts:67` already exposes `pushConfigToBackend(): Promise<void>` — zero args, reads `ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` / `ai.localContextSize` fresh from `getSetting(...)`, calls `resolveCloudConfig(...)`, fetches the keychain key via `getAiApiKey(providerId)`, calls `configureAi(provider, contextSize, cloudApiKey, cloudBaseUrl, cloudModel)` (note the REAL `configureAi` signature: 5 args, **no `cloudProvider` arg** — the round-1 plan got this wrong), AND surfaces secret-store errors via a deduped persistent toast (`secretErrorToastId = 'ai-secret-store-error'`). Today's `AiSection.svelte` is its only caller; the wizard + applier become callers two and three.

**Fix:**

1. **M1 — relocate** `pushConfigToBackend()` (plus the related `migrateApiKeysFromSettings` helper next to it, plus the `describeSecretError` dependency) from `apps/desktop/src/lib/settings/sections/ai-settings-utils.ts` to `apps/desktop/src/lib/settings/ai-config.ts`. The function isn't UI-component-coupled; `sections/` is for UI subcomponents. Update existing imports — grep `rg "ai-settings-utils|pushConfigToBackend" apps/desktop/src/` and rewrite paths. Move (don't copy) — the goal is one push path, period.
2. **M1 — applier listener.** In `settings-applier.ts`, add three entries to the existing `passthroughBackendHandlers` lookup table at `settings-applier.ts:152-164` (the same pattern `network.enabled` / `indexing.enabled` / `fileOperations.mtpEnabled` already use — NOT new `if (id === ...) return` cases). All three entries call one shared handler that fires `pushConfigToBackend()`:
   ```ts
   'ai.provider': () => void pushConfigToBackend(),
   'ai.cloudProvider': () => void pushConfigToBackend(),
   'ai.cloudProviderConfigs': () => void pushConfigToBackend(),
   ```
   **Read-fresh semantics (load-bearing):** `pushConfigToBackend()` re-reads every relevant setting fresh from `getSetting(...)` at the moment it runs. **Do NOT pass cached args** into the call. If the applier batches/debounces multiple passthrough fires (check the existing pattern; if it doesn't today, leave it; per-setting fires are fine), the re-read still wins: whichever provider is "current" at the actual IPC moment is the one pushed. This handles the race where a user picks OpenAI → applier schedules → user switches to Anthropic before the fire → the fresh read picks Anthropic, exactly what the user expects. Cached args would push stale OpenAI config.
3. **M3 — belt-and-braces.** The wizard's step-2 "Start using Cmdr!" / "One more optional setup step" handlers call `await pushConfigToBackend()` directly after `setSetting(...)` returns. The applier listener already covers them; the explicit call here guarantees ordering when all three settings change at once (step 2 commits all three in one tick) and resolves the IPC before the wizard advances. Either path alone is sufficient; both together is cheap insurance.

**Per-key semantics handled by re-reading.** A flip from `'off'` → `'cloud'` vs `'off'` → `'local'` vs `'cloud'` → `'local'` all need different backend states. A change to `ai.cloudProvider` requires a different keychain key (different `getAiApiKey(providerId)` arg). A change to `ai.cloudProviderConfigs` may rotate the model. `pushConfigToBackend()` handles all of them correctly because it re-reads `ai.provider` + `ai.cloudProvider` (which determines the keychain key fetched) + `ai.cloudProviderConfigs` fresh on each call. The applier never decides per-key; it always asks. This is why the no-cached-args rule above is load-bearing.

**Keychain leftovers on provider switch.** API keys typed in the wizard are persisted to the OS secret store immediately (so the auto-check pipeline can validate them). Switching the provider radio to `local` or `off`, OR switching from OpenAI to Anthropic inside the cloud branch, does NOT delete previously-typed keys; they remain in the secret store for next-time pre-fill. Same shape as `AiCloudSection.svelte`'s flow today — consistent behaviour across the app. The user clears a key explicitly via the settings UI's "Forget" button (out of scope for the wizard).

**`ai.provider` default-flip migration.** Flipping the registry default from `'local'` to `'off'` only affects fresh installs. Existing users have a non-default stored value already (one of `'off'`, `'cloud'`, `'local'`); the default only applies when no value is stored, or when the stored value is corrupt / missing / fails enum validation. `ai.provider` is a three-value enum that rarely corrupts. **No migration code needed**, no `SCHEMA_VERSION` bump. (Documented here so a future agent doesn't add one defensively.)

### FDA gate clear-on-Allow (MAJOR fix)

`fda_gate::FDA_PENDING` is set once at boot from `(fda_choice, os_fda_granted)`. Deny clears it via the existing `start_indexing_after_fda_decision` Tauri command. Allow today requires a restart (which re-enters `setup()` with a fresh probe and the gate ends up cleared). Under the wizard, if we let the user click Allow → grant in System Settings → return → advance past step 1 → finish without restarting, the gate stays `true` forever in that session: volumes stay icon-less, MTP watcher stays off.

**Decision: option (a) — require restart on Allow before advancing past step 1.** Why:

- Safety: clearing the gate at runtime means racing the same TCC popups the gate was built to suppress. The gate's whole point (per `AGENTS.md` § "Critical rules" → FDA gate) is "5–10 popups stacked on top of the modal" we already saw happen once. Clearing it mid-session re-opens that risk window for any background thread that resolves an icon before the next `volumes-changed`.
- Simplicity: the existing Allow path already restarts; the wizard just needs to surface that as the explicit next action. No new IPC, no new state to keep consistent.
- UX cost is minimal: the user already opened System Settings, toggled a system permission, and came back — a "Restart Cmdr" button is the natural next click. The wizard remembers it's mid-flow via the persisted `fullDiskAccessChoice: 'allow'` + `isOnboarded: false`; the resume rule lands the user on step 2 immediately after restart.

**Wizard behaviour on step 1 Allow path:**
1. User clicks "Open System Settings" → `checkFullDiskAccess()` re-probe → `openPrivacySettings()` → post-action hint appears.
2. The footer primary button changes from "Open System Settings" to "Restart Cmdr". The Back button stays available so the user can change to Deny instead.
3. Clicking "Restart Cmdr" calls `relaunch` from `@tauri-apps/plugin-process` (already a dependency for the updater). The wizard does NOT advance to step 2 in-session. On the next launch the resume rule lands the user on step 2.
4. If `checkFullDiskAccess()` happens to return `true` on the restart hint poll (rare — Tauri doesn't surface a callback, but the wizard could fire one fresh probe on focus return), the "Restart Cmdr" button stays — we still want the gate cleared via the normal `setup()` path.

No new Tauri command needed.

### Step persistence resume — edge cases (MAJOR documented + MAJOR fix)

Per round-3 #2 in the context bundle ("next launch starts at the first not-yet-decided step"), the resume rule maps to existing flags. Two edge cases need explicit handling:

**Edge A (accepted, no new schema):** a user who finished step 2 (picked AI provider) but crashed before step 3 re-sees step 2 next launch. Step 2 pre-fills the previous choice (`ai.provider` + provider config + key) so the user sees their previous selection highlighted; the wizard adds a passive cue ("You picked this last time. Confirm or change below.") when `!isOnboarded && ai.provider !== 'off'`. One Enter to advance. Why no `onboarding.step2Completed` field: it's a second source of truth that drifts the moment we add a step 4, and a new schema field needs a `SCHEMA_VERSION` bump + migration. Re-confirming one step on a rare-crash path is the better tradeoff.

**Edge B (MAJOR fix — separate "stuck" from "revoked"):** a user clicks Allow → never toggles in System Settings → clicks "Restart Cmdr" → relaunches. Their flag state is `fullDiskAccessChoice === 'allow' && !hasFda` — the SAME as a "revoked later" user. Without a third branch, the resume rule would land them on step 1 with the "Cmdr previously had Full Disk Access" copy, which is a lie. `isOnboarded` is the distinguisher: revoked-later users are post-onboarded (`true`); first-time stuck users aren't (`false`).

Resume rule (in `onboarding-state.svelte.ts::resumeStepFor(settings, hasFda)`):

```ts
// macOS step 1 paths:
if (isMacOS() && settings.fullDiskAccessChoice === 'notAskedYet') return 1 // first-ask
if (isMacOS() && settings.fullDiskAccessChoice === 'allow' && !hasFda && settings.isOnboarded) {
  return 1 // revoked-later: step 1 with the existing "wasRevoked" copy
}
// Step 2 paths:
if (isMacOS() && settings.fullDiskAccessChoice === 'allow' && !hasFda && !settings.isOnboarded) {
  return 2 // first-time stuck: lands on step 2 with the "didn't grant" banner copy
}
return 2 // FDA decided (allow + granted, OR deny) OR Linux
```

This maps cleanly to the three step-2 banner copy variants from the context bundle's § "Step 2 — AI":
- `hasFda === true` → "Thanks for granting Full Disk Access!" banner.
- `hasFda === false && fullDiskAccessChoice === 'deny'` → "You chose not to enable Full Disk Access" banner.
- `hasFda === false && fullDiskAccessChoice === 'allow' && !isOnboarded` → "You said you wanted to enable Full Disk Access, but Cmdr doesn't seem to have gotten it" banner (the "find Cmdr in System Settings or use +" branch).

The one-shot `checkFullDiskAccess()` call on step-2 entry (M3) picks the right variant. The resume rule and the banner branch use the same three-way fork.

### Mid-flight regression avoidance (MAJOR fix — M1+M2 layering)

If M1 lands the mount-point swap with stubbed step bodies, first-launch users on `main` between the M1 commit and M2 land in three empty stub steps with no FDA prompt and no persistence — a real regression. **Fix: M1 keeps `FullDiskAccessPrompt.svelte` as the production mount, and the wizard skeleton is only reachable via `CMDR_FORCE_ONBOARDING=1`.** M2 does the mount-point swap once `StepFda.svelte` is shipping (and deletes `FullDiskAccessPrompt.svelte` in the same commit). This preserves the milestone boundary, lets us test the wizard skeleton in isolation, and keeps `main` shippable at every commit.

See M1 and M2 milestone scope below for the exact split.

### Why the wizard is a NEW component, not a `ModalDialog` variant

`ModalDialog` is the right primitive for confirmation-shape dialogs: 480 px wide max, title bar, drag, optional × button, Escape closes. The wizard breaks every one of those constraints — full-bleed 90% sheet, no title bar, never draggable, no Escape, no ×. Bolting variants onto `ModalDialog` would dilute it for every other consumer; the dialog has a tight contract and we want to preserve it. The wizard still **plugs into the same MCP dialog registry** via `dialogId="onboarding"` because the registry is dialog-id-based, not component-based — same surface area, different shell.

### Why AI default flips from `'local'` to `'off'`

Today `ai.provider`'s default of `'local'` is what surfaces the post-FDA "Download AI?" offer toast: backend sees a local-provider machine without a downloaded model, emits `Offer`, frontend renders the toast. With the wizard owning AI consent end-to-end, that offer toast must NOT race the wizard. The cleanest way to kill it is to default the provider to `'off'`: backend never emits `Offer` for off-provider, no toast machinery to gate. The wizard then writes the user's actual choice (`'cloud'` / `'local'` / `'off'`). This also means the `onboarded` flag in `ai-state.svelte.ts` is gone, not just inert.

### Why we render the app behind the wizard

(a) First launch lands on `~`, friendly content. (b) No "white screen until wizard done" code path to maintain. (c) The backdrop blur already separates wizard from app; users perceive the wizard as a sheet over their real app, not a gate. (d) The wizard inherits the macOS-sheet vibe of the recently redesigned Settings (round corners, frosted backdrop, lifted off the canvas), per round-4 #5.

### Why we kill `pendingOffer` / `notifyAiOnboardingComplete`

Their entire purpose was to defer the AI offer toast until after the FDA modal closed. The wizard's step 2 IS the AI consent moment — there's nothing to defer, nothing to surface later. Keeping the gate would leave a dead branch that future agents would misread as load-bearing. Delete it.

### Why we kick off the local-model download immediately on selection

Two reasons. (1) Network egress is free thanks to HTTP-Range resume in `manager.rs::do_download`: if the user switches away from "local" before the file is fully down, we cancel; if they switch back, `startAiDownload()` picks up at the byte boundary it left off at. (2) Latency: by the time the user finishes step 3 and lands in the app, a portion of the ~2 GB model may already be on disk, so the post-wizard `installing` → `ready` transition lands faster. No in-wizard progress UI: the existing top-right toast handles `downloading`/`installing`/`ready`. If it shows in the corner while the user is on step 3, fine; per the context bundle, do NOT add suppression logic for it.

**Download survives wizard close.** `startAiDownload()` runs in a backend-owned tokio task in `manager.rs`. Closing or completing the wizard does not cancel it. The user-explicit cancel paths (the Cancel button on the `downloading` toast, picking a different provider mid-wizard) still cancel correctly. Cross-session resume is automatic via HTTP Range (within ~24 h before the startup cleanup might wipe the partial).

### Why partial-completion recovery uses existing flags, not a step cursor

The state we need to recover is already encoded: `fullDiskAccessChoice` says whether step 1 was decided; `ai.provider` defaulting to `'off'` lets us treat "user has not changed the AI default in the wizard" as step 2 unresolved, but we also need to distinguish "the user explicitly chose `off`" from "never opened the wizard." That distinction comes from `isOnboarded`: `false` = wizard never completed; the resume rule is "open at the first step that isn't decided." Concretely:

- `fullDiskAccessChoice === 'notAskedYet'` → resume step 1
- `fullDiskAccessChoice ∈ { 'allow', 'deny' }` AND `isOnboarded === false` → resume step 2
- `isOnboarded === true` → silent skip (legacy user) + maybe-fire upgrade-nudge toast

No new step-cursor field, no migration, no two-sources-of-truth race. The cost is that step 3 has no "I completed step 2 but stopped before step 3" intermediate state; on resume the user lands on step 2 and walks through it again. That's acceptable — step 2's previous choices are already persisted, so the user sees their picks pre-filled and clicks through quickly.

### Why each milestone ends with a commit + full check suite

David's rule from `AGENTS.md`: `./scripts/check.sh` (default suite) before every commit, `--include-slow` at the end of milestones that touched E2E-relevant code, `oxfmt` always (it's monorepo-wide and ~1 s). No exceptions. This plan inlines that under every milestone's "Checks" section.

## Preserved logic from `FullDiskAccessPrompt.svelte`

Move these into `StepFda.svelte` (don't paraphrase; transplant the calls):

1. `checkFullDiskAccess()` re-probe **before** `openPrivacySettings()` (TCC registration freshness). `apps/desktop/src/lib/onboarding/FullDiskAccessPrompt.svelte:37-48`.
2. `getMacosMajorVersion()` → branch the "find Cmdr in the list" wording (alphabetical on Ventura+, end-of-list on older). `FullDiskAccessPrompt.svelte:30-35` and `:94-98`.
3. The "Tip: click '+' button at the bottom" sub-step copy. `FullDiskAccessPrompt.svelte:99-102`. Lives in step 1 AND the "didn't grant" branch on step 2 (per context bundle).
4. `systemStrings.systemSettings` localized button label. `FullDiskAccessPrompt.svelte:92,108`.
5. `startIndexingAfterFdaDecision()` on Deny. `FullDiskAccessPrompt.svelte:57`.
6. The "Cmdr is source-available" GitHub link in the Con bullet. New per the context bundle; not in the current modal — add it fresh.
7. The post-click "Make sure to restart the app" hint. `FullDiskAccessPrompt.svelte:111-116`. Same anchor, but step 1 keeps the user on step 1 after clicking Allow + opening Settings, with a Next button that advances to step 2.

## Milestones

Six milestones. Each is committable on its own, full check suite green, with the slow lane added at M5 and M6.

---

### M1 — Foundations

#### Scope

Wizard skeleton renders behind `CMDR_FORCE_ONBOARDING=1` only. **The existing `FullDiskAccessPrompt.svelte` stays as the production mount on `main` until M2.** All plumbing for later milestones lands here: dialog registry, env-var mocks, AI default flip, AI toast cleanup, AI applier listener, settings additions, dead-IPC cleanup, sheet design tokens.

**In:**
- Wizard component, step shell, step-dot indicator, Back button, focus trap.
- Dialog registry entry (`'onboarding'`).
- `CMDR_FORCE_ONBOARDING` (Rust-side IPC) + `CMDR_MOCK_FDA` (backend short-circuit).
- `ai.provider` default flip `'local'` → `'off'`.
- `onboarding.upgradeNudgeShown` hidden setting.
- AI toast `'offer'` removal + `pendingOffer` / `onboarded` / `notifyAiOnboardingComplete` removal in `ai-state.svelte.ts`.
- **Delete** `dismiss_ai_offer` and `opt_out_ai` Tauri commands + their callers + their bindings (their only frontend callers — `handleDismiss`, `handleOptOut` — get removed in M1).
- Relocate `pushConfigToBackend()` (+ `migrateApiKeysFromSettings` + `describeSecretError` callers) from `lib/settings/sections/ai-settings-utils.ts` to a new `lib/settings/ai-config.ts`. Update `AiSection.svelte`'s imports.
- AI applier listener for `ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` (BLOCKER fix — see § "AI configure on step-2 persist"). All three entries call the relocated `pushConfigToBackend()`. Without this, the Settings UI itself can't push provider changes to the backend in-session once we flip the default to `'off'`.
- `setFdaPromptShowing` → `setOnboardingShowing` rename. The variable's semantic now spans all three wizard steps. Touches `updates/updater.svelte.ts` (export, internal `fdaPromptShowing` module state, `shouldShowUpdateToast`'s `fdaPromptShowing` field), `updates/updater.test.ts` (~12 references at lines 6, 83, 91, 95, 99, 103, 109, 111, 112, 155, 167, 173, 176, 179, 186, 187, 195, 196 — both the function name AND the renamed `fdaPromptShowing` field on the predicate's args), `updates/CLAUDE.md` § "Onboarding gating" (3 references), `onboarding/CLAUDE.md` (2 references), and `(main)/+page.svelte` (4 call sites).
- Sheet design tokens (`--sheet-width-fraction`, `--sheet-height-fraction`, `--sheet-radius`, `--sheet-backdrop-blur`, `--sheet-backdrop-color`) added to `app.css`. Stylelint allowed-prefix update in `.stylelintrc.mjs:48`: extend FROM `^(color|spacing|font|radius|shadow|transition|z)-.+` TO `^(color|spacing|font|radius|shadow|transition|z|sheet)-.+`. **Critical**: the regex is a full replacement; don't paraphrase to `(color|spacing|font|sheet)-` because that silently drops `radius` / `shadow` / `transition` / `z` and breaks every existing token. **M1 ships the tokens** because the wizard skeleton consumes them from day one; M6 only adds the `docs/design-system.md` subsection.

**Not in:**
- Mount-point swap from `<FullDiskAccessPrompt>` to `<OnboardingWizard>` — M2 owns that (after `StepFda.svelte` is populated). M1's wizard is reachable only via `CMDR_FORCE_ONBOARDING=1`.
- Step content (M2/M3/M4).
- Menu re-entry (M5).
- Upgrade-nudge toast firing (M5).

#### Implementation steps

1. Add `{ id: 'onboarding', description: 'First-launch (and re-openable) setup wizard' }` to `dialog-registry.ts`.
2. Add the new sheet design tokens to `apps/desktop/src/app.css` (see § "Sheet sizing tokens"). Update `.stylelintrc.mjs:48` regex from `^(color|spacing|font|radius|shadow|transition|z)-.+` to `^(color|spacing|font|radius|shadow|transition|z|sheet)-.+` (full-replacement edit, not paraphrase — see warning in § "Stylelint allowed-prefix update").
3. Create `OnboardingWizard.svelte`: full-screen overlay using `var(--sheet-backdrop-color)` + `backdrop-filter: blur(var(--sheet-backdrop-blur))`, centered panel sized with `var(--sheet-width-fraction)` / `var(--sheet-height-fraction)` (use `min(1200px, var(--sheet-width-fraction))` clamps in the same rule), rounded with `var(--sheet-radius)`. Step-dot row at top, Back button bottom-left with `use:tooltip={'Back'}`, primary button bottom-right that delegates to the active step. `dialogId="onboarding"` for MCP tracking. No Escape handler (intentional).
4. **Implement a hand-rolled focus trap** on the wizard panel. Pattern: panel gets `tabindex="-1"` and is focused on mount; a `keydown` handler watches for `Tab` / `Shift+Tab`, queries focusables fresh on every keypress via `panel.querySelectorAll('button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])')`, and wraps when `activeElement === last && !shiftKey` (focus first + `preventDefault`) or `activeElement === first && shiftKey` (focus last + `preventDefault`). Re-query every Tab — the focusable set changes as the user fills in API key, picks model, etc. Without this, Tab leaks to the app behind the wizard. `ModalDialog`'s overlay-focused-on-mount approach is insufficient for a multi-step form.
5. Create `OnboardingStepShell.svelte` (padding, scroll container, footer row).
6. Create three stubs `StepFda.svelte` / `StepAi.svelte` / `StepOptional.svelte` that render their step name + a Next button so the dot indicator + Back/Next plumbing works under `CMDR_FORCE_ONBOARDING`.
7. Create `onboarding-state.svelte.ts` with the `openWizard(source)` / `closeWizard()` / `currentStep` API and the step-resume rule (three macOS step-1 paths, one step-2 fallback — see § "Step persistence resume — edge cases").
8. In `+page.svelte`: gate on `CMDR_FORCE_ONBOARDING` via a new `is_force_onboarding()` Tauri command (mirrors `CMDR_MOCK_LICENSE`). When set, call `openWizard({ source: 'force' })` and render `<OnboardingWizard>`. **Do NOT remove the existing `<FullDiskAccessPrompt>` mount** — M2 does the swap. M1 only ADDS the wizard, gated. Stop calling `notifyAiOnboardingComplete()` everywhere (the export is gone after step 9). Keep `notifyOnboardingComplete()`.
9. In `ai-state.svelte.ts`: remove `onboarded` field, `pendingOffer` field, `notifyAiOnboardingComplete()` export, `handleDismiss()`, `handleOptOut()`, `handleDownload()` (the offer-only handlers), and the `'offer'` branch in `updateNotificationFromStatus()`. Drop `'offer'` from `AiNotificationState`. Drop the `dismissAiOffer` / `optOutAi` imports. Update `resetForTesting()`. Keep `handleCancel` / `handleGotIt` and the runtime states (`downloading` / `installing` / `ready` / `starting`).
10. In `AiToastContent.svelte`: delete the `offer` switch case + the actions block under it + the now-unused `.tertiary-link` styles. Drop `handleDismiss` / `handleOptOut` / `handleDownload` imports.
11. **Delete `'offer'`-state test cases** from `apps/desktop/src/lib/ai/ai-state.test.ts`, `apps/desktop/src/lib/ai/ai-toast-sync.svelte.test.ts`, `apps/desktop/src/lib/ai/AiToastContent.a11y.test.ts`, and `apps/desktop/src/lib/ai/AiNotification.test.ts`. Each file's offer-state coverage either deletes outright (e.g. "Offer state renders with X buttons") or moves to assert the new behaviour ("toast does NOT render in `'hidden'` state on first launch"). Don't replace coverage that the wizard now owns — that's M2/M3 a11y tests' job.
12. **Delete the `dismiss_ai_offer` and `opt_out_ai` Tauri commands** in `apps/desktop/src-tauri/src/commands/ai.rs` (or wherever they live — `grep -rn "dismiss_ai_offer\|opt_out_ai" apps/desktop/src-tauri`). Remove their `#[tauri::command]` blocks, remove from the `invoke_handler` builder, and run `cd apps/desktop && pnpm bindings:regen`.
13. In `permissions.rs::check_full_disk_access`: add the `CMDR_MOCK_FDA` short-circuit at the top. Values: `granted` → `true`; `denied` / `notgranted` → `false`. The wizard distinguishes "denied" (user clicked Deny last step) vs "notgranted" (user clicked Allow but TCC still says no) via the persisted setting; the mock controls only the OS-level signal. Linux's `permissions_linux::check_full_disk_access` already returns `true` unconditionally; no mock needed there (the stub path in `src-tauri/src/stubs/` is also fine as-is).
14. Add `is_force_onboarding()` Tauri command (reads `std::env::var("CMDR_FORCE_ONBOARDING")`). The new IPC is called from the main window only, so add it to `apps/desktop/src-tauri/capabilities/default.json` (per the per-window capability split documented in `src-tauri/capabilities/CLAUDE.md`). Regenerate bindings. The new command is a synchronous `#[tauri::command]` that reads the env-var once and returns the bool — no `blocking_with_timeout` needed because env reads don't touch the filesystem or network.
15. **Relocate `pushConfigToBackend()`** from `apps/desktop/src/lib/settings/sections/ai-settings-utils.ts` to a new `apps/desktop/src/lib/settings/ai-config.ts`. Bring along `migrateApiKeysFromSettings()` and any helpers it pulls (e.g. `describeSecretError` paths). Update imports in `AiSection.svelte` and `+layout.svelte` (and any other callers — grep `pushConfigToBackend\|ai-settings-utils`).
16. In `settings-registry.ts`: change `ai.provider` default to `'off'`. Add `onboarding.upgradeNudgeShown: boolean` (hidden, default `false`). No `SCHEMA_VERSION` bump needed — see § "AI configure on step-2 persist" → migration note.
17. In `settings-applier.ts:152` (the `passthroughBackendHandlers` lookup table — use the same pattern as the existing `network.enabled` / `indexing.enabled` / `fileOperations.mtpEnabled` entries, NOT new `if (id === ...) return` cases): add three entries `'ai.provider'`, `'ai.cloudProvider'`, `'ai.cloudProviderConfigs'`, each calling `() => void pushConfigToBackend()`. The helper reads each setting fresh from `getSetting(...)` on every call — do not pass cached values (see § "AI configure on step-2 persist" for the no-cached-args rationale).
18. Rename `setFdaPromptShowing` → `setOnboardingShowing` AND the internal `fdaPromptShowing` module state + the `shouldShowUpdateToast` arg field in `updates/updater.svelte.ts`. Update `updates/updater.test.ts` (all ~12 references), `updates/CLAUDE.md` § "Onboarding gating" (3 references), `onboarding/CLAUDE.md` (2 references), and `(main)/+page.svelte` (4 call sites). Search-and-replace globally on `setFdaPromptShowing` and `fdaPromptShowing` to be sure.
19. Update `apps/desktop/src/lib/ai/CLAUDE.md` and `apps/desktop/src/lib/onboarding/CLAUDE.md` accordingly.

#### Files

**Added:** `OnboardingWizard.svelte`, `OnboardingStepShell.svelte`, `StepFda.svelte` (stub), `StepAi.svelte` (stub), `StepOptional.svelte` (stub), `onboarding-state.svelte.ts`, `OnboardingWizard.a11y.test.ts`, `OnboardingWizard.test.ts` (separate file — see § TDD), `lib/settings/ai-config.ts` (relocation target).

**Modified:** `dialog-registry.ts`, `app.css`, `.stylelintrc.mjs`, `(main)/+page.svelte`, `permissions.rs`, `commands/ai.rs` (deletions), `settings-registry.ts`, `settings-applier.ts`, `ai-state.svelte.ts`, `AiToastContent.svelte`, `updates/updater.svelte.ts`, `updates/updater.test.ts`, `updates/CLAUDE.md`, `capabilities/default.json`, `apps/desktop/src/lib/ipc/bindings.ts` (regenerated), `apps/desktop/src/lib/ai/CLAUDE.md`, `apps/desktop/src/lib/onboarding/CLAUDE.md`, `apps/desktop/src/lib/settings/CLAUDE.md` (note the new applier listener), `lib/settings/sections/ai-settings-utils.ts` (relocation source — leave a re-export stub if anything outside `AiSection.svelte` imports it, otherwise delete), `AiSection.svelte` (import path update), `routes/(main)/+layout.svelte` (import path update for `pushConfigToBackend` / `migrateApiKeysFromSettings`), and the four AI test files: `ai-state.test.ts`, `ai-toast-sync.svelte.test.ts`, `AiToastContent.a11y.test.ts`, `AiNotification.test.ts`.

**Unchanged this milestone:** `FullDiskAccessPrompt.svelte` (still the production FDA mount; M2 deletes).

#### TDD / test plan

Split the two responsibilities into two files (axe-core can't test focus management — it's runtime behaviour, not static DOM):

- **`OnboardingWizard.a11y.test.ts`** (Tier 3 Vitest + axe-core): mount with stub steps. Default state + each-step state. Assert no a11y violations from axe-core (ARIA shape, labels, roles).
- **`OnboardingWizard.test.ts`** (Tier 3 Vitest behaviour, no axe): assert Escape is a no-op (`fireEvent.keyDown(panel, { key: 'Escape' })` → wizard still mounted). Assert focus trap: render a step with three buttons, focus the third, `userEvent.tab()` → `expect(document.activeElement).toBe(first)`; focus first, `userEvent.tab({ shift: true })` → `expect(document.activeElement).toBe(third)`. Re-query case: after a new focusable appears mid-step (e.g. a model picker), Tab from the last button reaches the new focusable, not the first. Also: `openWizard()` with each source produces the expected starting step (table-driven). Back button is disabled on the first reachable step.
- **Vitest applier** (`settings-applier.test.ts` extension): changing `ai.provider` from `'off'` to `'cloud'` triggers `pushConfigToBackend()` once; the helper re-reads each setting fresh (assert via spying on `getSetting` calls inside `pushConfigToBackend`); flipping all three AI settings in one tick triggers one `pushConfigToBackend()` invocation (or N invocations that all read the same fresh values — either is correct).
- **No new Playwright spec yet** — M5 adds the full wizard E2E. M1's Playwright contract is "the existing happy path (FDA fixture grants access) still works"; confirm by running the existing suite.

#### Docs

- Update `apps/desktop/src/lib/onboarding/CLAUDE.md`: add a wizard overview alongside the existing FDA modal docs (the modal is still live in M1). Keep all FDA gotchas (TCC `open()` trigger, Ventura deep-link host, macOS 26 Tahoe regression). Add a "Step persistence and resume" section + "M1 status: wizard exists, only reachable via `CMDR_FORCE_ONBOARDING`."
- Update `apps/desktop/src/lib/ai/CLAUDE.md`: delete the "Onboarding gate suppresses the offer toast" section and the `pendingOffer` gotcha. Add a one-paragraph pointer to the wizard. Add a "Live-apply: provider changes flow through `settings-applier.ts`" note (since M1 adds that listener).
- `docs/architecture.md`: leave the `lib/onboarding/` description until M2 (the modal still exists in M1; the description "Full Disk Access prompt for first-launch onboarding" is still accurate). M2 updates it.
- Do NOT touch `docs/design-system.md` yet (tokens land in `app.css` in M1, but the design-system narrative subsection lands in M6 when the wizard is real).

#### Checks before commit

`./scripts/check.sh` (default suite). Confirm `bindings-fresh` is green after both the new `is_force_onboarding` command and the deleted `dismiss_ai_offer` / `opt_out_ai` commands — run `cd apps/desktop && pnpm bindings:regen` first if needed. Confirm `stylelint` is green after the allowed-prefix update. M1 is not E2E-relevant beyond a smoke; defer `--include-slow` until M5.

#### Commit message

```
Onboarding: wizard skeleton, AI toast cleanup, live-apply for AI

- New OnboardingWizard soft-sheet (reachable via CMDR_FORCE_ONBOARDING only)
- Adds 'onboarding' to SOFT_DIALOG_REGISTRY for MCP tracking
- Existing FullDiskAccessPrompt stays live; M2 does the mount swap
- Flips ai.provider default 'local' → 'off' so the wizard owns AI consent
- Drops the AI offer toast and dismiss_ai_offer / opt_out_ai Tauri commands
- New settings-applier listener for AI provider triplet → pushConfigToBackend()
- Relocates pushConfigToBackend() to lib/settings/ai-config.ts (one push path)
- Renames setFdaPromptShowing → setOnboardingShowing
- CMDR_FORCE_ONBOARDING and CMDR_MOCK_FDA env-var mocks for tests
- Hidden setting: onboarding.upgradeNudgeShown
- Sheet-* design tokens land in app.css; design-system.md update in M6
```

#### Definition of done

`./scripts/check.sh` green. `CMDR_FORCE_ONBOARDING=1 pnpm dev` shows the empty wizard sheet with three step dots, a Back button, and a Next button. Without the env-var, behaviour on `main` is unchanged: first-launch users still see the old `FullDiskAccessPrompt`. AI offer toast no longer appears for any flag combination. Settings store loads cleanly on a fresh data dir.

---

### M2 — Step 1 (FDA) + mount swap

#### Scope

Populate `StepFda.svelte` with the FDA copy + actions. Three variants: first-ask, revoked, already-granted (collapses to a single-Next variant per round-2 #1). Linux skips this step entirely (the wizard's resume rule on Linux starts at step 2). **Swap the production mount** from `<FullDiskAccessPrompt>` to `<OnboardingWizard>` and delete the old modal.

**In:**
- All preserved-logic items from § "Preserved logic from `FullDiskAccessPrompt.svelte`" (1–7).
- Three variants: first-ask, revoked, already-granted.
- Linux-skip wiring (the resume rule treats Linux as "step 1 done" so step 2 is the first reachable step).
- **Allow path requires restart**: footer button toggles from "Open System Settings" to "Restart Cmdr" after the user clicks Open Settings. No advance to step 2 in-session on the Allow path. See § "FDA gate clear-on-Allow" for the why.
- Mount-point swap in `(main)/+page.svelte`.
- Delete `FullDiskAccessPrompt.svelte`.

**Not in:** step 2 transition copy that depends on FDA outcome (M3), step-2 mid-flow `checkFullDiskAccess()` (M3).

#### Implementation steps

1. Port the JSX/copy from `FullDiskAccessPrompt.svelte` into `StepFda.svelte`. Use the verbatim copy from the context bundle § "Step 1 — Full Disk Access" for the new welcome wording.
2. Wrap copy in a `{#if isMacOS()}` block; Linux returns `null` (and `onboarding-state.svelte.ts`'s resume rule treats Linux as "step 1 done" so users land on step 2 directly).
3. Wire `handleAllow` → `checkFullDiskAccess()` re-probe + `openPrivacySettings()` + persist `fullDiskAccessChoice: 'allow'` + show post-action hint + **footer button switches to "Restart Cmdr"** (calls `relaunch` from `@tauri-apps/plugin-process`, the same path the updater uses). Back button stays available so the user can change to Deny. No advance to step 2.
4. Wire `handleDeny` → persist `fullDiskAccessChoice: 'deny'` + `startIndexingAfterFdaDecision()` + advance to step 2 immediately (this clears the FDA gate via the existing IPC).
5. **Back-from-step-2 behaviour:** when the user uses the Back button from step 2 to return to step 1 (typical when they want to switch from Deny to Allow), both Allow and Deny buttons remain interactive regardless of prior selection. Neither is visually pre-selected — no "sticky" radio state. Clicking either re-persists `fullDiskAccessChoice` and follows the same forward flow (Allow → footer button flips to "Restart Cmdr"; Deny → re-fires `startIndexingAfterFdaDecision()` (idempotent) + advances). This matches step 1's mental model of "the OS-level decision is what's persisted; the buttons are always live."
6. Add the "already granted" variant: when `openWizard({ source: 'menu' })` and `checkFullDiskAccess() === true`, render a single line ("Cmdr currently has Full Disk Access. You can revoke it any time in System Settings.") + a Next button. No Allow/Deny buttons.
7. **Mount swap in `(main)/+page.svelte`**: replace `showFdaPrompt`/`handleFdaComplete`/`<FullDiskAccessPrompt>` with `showOnboarding`/`onWizardComplete`/`<OnboardingWizard>`. Wire `onWizardComplete` → `notifyOnboardingComplete()` + `setOnboardingShowing(false)` + `showApp = true` + `showOnboarding = false`. Apply the data-flow rules from § "Data flow" above (three step-1 paths, one step-2 fallback for first-time-stuck users).
8. Delete `apps/desktop/src/lib/onboarding/FullDiskAccessPrompt.svelte` — no other consumer.

#### Files

**Added:** `StepFda.a11y.test.ts`.

**Modified:** `StepFda.svelte` (populated), `OnboardingWizard.svelte` (route Next/Back, render "Restart Cmdr" button on Allow path), `onboarding-state.svelte.ts` (Linux skip rule + Allow-requires-restart state), `(main)/+page.svelte` (mount swap), `docs/architecture.md` (update `lib/onboarding/` description).

**Deleted:** `FullDiskAccessPrompt.svelte`.

#### TDD / test plan

- **Tier 3 Vitest**: `StepFda.a11y.test.ts` with three sub-tests (first-ask, revoked, already-granted). Mock `$lib/tauri-commands` (`checkFullDiskAccess`, `getMacosMajorVersion`, `openPrivacySettings`, `startIndexingAfterFdaDecision`, `relaunch`).
- **Vitest behaviour** (`StepFda.test.ts`): clicking Allow re-probes before opening Settings (assert call order), clicking Deny persists `'deny'` + calls `startIndexingAfterFdaDecision()` + advances to step 2, the "macOS 12" branch shows end-of-list wording, the Allow path shows "Restart Cmdr" as the next primary action and never advances in-session.
- **Playwright tier-2**: not yet — full wizard spec lands in M5. M2's Playwright contract is "the existing FDA-fixture test path still passes." The fixture grants FDA so the wizard's resume rule lands users on step 2 directly (which is still a stub in M2); the spec advances and completes via the stub Next buttons.

#### Docs

- Update `apps/desktop/src/lib/onboarding/CLAUDE.md`: replace the "Behavior" section with a step-1 spec. Keep TCC + Ventura + Tahoe gotchas verbatim.

#### Checks before commit

`./scripts/check.sh`. M2 doesn't touch any slow-lane spec; defer `--include-slow` until M5.

#### Commit message

```
Onboarding: step 1 (Full Disk Access) ported into wizard

- StepFda.svelte takes over FullDiskAccessPrompt's copy + actions
- Three variants: first-ask, revoked, already-granted (single-Next)
- Linux skips step 1 entirely
- Preserves TCC re-probe, Ventura-vs-older wording, macOS 26 +-button tip
- Deletes FullDiskAccessPrompt.svelte (no remaining caller)
```

#### Definition of done

`CMDR_FORCE_ONBOARDING=1 CMDR_MOCK_FDA=notgranted pnpm dev` shows step 1 first-ask. `CMDR_MOCK_FDA=granted` shows the collapsed already-granted variant. Allow persists `'allow'`, shows the post-action hint, and the footer's primary button becomes "Restart Cmdr" (no advance in-session — see § "FDA gate clear-on-Allow"). Deny persists `'deny'`, calls `startIndexingAfterFdaDecision()`, and advances to step 2 stub. `FullDiskAccessPrompt.svelte` is gone. Production launch on a fresh data dir (no `CMDR_FORCE_ONBOARDING`) opens the wizard at step 1 first-ask.

---

### M3 — Step 2 (AI)

#### Scope

The meaty milestone. Populate `StepAi.svelte` + ship `CloudProviderPicker.svelte` + `CloudProviderSetup.svelte`. All 15 cloud providers from `cloud-providers.ts`. Three FDA-outcome copy branches. Local-model background download orchestration. Re-use the connection-check pipeline from `AiCloudSection.svelte` (don't fork it).

**In:** the three radio choices (cloud / local / no-AI), provider list with arrows + type-to-jump, per-provider setup tutorial with auto-check + model picker, FDA-state banner (one-shot `checkFullDiskAccess()` on step entry), background `startAiDownload()` on local pick + cancellation on switch-away, the dual-button footer ("Start using Cmdr!" vs "One more optional setup step" with the optional one styled as primary).

**Not in:** step 3 implementation (M4).

#### Implementation steps

1. Build `CloudProviderPicker.svelte`: scrollable `<ul role="listbox">` of all 15 providers, with arrow up/down nav, Home/End, and type-to-jump. Try lifting `file-explorer/pane/type-to-jump-state.svelte.ts` first — the factory takes a `getResetMs` callback so it should be reusable. If it turns out to be pane-coupled (cursor/snapshot dependencies), inline a small prefix-match-with-reset matcher in the picker — don't fight the factory. Exposes `value` + `onChange`. ARIA: `<li role="option" aria-selected>`, listbox owns roving tabindex.
2. Build `CloudProviderSetup.svelte`: numbered tutorial steps. Steps:
   1. Open the provider's API key page (link via `openExternalUrl`).
   2. Create an API key.
   3. Paste it here → `<SettingPasswordInput controlled>` bound to local state; on change persist via `saveAiApiKey(providerId, value)`. If `SettingPasswordInput` doesn't yet support controlled mode (it should per `lib/settings/CLAUDE.md` § "Components"; verify on M3 day 1), either inline a small password input here or add controlled mode as a pre-task.
   4. Pick a model → combobox driven by the `/models` fetch from `check_ai_connection`.
   Each step's checkmark flips on completion. Auto-check fires on key-or-base-URL change with the existing 1s debounce. Connection status copy reuses the wording in `AiCloudSection.svelte:536-585`.
3. Build `StepAi.svelte`: one-shot `checkFullDiskAccess()` on step-2 entry from step 1 (or on mount via menu re-entry), pick one of the three FDA-state banner copy branches per context bundle § "Step 2 — AI". Render the comparison table (verbatim from context bundle). Render the three radio choices, **pre-selected from the persisted `ai.provider` + cloud-provider config** so a crash-then-resume user (see § "Step persistence resume — edge case") sees their previous pick highlighted. When `!isOnboarded && ai.provider !== 'off'`, prepend a passive cue: "You picked this last time. Confirm or change below." When "cloud" is chosen, render `<CloudProviderPicker>` + `<CloudProviderSetup>` side-by-side. When "local" is chosen, kick off `startAiDownload()` (and `cancelAiDownload()` if the user switches away within the same wizard session). **Intel Mac gate (MAJOR):** mirror `AiLocalSection.svelte`'s `localAiSupported` check (per `ai/CLAUDE.md` § "Apple Silicon only"). Call `getAiRuntimeStatus()` on step-2 entry; if `localAiSupported === false`, disable the "Local" radio and attach a tooltip "Local LLM requires Apple Silicon. Cloud works on Intel." Without this gate, an Intel user picks Local → wizard kicks off `startAiDownload()` → server refuses to start → silent broken state.
4. Persist on click of either footer button: `setSetting('ai.provider', ...)` + (if cloud) `setSetting('ai.cloudProvider', ...)` + `setSetting('ai.cloudProviderConfigs', setProviderConfig(...))` + the API key (already persisted live by step 3 above). **Then `await pushConfigToBackend()`** (imported from `$lib/settings/ai-config`, the relocated helper) so the backend's `ManagerState` is updated before the user lands in the app. **Do not call `configureAi` directly** — its real signature is `(provider, contextSize, cloudApiKey, cloudBaseUrl, cloudModel)` and the resolve-config + fetch-keychain-key plumbing belongs to `pushConfigToBackend()`. The applier listener (added in M1) covers the in-session race where one setting changes alone; the explicit call here covers the all-three-at-once case deterministically and lets the wizard await completion before advancing. On "Start using Cmdr!" → fire `onComplete()`. On "One more optional setup step" → advance to step 3.

   **No-API-key-blocks-advance rule.** Both footer buttons advance/finish unconditionally when "cloud" is picked, even if the API key field is empty or the connection-check is still `idle`/`auth-error`/`connection-error`. Reasoning: the auto-check status is right there in the right column, so the user already has clear feedback; forcing key entry as a precondition would fight users who want to grab the key later. They can re-enter via `Cmdr > Onboarding…` or fix it in Settings. Document this in step 2's primary-button microcopy (no extra "Are you sure?" dialog) and assert in the M3 Vitest behaviour spec: "advance with empty key → `pushConfigToBackend()` still fires; backend persists `ai.provider: 'cloud'`; first AI use surfaces the standard `NotConfigured` error path."

#### Files

**Added:** `CloudProviderPicker.svelte`, `CloudProviderSetup.svelte`, `StepAi.a11y.test.ts`, `CloudProviderPicker.a11y.test.ts`, `CloudProviderSetup.a11y.test.ts`.

**Modified:** `StepAi.svelte` (populated), `OnboardingWizard.svelte` (route Next/Back).

#### TDD / test plan

- **Tier 3 Vitest** per component file: default state + each meaningful sub-state. `CloudProviderPicker.a11y.test.ts` covers listbox semantics. `CloudProviderSetup.a11y.test.ts` covers the four-step tutorial states (idle, key entered, checking, connected). `StepAi.a11y.test.ts` covers the three FDA-banner branches + the three radio states.
- **Vitest behaviour**: arrow-key nav in `CloudProviderPicker` lands on the right option; type-to-jump matches case-insensitively. Picking `local` calls `startAiDownload`; switching away calls `cancelAiDownload`. The FDA banner uses the right copy branch per `checkFullDiskAccess()` outcome (mocked). **Intel gate:** mock `getAiRuntimeStatus()` to return `localAiSupported: false`; assert the local radio is disabled and carries the expected tooltip; assert clicking it does NOT fire `startAiDownload()` or persist `ai.provider: 'local'`. **No-key-advance:** with `ai.provider: 'cloud'` picked and the API key field empty, clicking the primary footer button still calls `pushConfigToBackend()` and persists `ai.provider: 'cloud'`; the wizard advances/finishes.
- **Real-API smoke** (M3 specific): `apps/desktop/test/e2e-playwright/onboarding-real-openai.spec.ts` — reads the key from keychain (`security find-generic-password -s OPENAI_API_KEY -a veszelovszki -w`), picks OpenAI, pastes the key, waits for `connected`, picks model **`gpt-5-mini` by default** (proven working in existing `ai/CLAUDE.md` real-API tests), with an `OPENAI_TEST_MODEL` env-var override to use `gpt-5.5` when David's credits make that preferable. Assert the model picker populated, fires "Start using Cmdr!", then asserts a follow-up `configureAi` IPC fired (via `pushConfigToBackend()`) with the resolved cloud baseUrl + model + the keychain-fetched API key. Skip if the keychain command fails (CI doesn't have it).
- **Playwright tier-2 happy path**: not yet — landed in full in M5.

Each spec must run < 1–2 s. If `CloudProviderSetup.test.ts`'s "auto-check on key change" debounce makes it slow, advance the debounce timer with `vi.useFakeTimers()`.

#### Docs

- Update `apps/desktop/src/lib/onboarding/CLAUDE.md`: add a "Step 2: AI provider picker" section with the FDA-banner state machine.
- Update `apps/desktop/src/lib/ai/CLAUDE.md`: add a one-liner noting the wizard reuses the connection-check pipeline; the pipeline itself is documented in `lib/settings/CLAUDE.md` § "AiSection".

#### Checks before commit

`./scripts/check.sh`. Run the real-OpenAI smoke separately (it's network-touching): `cd apps/desktop && pnpm playwright test test/e2e-playwright/onboarding-real-openai.spec.ts --headed=false` (skip if keychain key missing). Defer `--include-slow` until M5.

#### Commit message

```
Onboarding: step 2 (AI provider picker)

- StepAi: 3 FDA-outcome banner branches via one-shot checkFullDiskAccess()
- All 15 cloud providers from cloud-providers.ts, scrollable + type-to-jump
- Per-provider setup tutorial reuses AiCloudSection's connection-check pipeline
- Local pick kicks off background download (HTTP-Range resume on switch-back)
- Intel Macs: Local radio disabled with explanatory tooltip (mirrors AiLocalSection)
- Empty/invalid API key does NOT block advance; auto-check status is feedback enough
- Dual-button footer: 'Start using Cmdr!' vs 'One more optional setup step'
- Real-OpenAI smoke spec (gated on keychain key presence)
```

#### Definition of done

`CMDR_FORCE_ONBOARDING=1 pnpm dev`, advance to step 2, pick OpenAI, paste a real key from keychain, watch the model list populate, click "Start using Cmdr!". Settings file shows `ai.provider: 'cloud'`, `ai.cloudProvider: 'openai'`, `ai.cloudProviderConfigs` has the chosen model, keychain has the API key. Picking "local" starts the toast in the corner. Picking "no AI" persists `ai.provider: 'off'`.

---

### M4 — Step 3 (optional)

#### Scope

Populate `StepOptional.svelte` with the four toggle blocks. All copy verbatim from context bundle § "Step 3 (optional)". Defaults stay on; the step is about giving users a chance to turn things off with full context. **Includes a live-apply audit of the four target settings** — anything missing a `handleSettingChange` case lands in this milestone (per `lib/settings/CLAUDE.md` § "Live-apply rule").

**In:**
- Live-apply audit of `network.enabled`, `indexing.enabled`, `updates.autoCheck`, `fileOperations.mtpEnabled` — see § "Live-apply audit" below.
- Four toggles bound to those settings.
- Single "Start using Cmdr" button.

**Not in:** No proactive Local Network prompt trigger (covered by `network.firstTriggerDone` already). No indexing-cache cleanup (out of scope per context bundle).

#### Live-apply audit (BLOCKER fix — Day 1 task)

Before building the toggles, grep each ID in `apps/desktop/src/lib/settings/settings-applier.ts` and confirm a `handleSettingChange` case exists that wires the right side effect:

```
grep -n "'network\.enabled'\|'indexing\.enabled'\|'updates\.autoCheck'\|'fileOperations\.mtpEnabled'" apps/desktop/src/lib/settings/settings-applier.ts
```

For each ID, follow the case body to its Tauri command + verify a corresponding backend handler exists in `src-tauri/src/`. The settings system's contract (`lib/settings/CLAUDE.md` § "Live-apply rule") is that **every** setting MUST apply without restart. If any of the four lacks a case OR has one that wires to a no-op / non-existent IPC, fix it in this milestone — adding the case is M4 scope, not an out-of-scope deferral.

Expected wiring (verify each, don't trust):
- `network.enabled` → wired in `settings-applier.ts:164` (`setNetworkEnabled`). Confirm side effect still works (drops mDNS + clears discovered hosts when off, per `network/CLAUDE.md`).
- `indexing.enabled` → wired at `settings-applier.ts:156` (`setIndexingEnabled`). Confirm runtime start/stop per `indexing/CLAUDE.md`.
- `fileOperations.mtpEnabled` → wired at `settings-applier.ts:157` (`setMtpEnabled`). Confirm MTP hotplug watcher start/stop.
- `updates.autoCheck` → **confirmed missing** (not in the `passthroughBackendHandlers` table). Add an entry to the lookup table that calls into `updates/updater.svelte.ts` to cancel/restart the poll loop live. This is M4 scope, not deferred.

Use the same `passthroughBackendHandlers` lookup-table pattern at `settings-applier.ts:152` — don't introduce per-id `if (id === ...) return` cases.

Document the audit result in the M4 commit body (one line per ID: "already wired" or "added X").

#### Implementation steps

1. Run the live-apply audit (above). Add any missing applier cases.
2. Build `StepOptional.svelte`: four `<section>`s, each with the question + long-form explanation + a `<SettingSwitch>` (binary on/off — switch is cleaner than `SettingToggleGroup` for two options). Bind each switch directly to its setting via `setSetting('network.enabled', value)` etc.
3. Footer: single "Start using Cmdr" primary button → `onComplete()`.

#### Files

**Added:** `StepOptional.a11y.test.ts`.

**Modified:** `StepOptional.svelte` (populated), `OnboardingWizard.svelte` (route Next/Back).

#### TDD / test plan

- **Tier 3 Vitest**: `StepOptional.a11y.test.ts` — default state (all four on), one-off state (one toggle off). Assert each switch is labelled by the question heading + has a description.
- **Vitest behaviour**: toggling each switch calls `setSetting` with the correct id + value. "Start using Cmdr" calls `onComplete`.

#### Docs

- Update `apps/desktop/src/lib/onboarding/CLAUDE.md`: add a "Step 3" section noting which existing setting IDs the toggles bind to.

#### Checks before commit

`./scripts/check.sh`. M4 doesn't touch slow-lane specs; defer `--include-slow` until M5.

#### Commit message

```
Onboarding: step 3 (optional setup) toggles

- Networking, drive indexing, auto-updates, MTP — four switches
- Binds to existing registry settings; applier handles side effects live
- Verbatim copy from David's draft (context bundle § Step 3)
```

#### Definition of done

`CMDR_FORCE_ONBOARDING=1 pnpm dev`, walk through all three steps, toggle one off on step 3, click "Start using Cmdr", confirm the corresponding setting flipped in the settings file. Wizard closes, app shows.

---

### M5 — Re-entry + Playwright full pass + slow lane

#### Scope

Add the `Cmdr > Onboarding…` menu item (macOS only, by design), the `cmdr.openOnboarding` command-palette command, and the one-time upgrade-nudge toast for legacy users. Land the full Playwright tier-2 spec covering happy path + all edge branches. Run `--include-slow`.

**In:** macOS menu item (under "Check for updates…" per round-3 #6) + Linux palette-only entry, palette command on both platforms, upgrade-nudge toast (gated by `onboarding.upgradeNudgeShown`), full Playwright spec, real-OpenAI smoke run. Label is **"Onboarding…"** everywhere (menu label, palette label, the toast's link copy) — picked over "Open onboarding…" per context bundle round-3 #6. Internal command id stays `cmdr.openOnboarding`.

**Not in:** any new step content. **Not in:** a Linux menu entry — design decision below.

#### Implementation steps

1. In `apps/desktop/src-tauri/src/menu/mod.rs`: add `pub const OPEN_ONBOARDING_ID: &str = "open_onboarding";`.
2. In `menu/macos.rs`: add a `MenuItem::with_id` for "Onboarding…" right after `check_for_updates_item` (insert at position 3 in the Cmdr app menu, push later items down). Update the `register_item` index comment.
3. **Skip `menu/linux.rs`.** Decision: Linux re-entry is palette-only by design. The wizard's design language is macOS-centric (frosted backdrop matches macOS sheets, "Restart Cmdr" copy assumes Quit & Reopen flow, FDA-relevance), so the Linux menu doesn't need a redundant entry next to the palette command. The palette command (step 4) works identically on both platforms. Document this choice in `lib/onboarding/CLAUDE.md` § "Re-entry points".
4. In `apps/desktop/src/lib/commands/command-registry.ts`: add `cmdr.openOnboarding` (label **"Onboarding…"**, category "App") — matches the macOS menu item label for consistency. In `routes/(main)/command-dispatch.ts`'s `handleCommandExecute` switch: route to `openWizard({ source: 'menu' })`.
5. In `+page.svelte`'s `setupMenuListeners()` flow (or the menu-dispatch table — whichever is the canonical wiring point): handle the `open_onboarding` menu action by calling `openWizard({ source: 'menu' })`.
6. In `+page.svelte`'s onMount: after the `hasFda && isOnboarded` branch concludes, check `getSetting('onboarding.upgradeNudgeShown') === false`; if so, fire an `info` toast pointing to the menu item ("New: Onboarding settings live under Cmdr > Onboarding…"). Persist `onboarding.upgradeNudgeShown: true` so it never fires again.
7. **MCP open path**: verify the existing MCP `dialog` tool can open the new `'onboarding'` ID. Read `src-tauri/src/mcp/CLAUDE.md` § "dialog" + skim the dialog tool's open switch. If the tool is generic over `SoftDialogId` (it likely is — dialogs are dispatched through a registry-keyed switch), no code change needed beyond M1's registry addition. If the dialog tool only emits the soft-dialog *opened* notification but doesn't actively open dialogs from MCP, add a dedicated `open_onboarding` MCP command in `src-tauri/src/mcp/tools/` that emits a `cmdr-open-onboarding` Tauri event the frontend listens for in `+page.svelte` (`event.listen('cmdr-open-onboarding', () => openWizard({ source: 'menu' }))`). Either way, resolve in M5 — do not push to "open questions."
8. Write `apps/desktop/test/e2e-playwright/onboarding-wizard.spec.ts`. Specs (each under 1–2 s of test body work after `ensureAppReady()`; Tauri cold-start is its own budget, ~3–5 s per spec, and is acceptable because each branch needs its own env-var permutation). Use `CMDR_FORCE_ONBOARDING` + `CMDR_MOCK_FDA` env-vars on launch:
   - Happy path: Allow → grant (mock `granted`) → AI step (pick no-AI) → optional step → finish.
   - Allow + didn't grant (`CMDR_MOCK_FDA=notgranted` after step-1 "Allow" click): step-2 banner shows the "you might need to restart" copy. (Step-1 Allow expects "Restart Cmdr" to surface; the spec asserts the button label without actually relaunching.)
   - Deny: step-2 banner shows the "we respect that" copy.
   - Linux skip-step-1: launch in Linux env (Docker E2E), step 1 not shown, wizard opens at step 2. Re-entry on Linux happens via command palette (no menu entry); the spec opens the palette, runs "Onboarding…", asserts the wizard appears at step 2 (Linux's first reachable step).
   - Re-entry from menu (macOS): invoke `Cmdr > Onboarding…`, wizard opens at step 1 with the already-granted single-Next variant when FDA is granted.
   - Re-entry from palette (macOS): open the palette, run "Onboarding…", same expectation as the menu path.
   - Upgrade nudge: legacy user (`isOnboarded=true` seeded) sees the info toast once, never again.

#### Files

**Added:** `onboarding-wizard.spec.ts`.

**Modified:** `menu/mod.rs`, `menu/macos.rs`, `command-registry.ts`, `command-dispatch.ts`, `+page.svelte`. (No `menu/linux.rs` — palette-only on Linux.)

#### TDD / test plan

- **Playwright tier-2**: the 7 sub-specs above.
- **Real-OpenAI smoke**: re-run the M3 spec at the end of M5 (this milestone touches the open-wizard plumbing, so re-verifying end-to-end with a real provider is the right gate).
- **Tier 3 Vitest**: extend `OnboardingWizard.a11y.test.ts` with a "menu re-entry → already-granted variant" sub-test (axe only). Add a behaviour test in `OnboardingWizard.test.ts` for "menu re-entry source advances `openWizard` past the resume rule and always opens step 1 (macOS) / step 2 (Linux)."

#### Docs

- Update `apps/desktop/src/lib/onboarding/CLAUDE.md`: add a "Re-entry points" section listing the macOS menu, the cross-platform palette command, and the upgrade nudge. Explicitly state "Linux re-entry is palette-only by design."
- Update `apps/desktop/src/lib/commands/CLAUDE.md` if it lists commands by category.

#### Checks before commit

`./scripts/check.sh --include-slow`. This is the milestone where E2E-relevant code lands; full slow-lane is mandatory.

#### Commit message

```
Onboarding: menu + palette re-entry, upgrade nudge, full E2E

- Cmdr > Onboarding… menu item (macOS only), under Check for updates
- cmdr.openOnboarding command palette command (both platforms)
- Linux re-entry is palette-only by design (no menu entry)
- One-time info toast for legacy users (onboarding.upgradeNudgeShown)
- Playwright spec: happy path + 6 edge branches, each < 2s body work
- Real-OpenAI smoke re-run via keychain key
```

#### Definition of done

`Cmdr > Onboarding…` opens the wizard at step 1 (macOS). Command palette finds it as "Onboarding…" on both platforms. Legacy-user toast fires exactly once on the first launch after upgrade. `./scripts/check.sh --include-slow` green.

---

### M6 — Polish + design-system + a11y audit

#### Scope

Promote the wizard's sheet design language from "tokens-only in `app.css`" (added in M1) to a documented pattern in `docs/design-system.md`. Final a11y sweep (tier 1 contrast, tier 2 Playwright accessibility spec extension, tier 3 already done per milestone). Docs sweep. Final `--include-slow`.

**In:** `docs/design-system.md` "Soft-sheet wizard" subsection citing the `--sheet-*` tokens, additional reusable style choices lifted from `OnboardingWizard.svelte` into `app.css` if any emerged during M3/M4, tier-2 a11y spec extension, doc sweep across all touched CLAUDE.mds.

**Not in:** any behavioural change. (The `--sheet-*` tokens themselves landed in M1; M6 only writes them up in the design system.)

#### Implementation steps

1. Audit `OnboardingWizard.svelte` (post-M5) for any inline values that could become reusable tokens — frosted-glass panel padding rhythm, step-dot sizing, etc. Lift into `app.css` if reused 2+ times across the wizard files.
2. Write the "Soft-sheet wizard" subsection in `docs/design-system.md` § "Component patterns" documenting the `--sheet-*` tokens added in M1, when to use them, and why a sheet differs from `ModalDialog`.
3. Run `./scripts/check.sh --check a11y-contrast` and fix any flagged token combos.
4. Extend `apps/desktop/test/e2e-playwright/accessibility.spec.ts` with an onboarding-wizard scan (open the wizard via `CMDR_FORCE_ONBOARDING`, run axe over each step + each FDA-banner branch).
5. Doc sweep:
   - `docs/architecture.md` → confirm `lib/onboarding/` description is current ("Soft-sheet onboarding wizard: FDA + AI + optional setup").
   - `apps/desktop/src/lib/onboarding/CLAUDE.md` → top-to-bottom rewrite check.
   - `apps/desktop/src/lib/ai/CLAUDE.md` → confirm no stale references to `pendingOffer` / `notifyAiOnboardingComplete` / `dismiss_ai_offer` / `opt_out_ai`.
   - `apps/desktop/src/lib/ui/CLAUDE.md` → mention the wizard isn't a `ModalDialog` consumer (in case a future agent wonders).
   - `apps/desktop/src/lib/settings/CLAUDE.md` → if M1's applier listener for AI provider isn't documented yet, add it.

#### Files

**Modified:** `app.css`, `docs/design-system.md`, `docs/architecture.md` (maybe), all four CLAUDE.mds in the touched modules, `accessibility.spec.ts`, `OnboardingWizard.svelte` (token swap).

#### TDD / test plan

- **Tier 1**: `./scripts/check.sh --check a11y-contrast` green.
- **Tier 2**: extended `accessibility.spec.ts` covers the wizard.
- **Tier 3**: already in place per milestone.

#### Docs

Covered above.

#### Checks before commit

`./scripts/check.sh --include-slow`. This is the wrap milestone; everything must be green.

#### Commit message

```
Onboarding: design-system tokens + a11y sweep + docs

- New sheet-* tokens for wizard sizing/backdrop; documented in design-system.md
- Tier-2 a11y spec covers the wizard end-to-end
- CLAUDE.md sweep: onboarding/, ai/, ui/, architecture.md
```

#### Definition of done

Full `./scripts/check.sh --include-slow` green. `docs/design-system.md` has a "Soft-sheet wizard" subsection. Manual review of all touched CLAUDE.mds confirms no stale references to the old FDA modal, the AI offer toast, or the `pendingOffer` gate.

---

## Cross-cutting concerns

### Keyboard contract (whole wizard)

- **Tab / Shift+Tab**: cycle through interactive elements within the active step in natural DOM order. The wizard panel itself is `tabindex="-1"` and focused on mount so the first Tab lands on the first interactive element.
- **Enter**: activates the focused button. The primary footer button (rightmost) is the wizard's "advance" affordance.
- **Escape**: disabled. No `onclose` on the underlying overlay; the wizard never `closeWizard()`s on Escape.
- **Arrow keys + type-to-jump**: scoped to `CloudProviderPicker.svelte` only (listbox semantics).
- **Back button** (`←` icon, tooltip `Back`): always lets the user return to a previous step; disabled on the first step that's actually rendered (step 1 on macOS, step 2 on Linux — i.e. on Linux the Back button is disabled on step 2 because there's no step 1 to return to). Re-entry on Linux happens via the command palette only (no menu entry); it opens at step 2.

### Focus trap (MAJOR — implementation rule)

The wizard must implement a hand-rolled focus trap (M1 § implementation step 4). Without it, `Tab` from the last interactive element in a step leaks to the file-explorer behind the wizard — the wizard isn't a `<dialog>` and `ModalDialog`'s simpler "overlay tabindex=-1 + focused on mount" isn't enough for a multi-step form with many focusables. Pattern:

- Panel root: `tabindex="-1"`; focus on mount.
- `onkeydown` on the panel: intercept `Tab` / `Shift+Tab`. Find all focusable descendants via `panel.querySelectorAll('button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])')`. If `activeElement === last` and forward Tab → focus first + `preventDefault()`. If `activeElement === first` and Shift+Tab → focus last + `preventDefault()`.
- Re-query on every Tab (don't cache): the focusable set changes as the user fills in the API key, picks a model, etc.
- Tier-3 a11y test asserts both wrap directions.

### A11y (three-tier strategy, per `docs/design-system.md` § "Three-tier a11y testing strategy")

- **Tier 1** (Go contrast scanner): runs over the new sheet tokens once they land in `app.css` (M1).
- **Tier 2** (Playwright `accessibility.spec.ts`): extended in M6 to cover the wizard at each step + each FDA-banner branch.
- **Tier 3** (component-level axe-core): one `.a11y.test.ts` per new component, default + key states. Done per milestone.

Focus management rule: when a step transition happens (Next, Back, or initial mount), the wizard's `currentStep` change must move focus to the first interactive element of the new step within an `await tick()`. Without this, screen-reader users hear nothing change after Enter.

### MCP dialog registration + open path

The wizard registers as `dialogId="onboarding"` via `OnboardingWizard.svelte`. The MCP `available dialogs` resource updates automatically because `SOFT_DIALOG_REGISTRY` is the source of truth (per `apps/desktop/src/lib/ui/dialog-registry.ts:5`). M5 § implementation step 7 owns the executor-side check: confirm whether the existing MCP `dialog` tool's open path already works for the new id, or add a dedicated MCP command (`open_onboarding` → emits `cmdr-open-onboarding` event → frontend calls `openWizard({ source: 'menu' })`). Resolve in M5, not as an open question.

### Persistence schema for partial-completion recovery

No new fields. The state is fully encoded by:

- `fullDiskAccessChoice` (existing): `'notAskedYet' | 'allow' | 'deny'`.
- `isOnboarded` (existing): `boolean`, flipped to `true` only on full wizard completion via `notifyOnboardingComplete()` (existing helper persists it).
- `ai.provider` (existing): `'off' | 'cloud' | 'local'`. Now defaulting to `'off'`. The wizard's step 2 writes this on choice.
- `onboarding.upgradeNudgeShown` (new): hidden boolean, default `false`, flipped to `true` after firing the upgrade-nudge toast once.

The resume rule lives in `onboarding-state.svelte.ts::resumeStepFor(settings, hasFda)` and has THREE step-1 paths and one step-2 path on macOS, plus Linux straight to step 2 — see § "Step persistence resume — edge cases" above for the full source. Key distinction: `fullDiskAccessChoice === 'allow' && !hasFda` splits on `isOnboarded` (`true` = revoked-later, step-1 revoked copy; `false` = first-time stuck post-restart, step 2 with the "didn't grant" banner).

Step 3 is never the resume step because it's optional; users either reach it via step 2's "One more optional setup step" button or skip it forever.

**Accepted edge case** (Edge A above): a user who finished step 2 (picked AI provider) but crashed before step 3 re-sees step 2 next launch. Step 2 pre-fills the previous choice + shows a passive cue ("You picked this last time. Confirm or change below.") — one Enter to advance. No new schema field.

### Env-var mocks (naming + scope)

Mirror `CMDR_MOCK_LICENSE`'s pattern (read in Rust, present in env tests):

- `CMDR_FORCE_ONBOARDING=1` — read by a new `is_force_onboarding()` Tauri command (lives in `permissions.rs` or a new `commands/dev.rs` — executor's call). Frontend reads it on mount; if `true`, opens the wizard regardless of `isOnboarded`.
- `CMDR_MOCK_FDA=granted|denied|notgranted` — short-circuits `permissions.rs::check_full_disk_access` at the top. `granted` → `true`; `denied`/`notgranted` → `false`. (The wizard branches on `denied` vs `notgranted` via the persisted setting + a fresh `checkFullDiskAccess()` call on step-2 entry; the mock just controls the OS-level signal.) **Scope:** macOS only — Linux's `permissions_linux::check_full_disk_access` returns `true` unconditionally, so no mock is needed there; the stub path under `src-tauri/src/stubs/` is also fine as-is.

Both env-vars are explicitly **dev/test only** and documented in `apps/desktop/src/lib/onboarding/CLAUDE.md` § "Testing".

### Sheet sizing tokens (land in M1; documented in M6)

Token names (executor may rename if a more idiomatic name emerges):

- `--sheet-width-fraction: 90vw` (clamped: `width: min(1200px, var(--sheet-width-fraction))`).
- `--sheet-height-fraction: 90vh` (clamped similarly: `height: min(900px, var(--sheet-height-fraction))`).
- `--sheet-radius: var(--radius-lg)` (8 px, matches macOS sheet convention).
- `--sheet-backdrop-blur: 20px` (matches the tooltip frosted-glass value in `docs/design-system.md` § "Tooltips").
- `--sheet-backdrop-color`: light `rgba(0,0,0,0.4)`, dark `rgba(0,0,0,0.6)` (matches `ModalDialog` overlay convention from `docs/design-system.md` § "Dialogs (app)").

### Stylelint allowed-prefix update (M1 task)

`ModalDialog`'s `containerStyle` exists because stylelint's `custom-property-pattern` at `.stylelintrc.mjs:48` blocks custom CSS vars that don't match `^(color|spacing|font|radius|shadow|transition|z)-.+`. The new `--sheet-*` vars are added to this allowlist in M1 by extending the regex to `^(color|spacing|font|radius|shadow|transition|z|sheet)-.+` (NOT a paraphrase — full-replacement edit; paraphrasing to `(color|spacing|font|sheet)-` silently drops `radius` / `shadow` / `transition` / `z` and breaks every existing token). M1 owns this so the wizard skeleton can consume the tokens from day one; without it M1's `app.css` additions fail lint and the skeleton would have to hardcode literals.

### Local-model download cross-session resume

Per `ai/CLAUDE.md` § "Download resumption via HTTP Range", model downloads support HTTP Range for resume after interruption. Caveat: cross-session resume is fine within ~24 h, beyond that the startup cleanup may wipe the partial and the next pick starts fresh from byte zero. Not a bug — just the cost of leaving stale partial files around. The wizard doesn't need special handling for this; users see the existing top-right `downloading` toast, which displays progress from wherever the resume lands.

## Parallelism notes

Most milestones are sequential — M3 needs M1 + M2's plumbing, M4 needs M3's footer wiring (the "One more optional setup step" button advances to step 3), M5 needs all step content stable. Safe parallel work within a milestone:

- **In M1**: the AI toast cleanup (`ai-state.svelte.ts` + `AiToastContent.svelte` + `ai/CLAUDE.md`) is independent from the wizard skeleton. Two agents can work on them in parallel, then merge.
- **In M3**: `CloudProviderPicker.svelte` and `CloudProviderSetup.svelte` are independent component shells (they only meet at `StepAi.svelte`'s layout). One agent each is fine.
- **In M5**: menu wiring (`menu/{mod,macos,linux}.rs`) and command-palette wiring (`command-registry.ts` + `command-dispatch.ts`) are independent. The Playwright spec depends on both.

Across milestones: don't parallelise. Each milestone ends with a full check suite + commit; running M3 work before M2 has committed risks the executor agent re-doing decisions M2 just made.

## Open questions for the executor leader

Zero. Round-2 + round-3 reviews resolved every prior question. The plan now makes a concrete call on every previously-deferred decision:

- `CMDR_FORCE_ONBOARDING` → Rust-side via `is_force_onboarding()` Tauri command (M1).
- `dismiss_ai_offer` / `opt_out_ai` Tauri commands → **deleted in M1** (dead IPC surface).
- Linux menu re-entry → **palette-only by design** (no Linux menu entry). M5 step 3.
- `SettingPasswordInput` controlled mode → verify on M3 day 1; if missing, inline a small password input or add controlled mode as a pre-task (M3 § implementation step 2).
- `type-to-jump` reuse → try lifting the factory first; if pane-coupled, inline a small matcher (M3 § implementation step 1).
- Stylelint allowed-prefix → extend FROM `^(color|spacing|font|radius|shadow|transition|z)-.+` TO `^(color|spacing|font|radius|shadow|transition|z|sheet)-.+` in M1 (NOT a paraphrase — full replacement preserves all existing tokens).
- MCP `open_onboarding` → resolve in M5 § implementation step 7 (use existing dialog tool if generic, else add dedicated MCP command + event).
- FDA gate clear-on-Allow → require restart on Allow before advancing (no new IPC).
- Resume edge cases → (A) accept step-2 re-confirmation post-crash with pre-fill + passive cue; (B) split `'allow' && !hasFda` on `isOnboarded` so first-time-stuck users land on step 2 with the "didn't grant" banner, while revoked-later users land on step 1 with the "wasRevoked" copy.
- `configureAi` signature → don't call directly. Use the existing `pushConfigToBackend()` helper (M1 relocates to `lib/settings/ai-config.ts`); wizard + applier are the new callers.
- `ai.provider` default-flip migration → no migration / no `SCHEMA_VERSION` bump needed (existing users have a stored value already).
- `setFdaPromptShowing` rename → touches code + tests + 2 CLAUDE.mds; full search-and-replace on both `setFdaPromptShowing` and the internal `fdaPromptShowing` field, listed exhaustively in M1 step 18.
- Menu/palette label → "Onboarding…" everywhere; internal id stays `cmdr.openOnboarding`.
