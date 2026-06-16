# Onboarding module

Owns first-launch consent: Full Disk Access (macOS only), AI provider, the open-beta analytics disclosure, and a small
optional-settings step. Renders the `OnboardingWizard` soft-sheet as the single first-launch path.

Flow: FDA (1) → AI (2) → Open beta (3) → Optional (4). Linux skips step 1 and resumes at step 2.

## Module map

- `OnboardingWizard.svelte` (shell), `OnboardingStepShell.svelte` (per-step frame), `StepFda` / `StepAi` / `StepBeta` /
  `StepOptional`, plus `CloudProviderPicker` / `CloudProviderSetup` for the AI step.
- `onboarding-state.svelte.ts`: the state machine (step cursor, variants, banner mode, `resumeStepFor()`).

## Must-knows

- **The Open beta page (step 3) is non-skippable, and the AI step has no skip-to-finish.** Every first-launch user must
  see the anonymous-analytics disclosure once (the opt-out default only reads as fair consent if it was shown), so the
  AI step's only forward button ("Next") always lands on Beta. Beta itself offers "Start using Cmdr!" (skips the
  optional step) and "One more optional setup step", so the user can't reach the app without passing through Beta. Don't
  re-add a skip-to-finish on the AI step (it bypasses the disclosure).
- **The step-2 "Full disk access granted" banner shows ONLY on a fresh first-run grant** (`hasFda && !isOnboarded`).
  Once onboarded, menu / palette re-entry with FDA on shows no banner (`stepTwoBanner === 'none'`). Gated in both
  `stepTwoBannerFor()` and `StepAi`'s on-mount probe.
- **Allow (FDA) requires a restart before advancing past step 1.** After Allow, the footer flips to "Restart Cmdr"
  (`relaunch()`), it does NOT advance in-session. The FDA gate (`fda_gate::FDA_PENDING`) is set once at boot; clearing
  it at runtime races the TCC popups the gate suppresses (we hit 5-10 stacked popups once). The resume rule lands the
  user on step 2 after relaunch. Deny advances normally.
- **Step 1 polls for a live FDA grant (macOS).** While the Allow/Deny variants are open and FDA isn't granted, a 500 ms
  `$effect` poller in `StepFda` calls `checkFullDiskAccessQuiet` (the side-effect-free probe, NOT `checkFullDiskAccess`,
  which fires the TCC-registration storm and logging on every denial). On grant it calls `setStep1Granted()` (success
  state, footer flips to "Restart Cmdr") and stops; the interval also clears on unmount (no leaks). The restart still
  applies (the gate is boot-set); don't try to clear it live. The poller never runs on `already-granted` or Linux.
- **No Escape handler on the wizard.** Dismissing without choosing leaves no recorded preference; the user must commit
  to Allow / Deny / Next on each step. (Closing requires committing to a step; MCP close/focus aren't wired.)
- **The AI step's forward button stays enabled regardless of API-key validity** (no-key-blocks-advance). Don't gate
  advance on connection status; the auto-check is informational. First AI use surfaces the standard `NotConfigured`
  path.
- **FDA stays a three-state setting** (`notAskedYet` / `allow` / `deny`), never a boolean: the app must tell "never
  asked" from "granted-then-revoked" from "explicitly declined".
- **Two things stay gated on the FDA decision at boot:** the drive indexer and the path-based icon fetches in
  `volumes::list_locations`, both via `crate::fda_gate::is_fda_pending(...)`. On Deny, `startIndexingAfterFdaDecision()`
  clears the runtime gate and starts the indexer/MTP watcher (one TCC popup per protected folder). On Allow, the
  relaunch opens the gate at boot. See `src-tauri/src/fda_gate.rs`.
- **`StepBeta` and `StepOptional` reuse existing Settings wiring** (`UpdatesSection`'s `betaSignup`/email path,
  `<SettingSwitch>` writing via `setSetting()`). The email path POSTs only the email, never an install id. Don't fork
  it.
- **`CMDR_FORCE_ONBOARDING=1`** forces the wizard regardless of persisted state;
  `CMDR_MOCK_FDA=granted|denied|notgranted` overrides the TCC probe for testing all banner branches.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
