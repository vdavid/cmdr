# AI provider setup

The "point Cmdr at an AI provider" flow: numbered steps, an API key, a connection check, a
model picker. Rendered in TWO places, owned here so neither forks it: the onboarding wizard's step 2
(`lib/onboarding/CloudProviderSetup.svelte`) and Settings › AI › Provider
(`lib/settings/sections/AiCloudSection.svelte`).

## Module map

`provider-setup.svelte.ts` (`ProviderSetupController`: all the state, the debounces, the race guards),
`provider-setup-plan.ts` (pure: preset → ordered steps), `ProviderSetupSteps.svelte` (the numbered list and its
controls). Presets live in `lib/settings/cloud-providers.ts`.

## Must-knows

- **A new provider is a `cloud-providers.ts` entry, ❌ never markup here.** Its required `setup` union answers what
  the steps need (`cloud` = sign-up + API-keys URLs, `local` = download + guide, `byoEndpoint` = neither), so a
  half-filled preset is a compile error. That's the fix for the Qwen bug: a private id→links map answered empty
  strings for the one provider nobody added, and BOTH link steps silently vanished.
- **`setup.kind === 'local'` is the only "is this local" test.** ❌ Don't re-add an `isLocal` boolean; two sources
  drift. Local providers get install + get-a-model steps and NO sign-up step (you download Ollama, you don't
  register).
- **Mount the controller with `untrack`.** `setProvider()` both reads and writes its own `$state`, so a tracked
  `$effect(() => controller.setProvider(id))` re-runs itself when a check lands and wipes what it just produced.
  Copy `CloudProviderSetup.svelte`'s effect.
- **Copy branches on a LITERAL message key** in `ProviderSetupSteps.svelte`, never one read off the plan:
  `desktop-i18n-trans-snippet-parity` can only match `<tag>`s to snippets when it can see the key. The plan says
  which steps and in what order; this file says what they read.
- **The key never comes back from the backend**, so the field starts empty behind a "your key is saved"
  placeholder and `keyIsSet` (from `getAiApiKeyStatus`) drives the gate. `docs/security.md` § "AI API keys".
- **Every async answer is compared against `providerId` before it lands.** A keychain read or a check for a
  provider the user already clicked away from is dropped, ❌ never rendered against the new one.
- **The connection status is NOT rendered here.** Each surface owns that block (the wizard a quiet line, Settings a
  row with a recheck button), and each uses its own message keys.

Architecture, the per-provider matrix, the URL evidence, and decisions: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
