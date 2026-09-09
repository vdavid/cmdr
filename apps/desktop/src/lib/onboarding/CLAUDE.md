# Onboarding module

First-launch consent in the `OnboardingWizard` soft sheet. Flow: FDA (1, macOS only) → AI (2) → Open beta, carrying the
analytics disclosure and the terms (3) → Optional settings (4). Linux starts at step 2.

## Module map

`OnboardingWizard` (shell) + `OnboardingStepShell` (per-step frame), `StepFda` / `StepAi` / `StepBeta` / `StepOptional`,
`OnboardingToggleCard` (steps 3 and 4 share it), `CloudProviderPicker` / `CloudProviderSetup`,
`OnboardingLanguagePicker`, `onboarding-state.svelte.ts` (the state machine).

## Must-knows

- **The frame is the FOOTER**, a `1fr auto 1fr` grid: Back, the language picker, the centred dots, the forward buttons.
  Nothing frames the flow from the top; the panel wears `ModalDialog`'s chrome. The picker is frame too, ❌ never a step
  and ❌ never with a companion notice: a first launch can land in a language the user can't read with every way out
  labeled in it. It's `SettingSelect` on `appearance.language` (❌ not a fork), portaled into the wizard OVERLAY.
- **The per-provider setup steps live in `$lib/ai-provider-setup/`**, shared with Settings › AI › Provider: a provider,
  link, or copy change goes there, ❌ never here.
- **The Open beta page (step 3) is non-skippable, and the AI step has ❌ no skip-to-finish**: an opt-out analytics
  default only reads as fair consent if every first-launch user was shown it.
- **Step 3's terms checkbox gates both footer buttons.** ❌ Never pre-tick or route around it. Unticked, they take
  `blockedReason`, ❌ not `disabled`, so a press still reveals the box — focusing it with `preventScroll: true`, ❌ never
  a plain `focus()`, whose own scroll cancels the deliberate one.
- **"Thanks but no thanks" lands on THREE things**: `StepAi.persist()` writes `ai.provider = 'off'`, revokes Ask Cmdr
  consent (a `main.db` record), and clears `askCmdr.proactive` (which ships ON). ❌ Cloud or local must NEVER grant
  consent. DETAILS § "What 'off' turns off".
- **Step 2's missing-API-key gate confirms once, ❌ never blocks**: cloud with no stored key warns on the first Next,
  goes through on the second, as a forced-open tooltip on the button (`showTooltipNow`). ❌ Its clearing listeners must
  keep exempting the wizard FOOTER, or that second press (and Enter, whose `keydown` beats its click) disarms the gate
  instead of passing it.
- **Long copy hides behind an info glyph, ❌ never back in the body**: step 4's cards and step 2's local option lead
  with a line, the rest in a `contentEl` tooltip.
- **Step 2's options are plain `RadioGroup` rows, and each piece's snippet is an a11y call**: helper text and the
  Recommended badge go in `itemInline` (inside the label, so a click on either picks the option), the info `<button>`
  in `itemTrailing`, ❌ never inside the `role="radio"` element.
- **Allow (FDA) requires a restart before advancing past step 1**: `fda_gate::FDA_PENDING` is set once at boot, and
  clearing it at runtime races the TCC popups it suppresses (5-10 stacked, once). Deny advances normally.
- **Step 1's live-grant poller calls `checkFullDiskAccessQuiet`, ❌ never `checkFullDiskAccess`**, which fires a TCC
  registration storm per denial. It runs only while Allow/Deny is open, and stops on grant.
- **The drive indexer and `volumes::list_locations`' icon fetches stay gated at boot** on
  `crate::fda_gate::is_fda_pending(...)`. Deny clears it (`startIndexingAfterFdaDecision()`), Allow on relaunch.
- **`StepBeta` / `StepOptional` reuse existing Settings wiring** (`UpdatesSection`'s email path, `<SettingSwitch>`),
  and that path POSTs only the email, ❌ never an install id.
- **Search's coverage note routes INTO step 1** (`coverage-note.ts::offersFullDiskAccess`): ❌ no second FDA prompt,
  ❌ never over a snapshot folder.

`DETAILS.md` owns the rest: the language escape hatch, the footer frame, the three-state FDA setting, no Escape
handler, the step-2 banners, and the `CMDR_FORCE_ONBOARDING` / `CMDR_MOCK_FDA` overrides. Read it first.
