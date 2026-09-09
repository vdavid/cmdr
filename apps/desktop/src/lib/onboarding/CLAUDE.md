# Onboarding module

First-launch consent in the `OnboardingWizard` soft sheet. Flow: FDA (1, macOS only) → AI (2) → Open beta, carrying the
analytics disclosure and terms (3) → Optional settings (4). Linux resumes at step 2.

## Module map

`OnboardingWizard` (shell) + `OnboardingStepShell` (per-step frame), `StepFda` / `StepAi` / `StepBeta` / `StepOptional`,
`OnboardingToggleCard` (steps 3 and 4 share it), `CloudProviderPicker` / `CloudProviderSetup`,
`OnboardingLanguagePicker`, and `onboarding-state.svelte.ts` (step cursor, variants, banner, footer override + note).

## Must-knows

- **The frame is the FOOTER**, a `1fr auto 1fr` grid: Back, the language picker, the centred step dots, the forward
  buttons. Nothing frames the flow from the top, and the panel wears `ModalDialog`'s chrome. DETAILS § "The frame lives
  in the footer".
- **The language picker is FRAME, ❌ never a step, and ❌ never gets a companion notice.** A first launch can land in a
  language the user can't read, with every other way out labeled in it. It's `SettingSelect` on `appearance.language`
  (❌ not a fork), portaled into the wizard OVERLAY so its menu escapes the panel's `overflow: hidden` without leaving
  the focus trap. DETAILS § "The language escape hatch".
- **The per-provider setup steps live in `$lib/ai-provider-setup/`**, shared with Settings › AI › Provider. A provider,
  link, or copy change goes there, ❌ never here.
- **The Open beta page (step 3) is non-skippable, and the AI step has ❌ no skip-to-finish.** Every first-launch user
  sees the usage-stats disclosure once: an opt-out default only reads as fair consent if it was shown.
- **Step 3's terms checkbox gates both footer buttons.** ❌ Never pre-tick or route around it: it's the assent the terms
  rest on. Unticked, the buttons take `blockedReason`, ❌ not `disabled`, so a press still fires and reveals the
  checkbox; that reveal focuses it with `preventScroll: true`, ❌ never a plain `focus()`, whose own scroll cancels the
  deliberate one. Acceptance stores `TERMS_VERSION` + timestamp.
- **"Thanks but no thanks" lands on THREE things**: `StepAi.persist()` writes `ai.provider = 'off'`, revokes Ask Cmdr
  consent (a `main.db` record, not a setting), and clears `askCmdr.proactive` (which ships ON). ❌ Picking cloud or
  local must NEVER grant consent. DETAILS § "What 'off' turns off".
- **Step 2's missing-API-key gate confirms once, ❌ never blocks.** Cloud with no stored key: the first Next warns, the
  second goes through. Its clearing listeners exempt the wizard FOOTER, which is what lets that second press (and Enter,
  whose `keydown` precedes its click) reach the handler. The warning is a forced-open tooltip ON the pressed button
  (`showTooltipNow`), ❌ never text in the footer row.
- **Long copy hides behind an info glyph, ❌ never back in the body**: step 4's cards and step 2's local-AI option lead
  with one line and park the rest in a `contentEl` tooltip. Step 2's options are plain `RadioGroup` rows, so their glyph
  is a `<button>` in `itemTrailing`, ❌ never inside the `role="radio"` element.
- **Allow (FDA) requires a restart before advancing past step 1**, so the footer flips to "Restart Cmdr" and ❌ does not
  advance in-session: `fda_gate::FDA_PENDING` is set once at boot, and clearing it at runtime races the TCC popups it
  suppresses (5-10 stacked, once). Deny advances normally.
- **Step 1's live-grant poller calls `checkFullDiskAccessQuiet`, ❌ never `checkFullDiskAccess`**, which fires a TCC
  registration storm per denial. It runs only while the Allow/Deny variants are open, and stops on grant.
- **The drive indexer and `volumes::list_locations`' icon fetches stay gated on the FDA decision at boot**, via
  `crate::fda_gate::is_fda_pending(...)`. Deny clears the gate (`startIndexingAfterFdaDecision()`); Allow opens it on
  relaunch.
- **`StepBeta` and `StepOptional` reuse existing Settings wiring** (`UpdatesSection`'s `betaSignup` / email path,
  `<SettingSwitch>`), and that email path POSTs only the email, ❌ never an install id.
- **Search's coverage note routes INTO step 1** when a walk was refused a folder and Cmdr lacks FDA
  (`coverage-note.ts::offersFullDiskAccess`): ❌ no second prompt, ❌ never over a snapshot folder.

`DETAILS.md` owns the rest: the three-state FDA setting, no Escape handler, the step-2 banner branches, and the
`CMDR_FORCE_ONBOARDING` / `CMDR_MOCK_FDA` overrides. Read it before any non-trivial work here.
