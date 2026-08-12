# Onboarding module

First-launch consent: Full Disk Access (macOS), AI provider, the open-beta analytics disclosure, terms acceptance, and a
small optional-settings step, all in the `OnboardingWizard` soft sheet. Flow: FDA (1) → AI (2) → Open beta (3) →
Optional (4); Linux skips step 1 and resumes at step 2.

## Module map

`OnboardingWizard.svelte` (shell), `OnboardingStepShell.svelte` (per-step frame), `StepFda` / `StepAi` / `StepBeta` /
`StepOptional`, `CloudProviderPicker` / `CloudProviderSetup` (AI step), and `onboarding-state.svelte.ts` (the state
machine: step cursor, variants, banner mode, `resumeStepFor()`).

## Must-knows

- **The Open beta page (step 3) is non-skippable, and the AI step has no skip-to-finish.** Every first-launch user has
  to see the usage-stats disclosure once: the opt-out default only reads as fair consent if it was shown. ❌ Don't
  re-add a skip-to-finish on the AI step.
- **Step 3's terms checkbox gates both footer buttons.** ❌ Never pre-tick or route around it: it's the assent the terms
  rest on. Unticked, the buttons take `blockedReason`, ❌ not `disabled`, so a press still fires and scrolls to the
  checkbox. Acceptance stores `TERMS_VERSION` + timestamp; bumping it re-asks everyone.
- **Allow (FDA) requires a restart before advancing past step 1**: the footer flips to "Restart Cmdr" and does NOT
  advance in-session, because the gate (`fda_gate::FDA_PENDING`) is set once at boot and clearing it at runtime races
  the TCC popups it suppresses (we hit 5-10 stacked popups once). Deny advances normally.
- **Step 1's live-grant poller calls `checkFullDiskAccessQuiet`, ❌ never `checkFullDiskAccess`**, which fires a TCC
  registration storm on every denial. It runs only while the Allow/Deny variants are open on macOS, and stops on grant.
- **Two things stay gated on the FDA decision at boot**: the drive indexer and the path-based icon fetches in
  `volumes::list_locations`, both via `crate::fda_gate::is_fda_pending(...)`. On Deny, `startIndexingAfterFdaDecision()`
  clears the runtime gate and starts them; on Allow, the relaunch opens the gate.
- **FDA stays a three-state setting** (`notAskedYet` / `allow` / `deny`), never a boolean: the app must tell "never
  asked" from "granted-then-revoked" from "explicitly declined".
- **`StepBeta` and `StepOptional` reuse existing Settings wiring** (`UpdatesSection`'s `betaSignup` / email path,
  `<SettingSwitch>` via `setSetting()`), and that email path POSTs only the email, never an install id. ❌ Don't fork
  it.
- **Search's coverage note routes INTO step 1** when a walk was refused a folder and Cmdr lacks FDA
  (`coverage-note.ts::offersFullDiskAccess`): ❌ no second FDA prompt, ❌ never over a snapshot folder. DETAILS owns the
  rest, including what stays fixed for a reason: no Escape handler, the always-enabled AI forward button, the step-2
  banner branches, and the `CMDR_FORCE_ONBOARDING` / `CMDR_MOCK_FDA` test overrides. Read `DETAILS.md` before any
  non-trivial work here: editing, planning, reorganizing, or advising.
