/**
 * AI section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { EnumOption, SettingDefinitionSource } from '../types'
import { cloudProviderPresets } from '../cloud-providers'
import { formatInteger, formatMoney } from '#lib/intl/number-format.js'

/**
 * A token-count option whose label is the number itself, grouped for the reader's locale
 * (16,000 / 16 000 / 16.000). A getter, so it formats at read time rather than freezing the
 * locale that happened to be active when this module first loaded; the registry passes an
 * option with a literal `label` through unchanged, getter included.
 */
function tokenOption(tokens: number): EnumOption {
  return {
    value: String(tokens),
    get label() {
      return formatInteger(tokens)
    },
  }
}

/** A dollar-amount option whose label is the amount as money in the reader's locale ($2.00). A
 * getter for the same reason as {@link tokenOption}. The value is the amount in dollars, which
 * the backend reads as is (`settings::load_ask_cmdr_spend_pause_micros`). */
function dollarOption(dollars: number): EnumOption {
  return {
    value: String(dollars),
    get label() {
      return formatMoney(dollars, 'USD')
    },
  }
}

export const aiSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // AI › Provider
  // ========================================================================
  {
    id: 'ai.provider',
    section: ['AI', 'Provider'],
    labelKey: 'settings.ai.provider.label',
    descriptionKey: 'settings.ai.provider.description',
    keywordsKey: 'settings.ai.provider.keywords',
    terms: ['openai', 'anthropic', 'claude', 'gemini', 'llm', 'llama'],
    type: 'enum',
    default: 'off',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'off', labelKey: 'settings.ai.provider.opt.off' },
        { value: 'cloud', labelKey: 'settings.ai.provider.opt.cloud' },
        { value: 'local', labelKey: 'settings.ai.provider.opt.local' },
      ],
    },
  },
  {
    id: 'ai.cloudProvider',
    section: ['AI', 'Provider'],
    labelKey: 'settings.ai.cloudProvider.label',
    descriptionKey: 'settings.ai.cloudProvider.description',
    keywordsKey: 'settings.ai.cloudProvider.keywords',
    terms: [
      'openai',
      'anthropic',
      'groq',
      'together',
      'fireworks',
      'mistral',
      'ollama',
      'deepseek',
      'xai',
      'perplexity',
      'openrouter',
      'gemini',
      'azure',
      'lm-studio',
    ],
    type: 'enum',
    default: 'openai',
    component: 'select',
    constraints: {
      // Cloud-provider option labels are brand names (not translatable copy),
      // sourced from the provider preset table, not the catalog.
      options: cloudProviderPresets.map((p) => ({ value: p.id, label: p.name })),
    },
  },
  {
    id: 'ai.cloudProviderConfigs',
    section: ['AI', 'Provider'],
    labelKey: 'settings.ai.cloudProviderConfigs.label',
    descriptionKey: 'settings.ai.cloudProviderConfigs.description',
    type: 'string',
    default: '{}',
    component: 'text-input',
  },
  {
    // How hard each cloud model thinks, per provider and model: a JSON object keyed
    // `"<provider id>/<model>"` with `"recommended"`, `"off"`, or a level (`"low"` …
    // `"max"`). Rendered as the reasoning picker under the model in `AiCloudSection`, never as a
    // text field; the row's label is the provider's own term (`ai-effort.ts::effortLabelKey`),
    // and this `labelKey` is the fallback term, which search and MCP see. Read fresh per send by `load_ai_reasoning_efforts`, so no applier case; the
    // backend maps a pick the model can't take to the nearest it can
    // (`agent::chat::effort::resolve_effort`).
    id: 'ai.reasoningEfforts',
    section: ['AI', 'Provider'],
    labelKey: 'settings.ai.reasoningEfforts.label.thinkingLevel',
    descriptionKey: 'settings.ai.reasoningEfforts.description',
    keywordsKey: 'settings.ai.reasoningEfforts.keywords',
    type: 'string',
    default: '{}',
    component: 'text-input',
  },
  {
    id: 'ai.localContextSize',
    section: ['AI', 'Provider'],
    labelKey: 'settings.ai.localContextSize.label',
    descriptionKey: 'settings.ai.localContextSize.description',
    keywordsKey: 'settings.ai.localContextSize.keywords',
    type: 'enum',
    // 32,768 is the floor Ask Cmdr needs for one working turn
    // (`agent::chat::budget::MIN_LOCAL_CONTEXT_TOKENS`, which this default mirrors): the
    // system prompt plus the tool declarations cost ~7k tokens before the user says a
    // word, and one paged tool result can spend 8,000 more. Nothing smaller is offered, and
    // a smaller stored size from an earlier build no longer validates, so it reads as this
    // default instead of leaving a tester with a chat that can't complete a single message.
    default: '32768',
    component: 'select',
    constraints: {
      options: [tokenOption(32768), tokenOption(65536), tokenOption(131072), tokenOption(262144)],
    },
  },

  // ========================================================================
  // AI › Ask Cmdr
  //
  // The interactive-slot model override. Empty = use the model the shared `ai/`
  // provider is already configured with. The backend reads it fresh each send
  // (`load_ask_cmdr_interactive_model`), so it applies with no restart and needs no
  // `settings-applier` case (same pattern as the operation-log retention limits). The
  // on/off switch above it is a plain setting; consent to send anything to a cloud
  // service is NOT, it lives in `main.db` (`#lib/ai/cloud-consent.svelte.ts`).
  // ========================================================================
  {
    // Ask Cmdr's plain on/off. It grants no data flow: on Cloud, "Allow cloud AI" still
    // gates every call. The backend reads it fresh from `settings.json` per send
    // (`settings::load_ask_cmdr_switch`, absent = off), and the wake loop's cached
    // readiness hears about a change through `askCmdrEnabledChanged()` (settings-applier).
    // Existing installs get it set once at startup from the legacy opt-in
    // (`lib/ask-cmdr/ask-cmdr-enabled-mapping.ts`); new ones from the onboarding pick.
    id: 'askCmdr.enabled',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.enabled.label',
    descriptionKey: 'settings.askCmdr.enabled.description',
    keywordsKey: 'settings.askCmdr.enabled.keywords',
    terms: ['ask cmdr'],
    type: 'boolean',
    default: false,
    component: 'switch',
    // On Local it starts a proactive loop, so an MCP client mustn't be able to switch it on.
    mcpSettable: false,
  },
  {
    id: 'askCmdr.interactiveModel',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.interactiveModel.label',
    descriptionKey: 'settings.askCmdr.interactiveModel.description',
    keywordsKey: 'settings.askCmdr.interactiveModel.keywords',
    terms: ['ask cmdr', 'llm'],
    type: 'string',
    default: '',
    component: 'text-input',
  },
  {
    // How much of the thread one message may carry. `auto` spends the model's whole window
    // (the provider's report, else the family table in `agent::chat::budget`); a number is
    // an optional lower limit, to spend less, used as given even above the window we believe
    // the model has — the section warns instead of clamping, because our knowledge will be
    // wrong sometimes and the user may be right.
    // Presets rather than a free number field: the bounds are then unmisstateable, so
    // there's no below-minimum case and no validation copy. Read fresh per send by
    // `load_ask_cmdr_chat_memory_size`, so it applies to the next message and never moves a
    // turn already in flight. Its `settings-applier` case applies nothing for that reason:
    // it only nudges the slot-change timeline, so an open thread says why its next reply
    // carries a different amount of the chat.
    id: 'askCmdr.chatMemorySize',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.chatMemorySize.label',
    descriptionKey: 'settings.askCmdr.chatMemorySize.description',
    keywordsKey: 'settings.askCmdr.chatMemorySize.keywords',
    terms: ['ask cmdr'],
    type: 'enum',
    default: 'auto',
    component: 'select',
    constraints: {
      options: [
        { value: 'auto', labelKey: 'settings.askCmdr.chatMemorySize.opt.auto' },
        tokenOption(16000),
        tokenOption(32000),
        tokenOption(60000),
        tokenOption(128000),
        tokenOption(200000),
      ],
    },
  },
  {
    // Where one answer the user started pauses on its estimated spend and asks to keep going.
    // Read fresh per send by `load_ask_cmdr_spend_pause_micros`, so like the two above it applies
    // to the next message, never moves a turn in flight, and needs no `settings-applier` case.
    // The Rust loader mirrors the default ($2; why: `agent/chat/DETAILS.md` § Constants table),
    // since the store is sparse and an untouched row arrives as an absent key.
    id: 'askCmdr.spendPause',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.spendPause.label',
    descriptionKey: 'settings.askCmdr.spendPause.description',
    keywordsKey: 'settings.askCmdr.spendPause.keywords',
    terms: ['ask cmdr'],
    type: 'enum',
    default: '2',
    component: 'select',
    constraints: {
      options: [
        dollarOption(0.5),
        dollarOption(1),
        dollarOption(2),
        dollarOption(5),
        dollarOption(10),
        { value: 'never', labelKey: 'settings.askCmdr.spendPause.opt.never' },
      ],
    },
  },

  // ========================================================================
  // AI › Ask Cmdr › On its own
  //
  // The three rows behind the proactive loop (`agent::wake`). ⚠️ Unlike the two above,
  // these are NOT read fresh at send time: they drive a sleeping timer on the wake
  // loop's own thread, so each one needs a `settings-applier` case pushing
  // `askCmdrWakeSettingsChanged()`. Flipping `proactive` has to wake a parked
  // scheduler, and a change to `wakeDelay` has to re-arm the timer AND re-price the
  // rows already queued (the inbox merge is min-only, so a LENGTHENED delay would
  // otherwise never reach anything waiting).
  // ========================================================================
  {
    id: 'askCmdr.proactive',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.proactive.label',
    descriptionKey: 'settings.askCmdr.proactive.description',
    keywordsKey: 'settings.askCmdr.proactive.keywords',
    terms: ['ask cmdr'],
    type: 'boolean',
    // The middle tier between "no AI" and "AI that starts conversations", and the
    // fourth gate the scheduler checks beside consent, disk access, and a provider.
    // The Rust loader mirrors this default explicitly (`WakeSettings::from_parts`):
    // the store is sparse, so an untouched row reaches the backend as an absent key.
    // ⚠️ ON by default, which is the feature. The other three gates are what keep it
    // from doing anything for somebody who never opted into AI at all; this row is
    // the user's own "no thanks".
    default: true,
    component: 'switch',
  },
  {
    id: 'askCmdr.wakeDelay',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.wakeDelay.label',
    descriptionKey: 'settings.askCmdr.wakeDelay.description',
    keywordsKey: 'settings.askCmdr.wakeDelay.keywords',
    terms: ['ask cmdr'],
    type: 'number',
    // Seconds. Mirrors `agent::wake::DEFAULT_HOT_DELAY`, and `WAKE_DELAY_STOPS` in the
    // same Rust module mirrors `sliderStops` below; nothing enforces the pair
    // mechanically, so change both together.
    default: 5,
    component: 'slider',
    constraints: {
      // ⚠️ `stopsAreDiscrete`: the track runs over the stops' INDICES. A linear 5-to-7200
      // range would put the first three stops inside a single pixel. The stored value is
      // still the number of seconds, never the index.
      min: 5,
      max: 7200,
      step: 1,
      sliderStops: [5, 15, 30, 60, 120, 300, 900, 1800, 3600, 7200],
      stopsAreDiscrete: true,
    },
  },
  {
    id: 'askCmdr.wakeToast',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.wakeToast.label',
    descriptionKey: 'settings.askCmdr.wakeToast.description',
    keywordsKey: 'settings.askCmdr.wakeToast.keywords',
    terms: ['ask cmdr'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  // ========================================================================
  // AI › MCP server
  //
  // The Model Context Protocol server that lets external AI clients drive Cmdr.
  // Rendered by `McpServerSection.svelte`. (The `developer.mcp*` id prefix is a
  // stable persistence key; homing the setting under AI doesn't touch it.)
  // ========================================================================
  {
    id: 'developer.mcpEnabled',
    section: ['AI', 'MCP server'],
    labelKey: 'settings.developer.mcpEnabled.label',
    descriptionKey: 'settings.developer.mcpEnabled.description',
    keywordsKey: 'settings.developer.mcpEnabled.keywords',
    terms: ['mcp'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'developer.mcpPort',
    section: ['AI', 'MCP server'],
    labelKey: 'settings.developer.mcpPort.label',
    descriptionKey: 'settings.developer.mcpPort.description',
    keywordsKey: 'settings.developer.mcpPort.keywords',
    terms: ['mcp'],
    type: 'number',
    // 0 = ephemeral. The backend binds 127.0.0.1:0 and writes the actual port to
    // `<data_dir>/mcp.port` so external clients can discover it. Pinning a non-zero port
    // is still supported for tooling that needs a fixed target. See
    // `docs/tooling/instance-isolation.md` § "Per-resource breakdown" (Cmdr MCP HTTP port row).
    default: 0,
    component: 'number-input',
    constraints: {
      min: 0,
      max: 65535,
      step: 1,
    },
  },
]
