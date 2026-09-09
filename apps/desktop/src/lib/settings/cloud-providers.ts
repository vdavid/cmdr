/**
 * Where a provider's setup steps send the user, and which shape those steps take.
 *
 * A union rather than optional URL fields, so a preset can't ship half-answered: a
 * `cloud` provider MUST name both destinations (the missing-`qwen` bug was a lookup
 * that silently fell back to empty strings and dropped both steps), a `local` one has
 * no account to sign up for and names a download plus a "get a model running" guide
 * instead, and `byoEndpoint` declares out loud that the user supplies everything.
 *
 * `kind` is also the single source of truth for "is this provider local"; there's no
 * separate boolean to drift from it.
 */
export type ProviderSetup =
  | { kind: 'cloud'; signupUrl: string; apiKeysUrl: string }
  | { kind: 'local'; downloadUrl: string; guideUrl: string }
  | { kind: 'byoEndpoint' }

export interface CloudProviderPreset {
  id: string
  name: string
  baseUrl: string
  defaultModel: string
  requiresApiKey: boolean
  supportsModelList: boolean
  /** How the setup steps guide the user to this provider. Required: see `ProviderSetup`. */
  setup: ProviderSetup
  description: string
}

/**
 * Every `setup` URL below was fetched on 2026-09-09 (curl, following redirects, browser
 * user-agent). Most answered 200 or an auth redirect; `platform.openai.com`,
 * `console.x.ai`, and `console.perplexity.ai` answer 403 to automated requests, so those
 * four are taken from the vendors' own current quickstart pages instead (docs.x.ai and
 * docs.perplexity.ai link them verbatim). A 403 here is bot filtering, never evidence
 * about the URL.
 */
export const cloudProviderPresets: CloudProviderPreset[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    defaultModel: 'gpt-4.1-mini',
    requiresApiKey: true,
    supportsModelList: true,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://platform.openai.com/signup',
      apiKeysUrl: 'https://platform.openai.com/api-keys',
    },
    description: 'The original ChatGPT provider. Widest model selection and ecosystem.',
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    baseUrl: 'https://api.anthropic.com/v1/',
    defaultModel: 'claude-sonnet-4-5',
    requiresApiKey: true,
    supportsModelList: false,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://platform.claude.com/login',
      apiKeysUrl: 'https://platform.claude.com/settings/keys',
    },
    description: 'Claude models via the native Anthropic API. Strong reasoning and safety.',
  },
  {
    id: 'google-gemini',
    name: 'Google Gemini',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai/',
    defaultModel: 'gemini-2.5-flash',
    requiresApiKey: true,
    supportsModelList: true,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://aistudio.google.com/',
      apiKeysUrl: 'https://aistudio.google.com/app/apikey',
    },
    description: 'Google DeepMind models with 1M token context. Free tier available.',
  },
  {
    id: 'groq',
    name: 'Groq',
    baseUrl: 'https://api.groq.com/openai/v1',
    // Groq retired the whole Llama line on 2026-08-16 (console.groq.com/docs/deprecations);
    // this is its named replacement for the 70B, and a current production model.
    defaultModel: 'openai/gpt-oss-120b',
    requiresApiKey: true,
    supportsModelList: true,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://console.groq.com/login',
      apiKeysUrl: 'https://console.groq.com/keys',
    },
    description: 'Ultra-fast inference on custom LPU hardware. Best for low-latency use cases.',
  },
  {
    id: 'together-ai',
    name: 'Together AI',
    baseUrl: 'https://api.together.xyz/v1',
    defaultModel: 'meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8',
    requiresApiKey: true,
    supportsModelList: true,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://api.together.xyz/',
      apiKeysUrl: 'https://api.together.xyz/settings/api-keys',
    },
    description: 'Wide selection of open-source models with competitive pricing.',
  },
  {
    id: 'fireworks-ai',
    name: 'Fireworks AI',
    baseUrl: 'https://api.fireworks.ai/inference/v1',
    // The Llama 3.3 id this used to name 404s: Fireworks no longer serves it (verified
    // 2026-09-04 against `GET /inference/v1/models`).
    defaultModel: 'accounts/fireworks/models/glm-5p3',
    requiresApiKey: true,
    supportsModelList: true,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://app.fireworks.ai/login',
      apiKeysUrl: 'https://app.fireworks.ai/settings/users/api-keys',
    },
    description: 'Fast open-source model inference. Optimized for production workloads.',
  },
  {
    id: 'mistral',
    name: 'Mistral AI',
    baseUrl: 'https://api.mistral.ai/v1',
    defaultModel: 'mistral-small-latest',
    requiresApiKey: true,
    supportsModelList: true,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://console.mistral.ai/',
      apiKeysUrl: 'https://console.mistral.ai/api-keys/',
    },
    description: 'European AI lab. Efficient models with strong multilingual support.',
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    baseUrl: 'https://openrouter.ai/api/v1',
    defaultModel: 'openai/gpt-4.1-mini',
    requiresApiKey: true,
    supportsModelList: true,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://openrouter.ai/',
      apiKeysUrl: 'https://openrouter.ai/keys',
    },
    description: 'Unified gateway to 290+ models from all major providers. Single API key.',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/v1',
    defaultModel: 'deepseek-chat',
    requiresApiKey: true,
    supportsModelList: true,
    setup: {
      kind: 'cloud',
      signupUrl: 'https://platform.deepseek.com/',
      apiKeysUrl: 'https://platform.deepseek.com/api_keys',
    },
    description: 'Strong coding and reasoning at low cost.',
  },
  {
    id: 'qwen',
    name: 'Qwen',
    baseUrl: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1',
    defaultModel: 'qwen-plus',
    requiresApiKey: true,
    supportsModelList: true,
    // A Model Studio key is bound to the region it was created in, and `dashscope-intl`
    // above is the Singapore (ap-southeast-1) endpoint, so the key page has to be the
    // ap-southeast-1 one. Alibaba's own "How to obtain an API key" page names this exact
    // URL (verified 2026-09-09).
    setup: {
      kind: 'cloud',
      signupUrl: 'https://www.alibabacloud.com/en/product/modelstudio',
      apiKeysUrl: 'https://modelstudio.console.alibabacloud.com/ap-southeast-1?tab=globalset#/efm/api_key',
    },
    description: 'Qwen models through Alibaba Cloud’s OpenAI-compatible API.',
  },
  {
    id: 'xai',
    name: 'xAI',
    baseUrl: 'https://api.x.ai/v1',
    defaultModel: 'grok-3-mini-fast',
    requiresApiKey: true,
    supportsModelList: true,
    // Both are the links xAI's own quickstart hands out (docs.x.ai/developers/quickstart,
    // read 2026-09-09), minus its utm parameters. The console scopes keys under a team,
    // which is why the key page carries a team segment.
    setup: {
      kind: 'cloud',
      signupUrl: 'https://console.x.ai/login?mode=sign-up',
      apiKeysUrl: 'https://console.x.ai/team/default/api-keys',
    },
    description: 'Grok models from xAI. Fast reasoning with real-time knowledge.',
  },
  {
    id: 'perplexity',
    name: 'Perplexity',
    baseUrl: 'https://api.perplexity.ai',
    defaultModel: 'sonar',
    requiresApiKey: true,
    supportsModelList: false,
    // The API console is a separate site from perplexity.ai itself; the key page is the one
    // docs.perplexity.ai's quickstart links (read 2026-09-09).
    setup: {
      kind: 'cloud',
      signupUrl: 'https://console.perplexity.ai/',
      apiKeysUrl: 'https://console.perplexity.ai/project/keys',
    },
    description: 'Search-augmented AI. Responses include web citations.',
  },
  {
    id: 'azure-openai',
    name: 'Azure OpenAI',
    baseUrl: 'https://{resource-name}.openai.azure.com/openai/v1',
    defaultModel: 'gpt-4.1-mini',
    requiresApiKey: true,
    supportsModelList: true,
    // The endpoint above is Azure's v1 API, which needs no `api-version` parameter and takes
    // the same `Authorization: Bearer <key>` every other preset uses, so Cmdr's plain
    // OpenAI-compatible client reaches it unchanged. The catch is the model field: on Azure
    // it's the DEPLOYMENT name you chose, not the base model id. Both confirmed against
    // Microsoft's "Azure OpenAI v1 API" page (learn.microsoft.com, read 2026-09-09).
    setup: {
      kind: 'cloud',
      signupUrl: 'https://azure.microsoft.com/products/ai-foundry/models/openai/',
      apiKeysUrl: 'https://portal.azure.com/',
    },
    description: 'OpenAI models hosted on Azure. Enterprise compliance and data residency.',
  },
  {
    id: 'ollama',
    name: 'Ollama',
    baseUrl: 'http://localhost:11434/v1',
    defaultModel: 'llama3.2',
    requiresApiKey: false,
    supportsModelList: true,
    setup: {
      kind: 'local',
      downloadUrl: 'https://ollama.com/download',
      guideUrl: 'https://ollama.com/library',
    },
    description: 'Run open-source models locally. Easy CLI-based model management.',
  },
  {
    id: 'lm-studio',
    name: 'LM Studio',
    baseUrl: 'http://localhost:1234/v1',
    defaultModel: 'loaded-model',
    requiresApiKey: false,
    supportsModelList: true,
    setup: {
      kind: 'local',
      downloadUrl: 'https://lmstudio.ai/',
      guideUrl: 'https://lmstudio.ai/docs/app/api',
    },
    description: 'Desktop app for running local models. GUI-based model discovery.',
  },
  {
    id: 'custom',
    name: 'Custom',
    baseUrl: '',
    defaultModel: '',
    requiresApiKey: true,
    supportsModelList: true,
    setup: { kind: 'byoEndpoint' },
    description: 'Any OpenAI-compatible API endpoint.',
  },
]

export function getCloudProvider(id: string): CloudProviderPreset | undefined {
  return cloudProviderPresets.find((p) => p.id === id)
}

/** Non-secret per-provider config persisted in `settings.json`. API keys live in the OS secret
 *  store, written via `saveAiApiKey` and never readable back from a window, not here. */
export interface CloudProviderConfig {
  model: string
  baseUrl?: string // only stored for 'custom' and 'azure-openai'
}

export function getProviderConfigs(raw: string): Partial<Record<string, CloudProviderConfig>> {
  try {
    return JSON.parse(raw) as Partial<Record<string, CloudProviderConfig>>
  } catch {
    return {}
  }
}

export function setProviderConfig(raw: string, providerId: string, config: CloudProviderConfig): string {
  const existing = getProviderConfigs(raw)
  const configs = Object.fromEntries(
    Object.entries(existing).filter((entry): entry is [string, CloudProviderConfig] => entry[1] !== undefined),
  )
  configs[providerId] = config
  return JSON.stringify(configs)
}

/** Resolve the effective non-secret config for the current cloud provider. The API key isn't part of
 *  it: the backend reads that from the OS secret store itself, keyed by provider id. */
export function resolveCloudConfig(
  cloudProviderId: string,
  configsJson: string,
): {
  baseUrl: string
  model: string
} {
  const preset = getCloudProvider(cloudProviderId)
  const configs = getProviderConfigs(configsJson)
  const providerConfig = configs[cloudProviderId]

  const baseUrl =
    cloudProviderId === 'custom' || cloudProviderId === 'azure-openai'
      ? (providerConfig?.baseUrl ?? preset?.baseUrl ?? '')
      : (preset?.baseUrl ?? '')

  return {
    baseUrl,
    model: providerConfig?.model ?? preset?.defaultModel ?? '',
  }
}
