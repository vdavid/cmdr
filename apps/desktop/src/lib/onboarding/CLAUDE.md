# Onboarding module

First-launch consent in the `OnboardingWizard` soft sheet. Flow: FDA (1, macOS only) → AI (2) → Open beta, carrying the
analytics disclosure and the terms (3) → Optional settings (4). Linux starts at step 2.

## Module map

`OnboardingWizard` (shell) + `OnboardingStepShell` (per-step frame), `StepFda` / `StepAi` / `StepBeta` / `StepOptional`,
`CloudProviderPicker` / `CloudProviderSetup`, `OnboardingLanguagePicker`, `onboarding-state.svelte.ts` (state machine),
`fda-status.svelte.ts` (the reactive FDA fact) + `FdaBadge.svelte` (rendered by `$lib/status-corner/`),
`local-download-notice.ts` (failed model download → post-wizard toast).

## Must-knows

- **The frame is the FOOTER**, a `1fr auto 1fr` grid: Back, the language picker, the centred dots, the forward buttons.
  The picker is frame too, ❌ never a step: a first launch can land in a language the user can't read with every way out
  labeled in it. DETAILS § "The frame lives in the footer".
- **The per-provider setup steps live in `$lib/ai-provider-setup/`**, shared with Settings › AI › Provider: a provider
  or copy change goes there, ❌ never here.
- **Step 3 is non-skippable, and its checklist ticks report FACTS**: an opt-out analytics default is fair consent only
  if everyone saw it, so ❌ no skip-to-finish, and the first row's tip must keep covering crash reports. DETAILS § "The
  checklist".
- **Step 3's terms checkbox gates both footer buttons.** ❌ Never pre-tick or route around it. Unticked, they take
  `blockedReason`, ❌ not `disabled`, so a press still reveals the box, focused with `preventScroll: true`.
- **"No AI" turns off FOUR things**: `ai.provider`, cloud consent (`main.db`), `askCmdr.proactive`, `askCmdr.enabled`.
  ❌ Cloud or local NEVER grants cloud consent. DETAILS § "What 'off' turns off".
- **A preselect the MDM policy made is ❌ never an answer**: unless the person picked, `StepAi` writes none of the four,
  or the policy's `off` erases their own choice. DETAILS § "Under the organization's AI policy".
- **Step 2's footer notes confirm once, ❌ never block**: Cloud with Allow cloud AI off (setup locked), else Cloud with
  no stored key, warns on the first Next and passes on the second. ❌ Its clearing listeners must keep exempting the
  wizard FOOTER, or that second press disarms the gate.
- **Steps 3 and 4 ARE Settings surfaces**: `<SectionCard>` + `<SettingRow>` + `<SettingSwitch>`, plus `UpdatesSection`'s
  email path (which POSTs only the email, ❌ never an install id). ❌ Never hand-roll a frame here.
- **Long copy hides behind an `<InfoTip>` or a fold, ❌ never in the body.**
- **Step 2's `RadioGroup` keeps every interactive bit in `itemTrailing`**, ❌ never inside the `role="radio"`.
- **A closed FDA gate means step 1 is on screen, nothing less**: a launch skipping the wizard opens it
  (`startup-gates.ts`), or launch work defers unexplained. ❌ Allow still needs a restart before step 1 advances
  (`FDA_PENDING` is boot-only: clearing it at runtime stacks TCC popups). Deny advances normally.
- **Step 1's live-grant poller calls `checkFullDiskAccessQuiet`, ❌ never `checkFullDiskAccess`** (a TCC registration
  storm per denial).
- **Search's coverage note routes INTO step 1** (`coverage-note.ts::offersFullDiskAccess`): ❌ no second FDA prompt, ❌
  never over a snapshot folder.
- **Ask `fda-status.svelte.ts`, ❌ never probe FDA yourself.** One reactive answer, so the badge and an error message
  can't disagree. It probes QUIETLY, holds `null` until the first answer (❌ never render that as "no access"), and a
  failed probe KEEPS the last one. The badge hides on step 1, its own destination (`isWizardOnFdaStep()`). DETAILS §§
  "Making a missing grant visible", "What an error message may add about it".

The language escape hatch, the FDA boot gate, the organization's AI policy, and the dev overrides: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
