export interface CloudProviderPreset {
  id: string
  name: string
  baseUrl: string
  defaultModel: string
  requiresApiKey: boolean
  supportsModelList: boolean
  isLocal: boolean
  description: string
}

export const cloudProviderPresets: CloudProviderPreset[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    defaultModel: 'gpt-4.1-mini',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'The original ChatGPT provider. Widest model selection and ecosystem.',
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    baseUrl: 'https://api.anthropic.com/v1/',
    defaultModel: 'claude-sonnet-4-5',
    requiresApiKey: true,
    supportsModelList: false,
    isLocal: false,
    description: 'Claude models via OpenAI-compatible endpoint. Strong reasoning and safety.',
  },
  {
    id: 'google-gemini',
    name: 'Google Gemini',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai/',
    defaultModel: 'gemini-2.5-flash',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'Google DeepMind models with 1M token context. Free tier available.',
  },
  {
    id: 'groq',
    name: 'Groq',
    baseUrl: 'https://api.groq.com/openai/v1',
    defaultModel: 'llama-3.3-70b-versatile',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'Ultra-fast inference on custom LPU hardware. Best for low-latency use cases.',
  },
  {
    id: 'together-ai',
    name: 'Together AI',
    baseUrl: 'https://api.together.xyz/v1',
    defaultModel: 'meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'Wide selection of open-source models with competitive pricing.',
  },
  {
    id: 'fireworks-ai',
    name: 'Fireworks AI',
    baseUrl: 'https://api.fireworks.ai/inference/v1',
    defaultModel: 'accounts/fireworks/models/llama-v3p3-70b-instruct',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'Fast open-source model inference. Optimized for production workloads.',
  },
  {
    id: 'mistral',
    name: 'Mistral AI',
    baseUrl: 'https://api.mistral.ai/v1',
    defaultModel: 'mistral-small-latest',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'European AI lab. Efficient models with strong multilingual support.',
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    baseUrl: 'https://openrouter.ai/api/v1',
    defaultModel: 'openai/gpt-4.1-mini',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'Unified gateway to 290+ models from all major providers. Single API key.',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/v1',
    defaultModel: 'deepseek-chat',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'Strong coding and reasoning at low cost.',
  },
  {
    id: 'xai',
    name: 'xAI',
    baseUrl: 'https://api.x.ai/v1',
    defaultModel: 'grok-3-mini-fast',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'Grok models from xAI. Fast reasoning with real-time knowledge.',
  },
  {
    id: 'perplexity',
    name: 'Perplexity',
    baseUrl: 'https://api.perplexity.ai',
    defaultModel: 'sonar',
    requiresApiKey: true,
    supportsModelList: false,
    isLocal: false,
    description: 'Search-augmented AI. Responses include web citations.',
  },
  {
    id: 'azure-openai',
    name: 'Azure OpenAI',
    baseUrl: 'https://{resource-name}.openai.azure.com/openai/v1',
    defaultModel: 'gpt-4.1-mini',
    requiresApiKey: true,
    supportsModelList: true,
    isLocal: false,
    description: 'OpenAI models hosted on Azure. Enterprise compliance and data residency.',
  },
  {
    id: 'ollama',
    name: 'Ollama',
    baseUrl: 'http://localhost:11434/v1',
    defaultModel: 'llama3.2',
    requiresApiKey: false,
    supportsModelList: true,
    isLocal: true,
    description: 'Run open-source models locally. Easy CLI-based model management.',
  },
  {
    id: 'lm-studio',
    name: 'LM Studio',
    baseUrl: 'http://localhost:1234/v1',
    defaultModel: 'loaded-model',
    requiresApiKey: false,
    supportsModelList: true,
    isLocal: true,
    description: 'Desktop app for running local models. GUI-based model discovery.',
  },
  {
    id: 'custom',
    name: 'Custom',
    baseUrl: '',
    defaultModel: '',
    requiresApiKey: false,
    supportsModelList: true,
    isLocal: false,
    description: 'Any OpenAI-compatible API endpoint.',
  },
]

export function getCloudProvider(id: string): CloudProviderPreset | undefined {
  return cloudProviderPresets.find((p) => p.id === id)
}

export interface CloudProviderConfig {
  apiKey: string
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

/** Resolve the effective config for the current cloud provider. */
export function resolveCloudConfig(
  cloudProviderId: string,
  configsJson: string,
): {
  apiKey: string
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
    apiKey: providerConfig?.apiKey ?? '',
    baseUrl,
    model: providerConfig?.model ?? preset?.defaultModel ?? '',
  }
}
