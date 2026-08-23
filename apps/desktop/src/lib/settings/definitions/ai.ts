/**
 * AI section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { EnumOption, SettingDefinitionSource } from '../types'
import { cloudProviderPresets } from '../cloud-providers'
import { formatInteger } from '$lib/intl/number-format'

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

export const aiSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // AI › Provider
  // ========================================================================
  {
    id: 'ai.provider',
    section: ['AI', 'Provider'],
    labelKey: 'settings.ai.provider.label',
    descriptionKey: 'settings.ai.provider.description',
    keywords: ['ai', 'provider', 'cloud', 'openai', 'anthropic', 'claude', 'gemini', 'local', 'llm', 'off', 'model'],
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
    keywords: [
      'cloud',
      'provider',
      'service',
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
      'custom',
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
    keywords: [],
    type: 'string',
    default: '{}',
    component: 'text-input',
  },
  {
    id: 'ai.localContextSize',
    section: ['AI', 'Provider'],
    labelKey: 'settings.ai.localContextSize.label',
    descriptionKey: 'settings.ai.localContextSize.description',
    keywords: ['context', 'window', 'tokens', 'memory', 'size', 'local'],
    type: 'enum',
    // 16,384 is the floor Ask Cmdr needs for one working turn
    // (`agent::chat::budget::MIN_LOCAL_CONTEXT_TOKENS`, which this default mirrors): the
    // system prompt plus the tool declarations cost ~3,124 tokens before the user says a
    // word. Nothing smaller is offered, and a stored 2,048 / 4,096 / 8,192 from an earlier
    // build no longer validates, so it reads as this default instead of leaving a tester
    // with a chat that can't complete a single message.
    default: '16384',
    component: 'select',
    constraints: {
      options: [tokenOption(16384), tokenOption(32768), tokenOption(65536), tokenOption(131072), tokenOption(262144)],
    },
  },

  // ========================================================================
  // AI › Ask Cmdr
  //
  // The interactive-slot model override. Empty = use the model the shared `ai/`
  // provider is already configured with. The backend reads it fresh each send
  // (`load_ask_cmdr_interactive_model`), so it applies with no restart and needs no
  // `settings-applier` case (same pattern as the operation-log retention limits). The
  // enable/consent state is NOT a setting — it lives in `main.db` (agent state), driven
  // by `AskCmdrSection.svelte` via the consent commands.
  // ========================================================================
  {
    id: 'askCmdr.interactiveModel',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.interactiveModel.label',
    descriptionKey: 'settings.askCmdr.interactiveModel.description',
    keywords: ['ask cmdr', 'ai', 'chat', 'assistant', 'model', 'llm', 'interactive', 'slot'],
    type: 'string',
    default: '',
    component: 'text-input',
  },
  {
    // How much of the thread one message may carry. `auto` follows the model's window (the
    // family table in `agent::chat::budget`); a number overrides it and is used as given,
    // even above the window we believe the model has — the section warns instead of
    // clamping, because that table will be wrong sometimes and the user may be right.
    // Presets rather than a free number field: the bounds are then unmisstateable, so
    // there's no below-minimum case and no validation copy. Read fresh per send by
    // `load_ask_cmdr_chat_memory_size`, so it applies to the next message with no
    // `settings-applier` case and never moves a turn already in flight.
    id: 'askCmdr.chatMemorySize',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.chatMemorySize.label',
    descriptionKey: 'settings.askCmdr.chatMemorySize.description',
    keywords: ['ask cmdr', 'chat', 'memory', 'context', 'window', 'tokens', 'size', 'budget', 'history'],
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
    keywords: ['ask cmdr', 'proactive', 'suggestions', 'automatic', 'notice', 'watch', 'wake', 'ai'],
    type: 'boolean',
    // The middle tier between "no AI" and "AI that starts conversations", and the
    // fourth gate the scheduler checks beside consent, disk access, and a provider.
    // The Rust loader mirrors this default explicitly (`WakeSettings::from_parts`):
    // the store is sparse, so an untouched row reaches the backend as an absent key.
    default: false,
    component: 'switch',
  },
  {
    id: 'askCmdr.wakeDelay',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.wakeDelay.label',
    descriptionKey: 'settings.askCmdr.wakeDelay.description',
    keywords: ['ask cmdr', 'delay', 'cadence', 'wait', 'often', 'frequency', 'calm', 'quiet', 'proactive'],
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
    keywords: ['ask cmdr', 'toast', 'notice', 'notification', 'suggestion', 'proactive', 'alert'],
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
    keywords: ['mcp', 'server', 'ai', 'assistant', 'protocol', 'model'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'developer.mcpPort',
    section: ['AI', 'MCP server'],
    labelKey: 'settings.developer.mcpPort.label',
    descriptionKey: 'settings.developer.mcpPort.description',
    keywords: ['port', 'mcp', 'network', 'ephemeral'],
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
