/**
 * AI section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { SettingDefinitionSource } from '../types'
import { cloudProviderPresets } from '../cloud-providers'

export const aiSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // AI
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
    default: '4096',
    component: 'select',
    constraints: {
      // Token-count option labels are plain numerals, not translatable copy.
      options: [
        { value: '2048', label: '2048' },
        { value: '4096', label: '4096' },
        { value: '8192', label: '8192' },
        { value: '16384', label: '16384' },
        { value: '32768', label: '32768' },
        { value: '65536', label: '65536' },
        { value: '131072', label: '131072' },
        { value: '262144', label: '262144' },
      ],
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
]
