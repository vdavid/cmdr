# Onboarding module

First-launch consent in the `OnboardingWizard` soft sheet. Flow: FDA (1, macOS only) → AI (2) → Open beta, with the
analytics disclosure and terms (3) → Optional settings (4); Linux resumes at step 2.

## Module map

`OnboardingWizard` (shell) + `OnboardingStepShell` (per-step frame), `StepFda` / `StepAi` / `StepBeta` / `StepOptional`,
`OnboardingToggleCard` (the switch card steps 3 and 4 share), `CloudProviderPicker` / `CloudProviderSetup` (AI step),
`OnboardingLanguagePicker`, and `onboarding-state.svelte.ts` (step cursor, variants, banner mode, footer override +
note, `resumeStepFor()`).

## Must-knows

- **The language picker in the header is FRAME, ❌ never a step, and ❌ never gets a companion notice.** Cmdr follows
  the Mac's language preferences, so a first launch can land in a language the user can't read while every other way out
  is labeled in it. It's `SettingSelect` on `appearance.language` (❌ not a fork), portaled into the wizard OVERLAY so
  its menu escapes the panel's `overflow: hidden` without leaving the focus trap. DETAILS § "The language escape hatch".
- **The per-provider setup steps live in `$lib/ai-provider-setup/`**, shared with Settings › AI › Provider. A provider,
  link, or copy change goes there, ❌ never here.
- **The Open beta page (step 3) is non-skippable, and the AI step has no skip-to-finish.** Every first-launch user has
  to see the usage-stats disclosure once: the opt-out default only reads as fair consent if it was shown.
- **Step 3's terms checkbox gates both footer buttons.** ❌ Never pre-tick or route around it: it's the assent the terms
  rest on. Unticked, the buttons take `blockedReason`, ❌ not `disabled`, so a press still fires and reveals the
  checkbox; that reveal focuses it with `preventScroll: true`, ❌ never a plain `focus()`, whose own scroll cancels the
  deliberate one and leaves the press doing nothing. Acceptance stores `TERMS_VERSION` + timestamp.
- **"Thanks but no thanks" lands on THREE things**: `StepAi.persist()` writes `ai.provider = 'off'`, revokes Ask Cmdr
  consent (a `main.db` record, not a setting), and clears `askCmdr.proactive` (which ships ON). ❌ Picking cloud or
  local must NEVER grant consent. DETAILS § "What 'off' turns off".
- **Step 2's missing-API-key gate confirms once, ❌ never blocks.** Cloud with no stored key: the first Next shows
  `footerNote`, the second goes through. Its clearing listeners exempt the wizard FOOTER, which is what lets the second
  press (and Enter, whose `keydown` precedes its click) reach the handler.
- **Step 4's cards are a half-line summary plus an info glyph** whose tooltip carries the long copy as a live
  `contentEl`. ❌ Don't move a `desc` back into the card body.
- **Allow (FDA) requires a restart before advancing past step 1**: the footer flips to "Restart Cmdr" and does NOT
  advance in-session, because the gate (`fda_gate::FDA_PENDING`) is set once at boot and clearing it at runtime races
  the TCC popups it suppresses (we hit 5-10 stacked popups once). Deny advances normally.
- **Step 1's live-grant poller calls `checkFullDiskAccessQuiet`, ❌ never `checkFullDiskAccess`**, which fires a TCC
  registration storm on every denial. It runs only while the Allow/Deny variants are open on macOS, and stops on grant.
- **Two things stay gated on the FDA decision at boot**: the drive indexer and `volumes::list_locations`' path-based
  icon fetches, both via `crate::fda_gate::is_fda_pending(...)`. Deny clears the runtime gate through
  `startIndexingAfterFdaDecision()`; Allow opens it on relaunch.
- **FDA stays a three-state setting** (`notAskedYet` / `allow` / `deny`), ❌ never a boolean: "never asked",
  "granted-then-revoked", and "explicitly declined" are three different screens.
- **`StepBeta` and `StepOptional` reuse existing Settings wiring** (`UpdatesSection`'s `betaSignup` / email path,
  `<SettingSwitch>` via `setSetting()`), and that email path POSTs only the email, ❌ never an install id.
- **Search's coverage note routes INTO step 1** when a walk was refused a folder and Cmdr lacks FDA
  (`coverage-note.ts::offersFullDiskAccess`): ❌ no second FDA prompt, ❌ never over a snapshot folder.

`DETAILS.md` owns the rest, including what stays fixed for a reason: no Escape handler, the step-2 banner branches, and
the `CMDR_FORCE_ONBOARDING` / `CMDR_MOCK_FDA` test overrides. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
