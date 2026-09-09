# AI provider setup details

Pull-tier docs for `apps/desktop/src/lib/ai-provider-setup/`: architecture, the per-provider matrix, and decision
rationale. Must-know invariants live in `CLAUDE.md`.

## Why this module exists

Two surfaces set an AI provider up, and until this module they did it twice:

- The onboarding wizard's step 2 had numbered per-provider steps with sign-up and API-key links, per-step checkmarks,
  and inline status (`lib/onboarding/CloudProviderSetup.svelte`).
- Settings › AI › Provider had three flat rows (endpoint, key, model) plus a status line, and NO guidance at all: a user
  who skipped AI during onboarding and came back later got a field labelled "API key" with nowhere to get one
  (`lib/settings/sections/AiCloudSection.svelte`).

Both independently implemented the same ~250 lines: the key debounce-and-save, `hasCheckableConfig`,
`triggerConnectionCheck`, `resolvedBaseUrl`, `persistApiKey`, the provider-switch race guards, and the same seven-state
`ConnectionStatus` union. They had already drifted.

## The split: controller, plan, presentation

**`ProviderSetupController` (`provider-setup.svelte.ts`)** owns everything stateful. It's a class rather than a factory
returning an object literal so the private fields stay private (`#`) while the getters read as plain properties at the
call sites. Runes force the `.svelte.ts` extension.

**`buildSetupPlan` (`provider-setup-plan.ts`)** is pure: preset in, ordered `SetupStep[]` out. Being Svelte-free is the
point — the whole 16-provider matrix is asserted in `provider-setup-plan.test.ts` without mounting anything, and that's
the test that would have caught the Qwen gap.

**`ProviderSetupSteps.svelte`** renders the plan. It owns the wording, because `desktop-i18n-trans-snippet-parity`
resolves a `<Trans>` call site only when its `key` is a literal; a `key={step.messageKey}` would be reported as skipped
and the tag/snippet pairing would go unchecked. So the plan emits distinct step ids (`ollamaModel` vs `lmStudioServer`)
and the component branches on them.

**Decision / a shared controller plus a shared steps component, not one component with a `variant` prop.** The two
chromes genuinely differ: Settings frames the service picker in a `SettingRow`, carries recheck buttons, mirrors a
secret-store failure into a toast, and shows the Ask Cmdr model-override note; the wizard has a provider header and a
single quiet status line and never blocks advance. A `variant` prop would have carried all of that as branches inside
one file. What the two actually share is the state machine and the numbered list, and those are what moved.

**Decision / the status block stays per-surface.** Merging it would have orphaned one of the two message-key sets
(`ai.cloud.status*` vs `onboarding.cloudSetup.status.*`), and the sets differ in more than wording: Settings offers
"Recheck" and "Test connection" buttons that the wizard deliberately doesn't. So each surface renders its own, reading
`controller.status` / `controller.error`.

**Message keys keep their `onboarding.cloudSetup.` prefix** even though the component is shared now. They carry 10
locales' translations plus stored `@key.sourceHash` values; renaming them would be pure churn for a nicer prefix. A
later i18n pass is welcome to rename them across all catalogs at once.

## What each surface kept

The merge had to preserve behaviour that existed in only one of the two. Where each landed:

- **Model-list cache** (`$lib/settings/ai-model-cache`, keyed on the backend's key fingerprint so it misses after a key
  change): moved into the controller, so the wizard now gets it too. The digest is guarded: a runtime without Web Crypto
  degrades to "always refetch", never to "never check".
- **Secret errors as a persistent toast**: an `onSecretErrorChange` option. Settings passes it; the wizard doesn't, so
  it stays at the inline message. Both render `controller.secretError` inline.
- **`isE2eRun()` suppression of the auto-check on open**: in the controller, so it now covers the wizard as well. An
  automated run has no real provider to answer, and a cache hit still serves everywhere. This is why unit tests that
  want the everyday path have to mock `$lib/app-mode`: `vitest.config.ts` bakes `__CMDR_I18N_CAPTURE__` in, so the real
  `isE2eRun()` answers true in every unit test.
- **`pushConfigToBackend()` after a key persist**: an `onKeyPersisted` option. Settings passes it; the wizard pushes
  once from `StepAi.persist()` instead, so it doesn't push a provider the user hasn't confirmed yet.
- **The `ai.cloud.askCmdrOverrideHint` note** and the settings-search `shouldShow` gating: stayed in
  `AiCloudSection.svelte`.
- **The immediate (no-debounce) check when a stored key is found on open**: in the controller, for both. Settings used
  to wait out a 1 s debounce there for no reason; nothing is being typed on open.
- **Numbered steps with per-step checkmarks, and the console links**: now in `ProviderSetupSteps.svelte`, so Settings
  shows them too.

**Settings search still resolves the same way.** The endpoint / key / model rows were three `SettingRow`s all carrying
the SAME id, `ai.cloudProviderConfigs`; the steps block that replaced them is gated on
`shouldShow('ai.cloudProviderConfigs')`, so a query matching that registry entry (its label and description name "API
keys and model settings") lights the section exactly as before. Nothing deep-links to
`settingAnchorId('ai.cloudProviderConfigs')`, so dropping the rows costs no anchor.

## The per-provider matrix

`setup.kind` on the preset picks the opening pair of steps; the rest follows from `requiresApiKey` and whether the
endpoint is the user's to fill in.

- **Cloud (12 presets)**: signup → createKey → apiKey → model
- **Azure OpenAI**: signup → createKey → endpoint (+hint) → apiKey → model (+hint)
- **Custom**: endpoint → apiKey → model
- **Ollama**: install → ollamaModel → model
- **LM Studio**: install → lmStudioServer → model

### Azure OpenAI needs guidance, not fields

Azure is the one provider whose setup differs structurally, and the question was whether it needs config Cmdr can't
express. It doesn't. `CloudProviderConfig` persists a `model` and an optional `baseUrl`, and the backend
(`src-tauri/src/ai/`) sends a plain `Authorization: Bearer <key>` at `{baseUrl}/chat/completions`, with the connection
check probing `{baseUrl}/models`. Azure's **v1 API** — the shape the preset's
`https://{resource-name}.openai.azure.com/openai/v1` endpoint names — takes exactly that: no `api-version` query
parameter, and an OpenAI-client-compatible bearer key (verified against Microsoft's "Azure OpenAI v1 API" page,
learn.microsoft.com, read 2026-09-09). So ❌ no `api-version` field and no deployment field: both would be UI for a
value nothing reads.

What Azure users actually get wrong is covered by two hints instead:

- the endpoint is a TEMPLATE whose placeholder has to be replaced with their resource name, and
- the model field wants their **deployment** name, which Azure lets them choose independently of the model it runs.

### Where the URLs come from

Every `setup` URL was fetched on 2026-09-09 (curl, redirects followed, browser user-agent). Most answered 200 or an auth
redirect. Three hosts answer 403 to any automated request — `platform.openai.com`, `console.x.ai`,
`console.perplexity.ai` — so their URLs come from the vendors' own current quickstarts instead
(docs.x.ai/developers/quickstart and docs.perplexity.ai's quickstart link them verbatim, minus utm parameters). A 403
from those hosts is bot filtering and says nothing either way about the URL.

Three entries changed beyond adding the missing one:

- **Qwen** had no entry at all. Its `dashscope-intl` endpoint is the Singapore region, and a Model Studio key is bound
  to the region it was made in, so the key link is the `ap-southeast-1` console page Alibaba's own "How to obtain an API
  key" page names.
- **LM Studio**'s docs link (`lmstudio.ai/docs/local-server`) 404s; `lmstudio.ai/docs/app/api` is the live page.
- **xAI** pointed both links at the console root; the console scopes keys under a team, so the key page carries a team
  segment. **Perplexity** pointed both at `perplexity.ai/settings/api`; the API console is a separate site
  (`console.perplexity.ai`).
