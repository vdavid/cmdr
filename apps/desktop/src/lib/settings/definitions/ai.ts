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

  // ========================================================================
  // AI › Image search
  //
  // On-device image-content (OCR) search. Runs entirely on the user's Mac via
  // Apple's Vision framework — no cloud, no AI provider, no API key — so it
  // lives under AI but stands apart from the provider-backed features above.
  // Rendered by `ImageSearchSection.svelte`; only `mediaIndex.enabled` is a
  // visible row, the rest back the bespoke slider / network-volume components.
  // ========================================================================
  {
    // Master toggle for image-content (OCR) indexing. Off by default; live-applied to
    // the `media_index` backend scheduler via `set_image_index_enabled`. Its own card
    // in `ImageSearchSection.svelte`, titled by `cardKey`.
    id: 'mediaIndex.enabled',
    section: ['AI', 'Image search'],
    labelKey: 'settings.mediaIndex.enabled.label',
    descriptionKey: 'settings.mediaIndex.enabled.description',
    cardKey: 'settings.mediaIndex.card',
    keywords: ['image', 'photo', 'ocr', 'text', 'search', 'index', 'picture', 'screenshot', 'content'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    // Internal (FE-owned): JSON array of volume ids opted into background network (SMB)
    // image enrichment (network enrichment). Off by default per volume; the per-network-volume rows in
    // `ImageSearchSection`'s "Image search" card toggle it, persisting here AND
    // calling `media_index_set_network_volume_enabled`. Read by the Rust loader as an array.
    id: 'mediaIndex.networkVolumes',
    section: ['AI', 'Image search'],
    labelKey: 'settings.mediaIndex.networkVolumes.label',
    descriptionKey: 'settings.mediaIndex.networkVolumes.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // Internal (FE-owned): JSON array of volume ids marked "always index" (enrich
    // regardless of importance). Toggled by the per-network-volume rows; persisted here
    // AND pushed via `media_index_set_always_index_volume`.
    id: 'mediaIndex.alwaysIndexVolumes',
    section: ['AI', 'Image search'],
    labelKey: 'settings.mediaIndex.alwaysIndexVolumes.label',
    descriptionKey: 'settings.mediaIndex.alwaysIndexVolumes.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // Internal (FE-owned): JSON array of absolute OS-mount folder paths marked "always
    // index". Set by the per-folder override; persisted here AND pushed via
    // `media_index_set_always_index_folder`.
    id: 'mediaIndex.alwaysIndexFolders',
    section: ['AI', 'Image search'],
    labelKey: 'settings.mediaIndex.alwaysIndexFolders.label',
    descriptionKey: 'settings.mediaIndex.alwaysIndexFolders.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // Internal (FE-owned): JSON array of absolute OS folder paths EXCLUDED from image
    // indexing (the privacy veto). Set by the folder context-menu "Don't index images
    // in this folder" item; persisted here AND pushed via `media_index_set_excluded_folder`
    // (which also retro-deletes the folder's existing rows). Read by the Rust loader as
    // an array.
    id: 'mediaIndex.excludedFolders',
    section: ['AI', 'Image search'],
    labelKey: 'settings.mediaIndex.excludedFolders.label',
    descriptionKey: 'settings.mediaIndex.excludedFolders.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // The image-index importance threshold (`0.0..=1.0`): the lowest folder-importance
    // level the scheduler enriches. Rendered as named buckets by the bespoke
    // `MediaIndexImportanceSlider.svelte` inside the "Image search" card (not an auto
    // row), so `hidden`. Default `0.0` matches the backend `DEFAULT_IMPORTANCE_THRESHOLD`
    // (enrich every scored folder — non-regressive vs the OCR slice, junk is floored out anyway), so
    // the UI and a sparse (unpersisted) store agree without eagerly writing a default.
    // Live-applied via the `settings-applier.ts` passthrough → `media_index_set_importance_threshold`.
    id: 'mediaIndex.importanceThreshold',
    section: ['AI', 'Image search'],
    labelKey: 'settings.mediaIndex.importanceThreshold.label',
    descriptionKey: 'settings.mediaIndex.importanceThreshold.description',
    keywords: ['image', 'photo', 'index', 'importance', 'folders', 'coverage', 'depth'],
    type: 'number',
    default: 0,
    hidden: true,
  },
]
